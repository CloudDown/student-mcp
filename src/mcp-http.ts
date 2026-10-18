import { timingSafeEqual } from "node:crypto";
import type { IncomingMessage, ServerResponse } from "node:http";
import { randomUUID } from "node:crypto";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/streamableHttp.js";
import { readSettings } from "./config.js";
import { createAggregateServer } from "./server.js";
import { activeServiceStates } from "./services.js";

const MAX_BODY = 2 * 1024 * 1024;

/** Constant-time token comparison so request timing leaks nothing. */
export function tokenMatches(expected: string, provided: string | undefined): boolean {
  if (!provided || !expected) return false;
  const left = Buffer.from(expected);
  const right = Buffer.from(provided);
  return left.length === right.length && timingSafeEqual(left, right);
}

export function jsonError(res: ServerResponse, status: number, message: string, extra: Record<string, string> = {}): void {
  if (!res.headersSent) {
    res.writeHead(status, { "Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-store", ...extra });
  }
  if (!res.writableEnded) res.end(JSON.stringify({ error: message }));
}

async function readBody(req: IncomingMessage): Promise<unknown> {
  let size = 0;
  const chunks: Buffer[] = [];
  for await (const chunk of req) {
    size += (chunk as Buffer).length;
    if (size > MAX_BODY) throw new Error("Corps de requête trop gros.");
    chunks.push(chunk as Buffer);
  }
  const raw = Buffer.concat(chunks).toString("utf8");
  if (!raw.trim()) return undefined;
  return JSON.parse(raw);
}

export interface McpGateway {
  handle(req: IncomingMessage, res: ServerResponse, url: URL, expectedToken: string): Promise<void>;
  sessionCount(): number;
}

/**
 * Exposes the aggregate MCP server over Streamable HTTP, one stateful session
 * per client connection (Claude custom connectors speak this protocol).
 */
export function createMcpGateway(env: NodeJS.ProcessEnv = process.env): McpGateway {
  const sessions = new Map<string, { server: McpServer; transport: StreamableHTTPServerTransport }>();

  const openSession = async () => {
    const states = activeServiceStates(env, readSettings(env));
    const server = createAggregateServer(states);
    const transport = new StreamableHTTPServerTransport({
      sessionIdGenerator: () => randomUUID(),
      enableJsonResponse: false,
    });
    const remove = () => {
      const sessionId = transport.sessionId;
      if (sessionId) sessions.delete(sessionId);
      void server.close();
    };
    transport.onclose = remove;
    await server.connect(transport);
    return { server, transport };
  };

  return {
    sessionCount: () => sessions.size,
    async handle(req, res, url, expectedToken) {
      const bearer = /^Bearer\s+(.+)$/i.exec(req.headers.authorization ?? "")?.[1];
      // The public URL can carry the token: /mcp/<token> (Claude connector) or Authorization: Bearer <token>.
      const provided = bearer ?? (url.pathname !== "/mcp" ? url.pathname.replace(/^\/mcp\/?/, "") : undefined);
      if (!tokenMatches(expectedToken, provided)) {
        return jsonError(res, 401, "Token manquant ou invalide.", { "WWW-Authenticate": "Bearer" });
      }

      if (req.method === "OPTIONS") {
        res.writeHead(204, {
          "Access-Control-Allow-Methods": "GET, POST, DELETE, OPTIONS",
          "Access-Control-Allow-Headers": "Content-Type, Authorization, MCP-Session-Id, MCP-Protocol-Version, Last-Event-ID, mcp-session-id, mcp-protocol-version",
          "Access-Control-Allow-Origin": "*",
          "Access-Control-Max-Age": "600",
        });
        return;
      }

      const sessionIdHeader = req.headers["mcp-session-id"];
      const existing = typeof sessionIdHeader === "string" ? sessions.get(sessionIdHeader) : undefined;

      if (req.method === "POST") {
        let parsedBody: unknown;
        try {
          parsedBody = await readBody(req);
        } catch (error) {
          return jsonError(res, 400, error instanceof Error && /trop gros/.test(error.message) ? error.message : "Corps de requête invalide.");
        }
        try {
          if (existing) {
            await existing.transport.handleRequest(req, res, parsedBody);
          } else {
            // New session (initialize): the transport only reveals its id after that exchange.
            const { server, transport } = await openSession();
            try {
              await transport.handleRequest(req, res, parsedBody);
            } finally {
              const createdId = transport.sessionId;
              if (createdId) sessions.set(createdId, { server, transport });
              else await transport.close();
            }
          }
        } catch (error) {
          if (res.writableEnded) return;
          void error;
          return jsonError(res, 500, "Erreur du serveur MCP pendant la requête.");
        }
        return;
      }

      if (req.method === "GET") {
        if (!existing) return jsonError(res, 400, "Session MCP inconnue : initialise d'abord avec un POST.");
        try {
          await existing.transport.handleRequest(req, res);
        } catch {
          if (!res.writableEnded) jsonError(res, 500, "Flux MCP indisponible.");
        }
        return;
      }

      if (req.method === "DELETE") {
        if (existing) {
          await existing.transport.close();
          if (existing.transport.sessionId) sessions.delete(existing.transport.sessionId);
        }
        res.writeHead(204).end();
        return;
      }

      jsonError(res, 405, "Méthode non supportée.");
    },
  };
}
