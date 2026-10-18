import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { MoodleClient } from "./client.js";
import type { MoodleConfig } from "./config.js";
import { VERSION } from "./config.js";
import { runServiceTool, moodleTools, activeServiceStates, type ServiceState, type ServiceTool } from "./services.js";

type ServerTool = Parameters<McpServer["registerTool"]>;

/** Shared registration for both transports (stdio and Streamable HTTP). */
export function registerServiceTools(server: McpServer, tools: ServiceTool[]): void {
  for (const tool of tools) {
    server.registerTool(tool.name, {
      description: tool.description,
      ...(tool.inputSchema ? { inputSchema: tool.inputSchema } : {}),
      ...(tool.annotations ? { annotations: tool.annotations } : {}),
    } as ServerTool[1], async (args) => runServiceTool(tool, (args ?? {}) as Record<string, unknown>));
  }
}

/** One MCP server exposing every configured service at this moment. */
export function createAggregateServer(states: ServiceState[]): McpServer {
  const server = new McpServer({ name: "student-mcp", version: VERSION });
  server.registerTool("student_status", {
    description: "Show which student services this connector exposes (Moodle, Proxmox, ...) and the accounts in use.",
    annotations: { readOnlyHint: true },
  }, async () => ({
    content: [{ type: "text", text: JSON.stringify({
      version: VERSION,
      services: states.map((state) => state.descriptor.id),
      accounts: states.map((state) => ({ service: state.descriptor.id, name: state.descriptor.name })),
    }) }],
  }));
  for (const state of states) registerServiceTools(server, state.descriptor.connect(state.creds as never, process.env));
  return server;
}

/** stdio entry: MCP over inout for local AI clients (kept for OpenCode / Claude Desktop). */
export function createServer(config: MoodleConfig, client = new MoodleClient(config)): McpServer {
  const server = new McpServer({ name: "student-mcp", version: VERSION });
  registerServiceTools(server, moodleTools(config, client));
  return server;
}
