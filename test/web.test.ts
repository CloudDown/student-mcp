import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { startWebServer, type WebServer } from "../src/web.js";
import type { FetchLike } from "../src/client.js";

const windingDown: WebServer[] = [];
function tempEnv(): { file: string; env: NodeJS.ProcessEnv } {
  const file = join(mkdtempSync(join(tmpdir(), "student-mcp-web-")), "settings.json");
  return { file, env: { STUDENT_MCP_CONFIG_FILE: file } };
}

afterEach(async () => {
  while (windingDown.length) await windingDown.pop()!.stop();
});

function moodleFetchStub(): FetchLike {
  return async (url: string, init?: RequestInit) => {
    const target = new URL(url);
    const form = new URLSearchParams(init?.body as string);
    if (target.pathname.endsWith("/login/token.php")) {
      return new Response(JSON.stringify({ token: "moodle-ws-token" }), { status: 200 });
    }
    if (form.get("wsfunction") === "core_webservice_get_site_info") {
      return new Response(JSON.stringify({ userid: 7, sitename: "Poly", username: "etudiant", functions: [{ name: "core_enrol_get_users_courses" }] }));
    }
    return new Response(JSON.stringify([{ id: 42, fullname: "Calcul" }]));
  };
}

describe("dashboard HTTP server", () => {
  it("serves the white page, status and guards everything", async () => {
    const { env } = tempEnv();
    const web = await startWebServer({ port: 0, host: "127.0.0.1", env });
    windingDown.push(web);

    const home = new Response(await (await fetch(web.localUrl)).text());
    expect(home.status).toBe(200);

    const status = await (await fetch(`${web.localUrl}/api/status`)).json();
    expect(status.gateway.port).toBeGreaterThan(0);
    expect(status.services).toHaveLength(2);
    expect(status.services.map((s: { id: string }) => s.id)).toEqual(["moodle", "proxmox"]);
    expect(status.planned.map((s: { id: string }) => s.id)).toEqual(["dossier", "courriel"]);
    expect(status.connector.localMcpUrl).toMatch(/\/mcp\/[0-9a-f]{48}/);
    expect(status.connector.publicMcpUrl).toBeNull();

    expect((await fetch(`${web.localUrl}/mcp`)).status).toBe(401);
    const token = new URL(status.connector.localMcpUrl).pathname.split("/mcp/")[1];
    expect((await fetch(`${web.localUrl}/mcp/${token}`)).status).toBe(400);
    expect((await fetch(`${web.localUrl}/api/nope`)).status).toBe(404);
  });

  it("requires the consent checkbox before storing any credential", async () => {
    const { env } = tempEnv();
    const web = await startWebServer({ port: 0, host: "127.0.0.1", env });
    windingDown.push(web);
    const refusal = await post(web, "/api/services/moodle", { url: "https://moodle.polymtl.ca", username: "a", password: "b" });
    expect(refusal.status).toBe(400);
    expect(JSON.stringify(await refusal.json())).toContain("accord");
  });

  it("activates Moodle with credentials, stores only the token, then disables", async () => {
    const { file, env } = tempEnv();
    const web = await startWebServer({ port: 0, host: "127.0.0.1", env, deps: { moodleFetch: moodleFetchStub() } });
    windingDown.push(web);

    const enabled = await post(web, "/api/services/moodle", { ack: true, url: "https://moodle.polymtl.ca", username: "etudiant", password: "pass" });
    expect(enabled.status).toBe(200);
    const result = await enabled.json();
    expect(result.lines.join(" ")).toContain("Connecté à Poly");
    const stored = JSON.parse(readFileSync(file, "utf8"));
    expect(stored.moodle).toEqual({ url: "https://moodle.polymtl.ca", token: "moodle-ws-token", allowWrite: false });
    expect(JSON.stringify(stored)).not.toContain("pass");

    const status = await (await fetch(`${web.localUrl}/api/status`)).json();
    expect(status.services.find((s: { id: string }) => s.id === "moodle").enabled).toBe(true);

    const disabled = await post(web, "/api/services/moodle", { action: "disable" });
    expect(disabled.status).toBe(200);
    const after = JSON.parse(readFileSync(file, "utf8"));
    expect(after.moodle).toBeUndefined();
  });

  it("turns the funnel toggle off without requiring tailscale in tests", async () => {
    const { env } = tempEnv();
    const web = await startWebServer({ port: 0, host: "127.0.0.1", env });
    windingDown.push(web);
    const response = await post(web, "/api/funnel", { action: "disable" });
    expect(response.status).toBe(200);
  });
});

function post(web: WebServer, path: string, body: unknown): Promise<Response> {
  return fetch(`${web.localUrl}${path}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
}
