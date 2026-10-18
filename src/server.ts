import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { z } from "zod";
import { FunctionIndex, loadCatalog, toJsonSchema } from "./catalog.js";
import { MoodleApiError, MoodleClient } from "./client.js";
import type { MoodleConfig } from "./config.js";

type SiteInfo = { userid?: number; sitename?: string; username?: string; release?: string; functions?: Array<{ name: string }> };

function output(data: unknown): CallToolResult {
  return { content: [{ type: "text", text: JSON.stringify(data, null, 2) ?? "null" }] };
}

async function safe(action: () => Promise<unknown>): Promise<CallToolResult> {
  try {
    return output(await action());
  } catch (error) {
    return {
      isError: true,
      content: [{ type: "text", text: error instanceof MoodleApiError ? error.message : "Moodle tool failed. Check your configuration and token permissions." }],
    };
  }
}

export function createServer(config: MoodleConfig, client = new MoodleClient(config)): McpServer {
  const bundle = loadCatalog();
  const catalog = FunctionIndex.fromCatalog(bundle);
  const server = new McpServer({ name: "student-mcp", version: "0.1.1" });
  let siteInfoPromise: Promise<SiteInfo> | undefined;
  const siteInfo = () => {
    // Retry on a transient failure rather than caching it until restart.
    siteInfoPromise ??= client.siteInfo().then((info) => info as SiteInfo).catch((error: unknown) => {
      siteInfoPromise = undefined;
      throw error;
    });
    return siteInfoPromise;
  };
  const availability = async () => {
    const info = await siteInfo();
    return new Set((info.functions ?? []).map((fn) => fn.name));
  };
  const call = async (name: string, args: Record<string, unknown> = {}) => {
    const available = await availability();
    if (!available.has(name)) throw new MoodleApiError(`Function ${name} is not available to this token.`, 0);
    const definition = catalog.get(name);
    if ((!definition || definition.type === "write") && !config.allowWrite) {
      throw new MoodleApiError(`Function ${name} may change Moodle data. Set MOODLE_ALLOW_WRITE=true to enable it.`, 0);
    }
    return client.call(name, args);
  };

  server.registerTool("moodle_status", {
    description: "Check the Moodle connection and show which site and account this token belongs to.",
    annotations: { readOnlyHint: true },
  }, async () => safe(async () => {
    const { sitename, username, userid, release, functions } = await siteInfo();
    return { sitename, username, userid, release, availableFunctions: functions?.length ?? 0 };
  }));

  server.registerTool("moodle_my_courses", {
    description: "List the courses in which the connected student is enrolled.",
    annotations: { readOnlyHint: true },
  }, async () => safe(async () => {
    const { userid } = await siteInfo();
    if (!userid) throw new MoodleApiError("Moodle did not provide a user ID for this token.", 0);
    return call("core_enrol_get_users_courses", { userid });
  }));

  server.registerTool("moodle_course_content", {
    description: "Get the sections, activities and resources in a course.",
    inputSchema: { courseid: z.number().int().positive() },
    annotations: { readOnlyHint: true },
  }, async ({ courseid }) => safe(() => call("core_course_get_contents", { courseid })));

  server.registerTool("moodle_assignments", {
    description: "List visible assignments; optionally filter to specific course IDs.",
    inputSchema: { courseids: z.array(z.number().int().positive()).optional() },
    annotations: { readOnlyHint: true },
  }, async ({ courseids }) => safe(() => call("mod_assign_get_assignments", courseids ? { courseids } : {})));

  server.registerTool("moodle_quizzes", {
    description: "List visible quizzes; optionally filter to specific course IDs.",
    inputSchema: { courseids: z.array(z.number().int().positive()).optional() },
    annotations: { readOnlyHint: true },
  }, async ({ courseids }) => safe(() => call("mod_quiz_get_quizzes_by_courses", courseids ? { courseids } : {})));

  server.registerTool("moodle_grades", {
    description: "Get the connected student's grade items for a course.",
    inputSchema: { courseid: z.number().int().positive() },
    annotations: { readOnlyHint: true },
  }, async ({ courseid }) => safe(() => call("gradereport_user_get_grade_items", { courseid })));

  server.registerTool("moodle_calendar", {
    description: "Get the upcoming Moodle calendar view; optionally filter to one course.",
    inputSchema: { courseid: z.number().int().positive().optional() },
    annotations: { readOnlyHint: true },
  }, async ({ courseid }) => safe(() => call("core_calendar_get_calendar_upcoming_view", courseid ? { courseid } : {})));

  server.registerTool("ws_discover", {
    description: "Search Moodle Web Service functions this token can actually call, with descriptions and access type.",
    inputSchema: { query: z.string().default(""), limit: z.number().int().min(1).max(100).default(20) },
    annotations: { readOnlyHint: true },
  }, async ({ query, limit }) => safe(async () => {
    const available = await availability();
    const matches = catalog.search(query, catalog.size).filter((fn) => available.has(fn.name)).slice(0, limit);
    return { totalAvailable: available.size, results: matches.map(({ name, description, type }) => ({ name, description, type })) };
  }));

  server.registerTool("ws_schema", {
    description: "Show the parameters and usage details for an available Moodle Web Service function.",
    inputSchema: { name: z.string() },
    annotations: { readOnlyHint: true },
  }, async ({ name }) => safe(async () => {
    const available = await availability();
    if (!available.has(name)) throw new MoodleApiError(`Function ${name} is not available to this token.`, 0);
    const fn = catalog.get(name);
    return fn ? { name: fn.name, description: fn.description, type: fn.type, inputSchema: toJsonSchema(fn), capabilities: fn.capabilities, catalogVersion: bundle.moodle?.release }
      : { name, type: "unknown", note: "Custom or newer function: consult your Moodle site's API documentation. Calls require MOODLE_ALLOW_WRITE=true." };
  }));

  server.registerTool("ws_call", {
    description: "Call an available Moodle Web Service function by exact name. Find it with ws_discover and check parameters with ws_schema first. Writes require MOODLE_ALLOW_WRITE=true.",
    inputSchema: { name: z.string(), args: z.record(z.string(), z.unknown()).default({}) },
    annotations: { readOnlyHint: !config.allowWrite },
  }, async ({ name, args }) => safe(() => call(name, args)));

  return server;
}
