#!/usr/bin/env node
// Native binary integration check: run the executable outside the checkout,
// speak MCP over stdio, and use a local mock Moodle over HTTP.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport, getDefaultEnvironment } from "@modelcontextprotocol/sdk/client/stdio.js";

const executable = resolve(process.argv[2] ?? "");
if (!process.argv[2]) throw new Error("Usage: node scripts/smoke-binary.mjs <executable>");
let courseRequest = false;
const moodle = createServer(async (request, response) => {
  let body = "";
  for await (const chunk of request) body += chunk;
  const form = new URLSearchParams(body);
  assert.equal(request.method, "POST");
  assert.equal(form.get("wstoken"), "SMOKE_TOKEN");
  response.setHeader("Content-Type", "application/json");
  if (form.get("wsfunction") === "core_webservice_get_site_info") {
    response.end(JSON.stringify({ userid: 12, sitename: "Test campus", functions: [{ name: "mod_quiz_get_quizzes_by_courses" }] }));
  } else if (form.get("wsfunction") === "mod_quiz_get_quizzes_by_courses") {
    assert.equal(form.get("courseids[0]"), "42");
    courseRequest = true;
    response.end(JSON.stringify({ quizzes: [{ id: 7, name: "Quiz" }] }));
  } else {
    response.statusCode = 404;
    response.end(JSON.stringify({ exception: "invalid_function", message: "Unknown function" }));
  }
});

await new Promise((accept) => moodle.listen(0, "127.0.0.1", accept));
const { port } = moodle.address();
const transport = new StdioClientTransport({
  command: executable,
  cwd: tmpdir(),
  env: {
    ...getDefaultEnvironment(),
    MOODLE_URL: `http://127.0.0.1:${port}`,
    MOODLE_TOKEN: "SMOKE_TOKEN",
    MOODLE_CONFIG_FILE: resolve(tmpdir(), "student-mcp-no-config-smoke.json"),
  },
  stderr: "pipe",
});
const client = new Client({ name: "binary-smoke", version: "1.0" });
try {
  await client.connect(transport);
  const tools = await client.listTools();
  assert(tools.tools.some((tool) => tool.name === "ws_schema"));
  const schema = await client.callTool({ name: "ws_schema", arguments: { name: "mod_quiz_get_quizzes_by_courses" } });
  assert.match(JSON.stringify(schema), /courseids/);
  const quizzes = await client.callTool({ name: "moodle_quizzes", arguments: { courseids: [42] } });
  assert.match(JSON.stringify(quizzes), /Quiz/);
  assert.equal(courseRequest, true);
  process.stdout.write("Standalone MCP smoke test passed.\n");
} finally {
  await client.close();
  await new Promise((accept) => moodle.close(accept));
}
