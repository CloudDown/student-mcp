import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { VERSION, gatewayConfigFrom, moodleConfigFrom, proxmoxConfigFrom, readSettings, writeSettings, type AppSettings } from "./config.js";
import { PLANNED_SERVICES, SERVICES, credentialsToSettings, serviceById, type ServiceDeps } from "./services.js";
import { createMcpGateway } from "./mcp-http.js";
import { funnelDisable, funnelEnable, publicConnectorUrl, tailscaleInfo } from "./funnel.js";
import { renderDashboard } from "./dashboard.js";

class HttpError extends Error {
  constructor(readonly status: number, message: string) {
    super(message);
    this.name = "HttpError";
  }
}

function json(res: ServerResponse, status: number, payload: unknown): void {
  res.writeHead(status, { "Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-store" });
  res.end(JSON.stringify(payload));
}

function fail(res: ServerResponse, status: number, error: string): void {
  json(res, status, { error });
}

async function readJson(req: IncomingMessage, limit = 256 * 1024): Promise<Record<string, unknown>> {
  let size = 0;
  const chunks: Buffer[] = [];
  for await (const chunk of req) {
    size += (chunk as Buffer).length;
    if (size > limit) throw new HttpError(413, "Requête trop grosse.");
    chunks.push(chunk as Buffer);
  }
  if (!chunks.length) return {};
  try {
    const parsed: unknown = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("attendu un objet");
    return parsed as Record<string, unknown>;
  } catch {
    throw new HttpError(400, "Corps JSON invalide.");
  }
}

export interface WebServer {
  server: Server;
  localUrl: string;
  stop(): Promise<void>;
}

export interface WebServerOptions {
  port: number;
  host: string;
  env?: NodeJS.ProcessEnv;
  deps?: ServiceDeps;
}

/** Minimal dashboard + JSON API + MCP endpoint, no framework. */
export function startWebServer(options: WebServerOptions): Promise<WebServer> {
  const env = options.env ?? process.env;
  const deps: ServiceDeps = options.deps ?? {};
  const mcp = createMcpGateway(env);
  const isLocalBinding = options.host === "127.0.0.1" || options.host === "localhost";
  let actualPort = options.port;

  function dashboardAuthorized(url: URL, req: IncomingMessage, token: string): boolean {
    if (isLocalBinding) return true;
    const bearer = /^Bearer\s+(.+)$/i.exec(req.headers.authorization ?? "")?.[1];
    return (url.searchParams.get("token") ?? bearer ?? "") === token;
  }

  async function route(req: IncomingMessage, res: ServerResponse, url: URL): Promise<void> {
    const settings = readSettings(env);
    const gateway = gatewayConfigFrom(settings, env);

    if (url.pathname === "/" && req.method === "GET") {
      if (!dashboardAuthorized(url, req, gateway.token)) return fail(res, 401, "Token manquant ou invalide : ouvre l'URL affichée par `student-mcp up`.");
      res.writeHead(200, {
        "Content-Type": "text/html; charset=utf-8",
        "Cache-Control": "no-store",
        "X-Content-Type-Options": "nosniff",
        "Referrer-Policy": "no-referrer",
      });
      res.end(renderDashboard());
      return;
    }

    if (url.pathname === "/mcp" || url.pathname.startsWith("/mcp/")) {
      return mcp.handle(req, res, url, gateway.token);
    }

    if (!url.pathname.startsWith("/api/")) throw new HttpError(404, "Page introuvable.");
    if (!dashboardAuthorized(url, req, gateway.token)) return fail(res, 401, "Token manquant ou invalide.");

    if (url.pathname === "/api/status" && req.method === "GET") {
      return json(res, 200, await statusPayload(gateway.token));
    }

    const serviceMatch = /^\/api\/services\/([a-z-]+)$/.exec(url.pathname);
    if (serviceMatch && req.method === "POST") {
      const id = serviceMatch[1]!;
      const descriptor = serviceById(id);
      if (!descriptor) throw new HttpError(404, `Service inconnu : ${id}.`);
      const body = await readJson(req);
      if (body.action === "disable") {
        return json(res, 200, { ok: true, id, ...writeSettings(id === "moodle" ? { moodle: null } : { proxmox: null }, env) });
      }
      if (body.ack !== true) {
        return fail(res, 400, "Accepte l'accord d'utilisation avant d'activer le service.");
      }
      const { creds, lines } = await descriptor.activate(body, deps);
      const update = credentialsToSettings(descriptor.id, creds, body.allowWrite === true);
      writeSettings(update, env);
      return json(res, 200, { ok: true, id, lines });
    }

    if (url.pathname === "/api/funnel" && req.method === "POST") {
      const body = await readJson(req);
      const result = body.action === "disable" ? await funnelDisable() : await funnelEnable(options.port);
      return json(res, result.ok ? 200 : 400, { ok: result.ok, message: result.message });
    }

    throw new HttpError(404, "Page introuvable.");
  }

  const server = createServer((req, res) => {
    void (async () => {
      try {
        const url = new URL(req.url ?? "/", "http://localhost");
        await route(req, res, url);
      } catch (error) {
        if (error instanceof HttpError) return fail(res, error.status, error.message);
        if (error instanceof Error && ["ConfigError", "TokenError", "MoodleApiError", "ProxmoxApiError"].includes(error.name)) {
          return fail(res, 400, error.message);
        }
        if (!res.writableEnded) fail(res, 500, "Erreur interne du serveur student-mcp.");
      }
    })();
  });

  return new Promise<WebServer>((resolve, reject) => {
    server.once("error", reject);
    server.listen(options.port, options.host, () => {
      const address = server.address();
      const resolvedPort = typeof address === "object" && address ? address.port : options.port;
      actualPort = resolvedPort;
      const displayHost = options.host === "127.0.0.1" ? "localhost" : options.host;
      resolve({
        server,
        localUrl: `http://${displayHost}:${resolvedPort}`,
        stop() {
          return new Promise((done) => server.close(() => done()));
        },
      });
    });
  });

  async function statusPayload(token: string): Promise<Record<string, unknown>> {
    const settings = readSettings(env);
    const enabled = {
      moodle: moodleConfigFrom(settings, env) !== null,
      proxmox: proxmoxConfigFrom(settings, env) !== null,
    };
    const tailscale = await tailscaleInfo();
    const publicUrl = tailscale.loggedIn && tailscale.host && tailscale.funnelOn ? publicConnectorUrl(tailscale.host, token) : null;
    return {
      version: VERSION,
      gateway: {
        port: actualPort,
        localMcpUrl: `http://localhost:${actualPort}/mcp/${token}`,
      },
      services: SERVICES.map((service) => ({
        id: service.id,
        name: service.name,
        tagline: service.tagline,
        notes: service.notes,
        fields: service.fields,
        allowWriteOptional: service.allowWriteOptional,
        writeLabel: service.writeLabel,
        mark: service.mark,
        accent: service.accent,
        enabled: enabled[service.id],
      })),
      planned: PLANNED_SERVICES,
      connector: {
        localMcpUrl: `http://localhost:${actualPort}/mcp/${token}`,
        publicMcpUrl: publicUrl,
        tailscale: { present: tailscale.present, loggedIn: tailscale.loggedIn, funnelOn: tailscale.funnelOn, message: tailscale.message ?? null },
      },
    };
  }
}

export type { AppSettings };
