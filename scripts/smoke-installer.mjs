#!/usr/bin/env node
// Optional local Linux/macOS PTY test (`expect` must be installed).
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, existsSync, rmSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

if (!process.argv[2]) throw new Error("Usage: node scripts/smoke-installer.mjs <native-binary>");
const temp = mkdtempSync(join(tmpdir(), "moodle-install-smoke-"));
const saved = join(temp, "settings.json");
const installed = join(temp, "installed-student-mcp");
const xdg = join(temp, "xdg");
const site = createServer(async (req, res) => {
  let body = "";
  for await (const part of req) body += part;
  const form = new URLSearchParams(body);
  assert.equal(req.method, "POST");
  res.setHeader("Content-Type", "application/json");
  if (req.url === "/login/token.php") {
    assert.equal(form.get("service"), "moodle_mobile_app");
    assert.equal(form.get("username"), "fake_student");
    assert.equal(form.get("password"), "FAKE_PASSWORD");
    res.end(JSON.stringify({ token: "FAKE_MOBILE_TOKEN", privatetoken: "NOT_FOR_REST" }));
  } else {
    assert.equal(form.get("wsfunction"), "core_webservice_get_site_info");
    assert.equal(form.get("wstoken"), "FAKE_MOBILE_TOKEN");
    res.end(JSON.stringify({ userid: 42, functions: [] }));
  }
});

try {
  await new Promise((accept) => site.listen(0, "127.0.0.1", accept));
  const { port } = site.address();
  const script = `
    set timeout 20
    spawn $env(MCP_BINARY) install
    expect "Adresse de votre Moodle"
    send "http://127.0.0.1:$env(MCP_PORT)\\r"
    expect "Obtenir un token"
    send "\\r"
    expect "Identifiant Moodle"
    send "fake_student\\r"
    expect "Mot de passe Moodle"
    send "FAKE_PASSWORD\\r"
    expect "Votre application IA"
    send "1\\r"
    expect eof
    catch wait result
    exit [lindex $result 3]
  `;
  const child = spawn("expect", ["-c", script], {
    env: { ...process.env, MCP_BINARY: resolve(process.argv[2]), MCP_PORT: String(port), MOODLE_CONFIG_FILE: saved, MOODLE_INSTALL_PATH: installed, XDG_CONFIG_HOME: xdg },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  child.stdout.on("data", (chunk) => { output += chunk; });
  child.stderr.on("data", (chunk) => { output += chunk; });
  const exit = await new Promise((accept, reject) => {
    child.on("error", reject);
    child.on("close", accept);
  });
  assert.equal(exit, 0, output.replaceAll("FAKE_PASSWORD", "[redacted]"));
  assert.equal(existsSync(installed), true);
  assert.equal(spawnSync(installed, ["--help"]).status, 0);
  const settings = JSON.parse(readFileSync(saved, "utf8"));
  assert.equal(settings.token, "FAKE_MOBILE_TOKEN");
  assert(!readFileSync(saved, "utf8").includes("FAKE_PASSWORD"));
  assert(!readFileSync(saved, "utf8").includes("NOT_FOR_REST"));
  const client = JSON.parse(readFileSync(join(xdg, "opencode", "opencode.json"), "utf8"));
  assert.deepEqual(client.mcp.moodle.command, [installed]);
  assert(!JSON.stringify(client).includes("FAKE_MOBILE_TOKEN"));
  process.stdout.write("Student installer smoke test passed (no real credentials used).\n");
} finally {
  await new Promise((accept) => site.close(accept));
  rmSync(temp, { recursive: true, force: true });
}
