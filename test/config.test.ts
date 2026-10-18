import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { configPath, gatewayConfigFrom, mcpEndpoint, normalizeProxmoxUrl, readSettings, writeSettings } from "../src/config.js";
import { locateResource, ProxmoxClient, ProxmoxApiError, type ProxmoxFetcher } from "../src/proxmox.js";

const tempDirs: string[] = [];
function tempConfigFile(): string {
  const dir = mkdtempSync(join(tmpdir(), "student-mcp-test-"));
  tempDirs.push(dir);
  return join(dir, "settings.json");
}

describe("settings file", () => {
  const envFiles: Array<{ clear: Array<string> }> = [];
  afterEach(() => {
    while (tempDirs.length) rmSync(tempDirs.pop()!, { recursive: true, force: true });
  });

  it("generates and persists a gateway token on first run", () => {
    const file = tempConfigFile();
    const env = { STUDENT_MCP_CONFIG_FILE: file };
    const first = gatewayConfigFrom({} as never, env as unknown as NodeJS.ProcessEnv);
    expect(first.token).toMatch(/^[0-9a-f]{48}$/);
    expect(readFileSync(file, "utf8")).toContain(first.token);
    const again = gatewayConfigFrom(readSettings(env as unknown as NodeJS.ProcessEnv), env as unknown as NodeJS.ProcessEnv);
    expect(again.token).toBe(first.token);
    expect(again.port).toBe(8420);
  });

  it("merges service blocks without losing existing keys", () => {
    const file = tempConfigFile();
    const env = { STUDENT_MCP_CONFIG_FILE: file } as unknown as NodeJS.ProcessEnv;
    writeSettings({ moodle: { url: "https://moodle.polymtl.ca", token: "t", allowWrite: false } }, env);
    writeSettings({ proxmox: { url: "https://pve:8006", tokenId: "st@pam!mcp", secret: "s", allowWrite: false } }, env);
    const settings = readSettings(env);
    expect(settings.moodle?.token).toBe("t");
    expect(settings.proxmox?.tokenId).toBe("st@pam!mcp");
    expect(existsSync(configPath(env))).toBe(true);
  });

  it("builds the public endpoint with the token in the path", () => {
    expect(mcpEndpoint("https://laptop.tail1234.ts.net/", { token: "abc" })).toBe("https://laptop.tail1234.ts.net/mcp/abc");
  });

  it("rejects weird proxmox urls", () => {
    expect(normalizeProxmoxUrl(" https://pve.exemple.ca:8006/api2/json ")).toBe("https://pve.exemple.ca:8006");
    expect(normalizeProxmoxUrl("pve.exemple.ca")).toBe("https://pve.exemple.ca");
    expect(() => normalizeProxmoxUrl("https://user:pass@pve:8006")).toThrow();
    expect(() => normalizeProxmoxUrl("https://pve:8006/dossier")).toThrow();
    expect(() => normalizeProxmoxUrl("ftp://pve")).toThrow();
  });
});

const baseConfig = { baseUrl: "https://pve.exemple.ca:8006", tokenId: "st@pam!mcp", secret: "sec", timeoutMs: 2_000, allowWrite: false };

function fetcher(handler: (path: string, init?: RequestInit) => Response): ProxmoxFetcher {
  return async (url: string, init?: RequestInit) => {
    const path = new URL(url).pathname.replace(/^\/api2\/json/, "") || "/";
    return handler(path, init);
  };
}

describe("proxmox client", () => {
  it("authenticates with the API token header and unwraps the data envelope", async () => {
    const seen: Array<{ auth?: string; path: string }> = [];
    const client = new ProxmoxClient(baseConfig, fetcher((path, init) => {
      seen.push({ auth: (init?.headers as Record<string, string>).Authorization, path });
      if (path === "/version") return Response.json({ data: { version: "8.3.1", release: "8.3" } });
      return Response.json({ data: [{ node: "pve1", status: "online" }] });
    }));
    expect(await client.version()).toEqual({ version: "8.3.1", release: "8.3" });
    await client.nodes();
    expect(seen[0]!.auth).toBe("PVEAPIToken=st@pam!mcp=sec");
    expect(seen[1]!.path).toBe("/nodes");
  });

  it("surfaces token/permission refusals as readable errors", async () => {
    const client = new ProxmoxClient({ ...baseConfig, tokenId: "st@pam!mcp" }, fetcher(() => Response.json({ errors: "no such token" }, { status: 401 })));
    await expect(client.nodes()).rejects.toThrow(ProxmoxApiError);
    await expect(client.nodes()).rejects.toThrow(/no such token/);
  });

  it("locates a machine across nodes for start/stop tools", async () => {
    const client = new ProxmoxClient(baseConfig, fetcher((path) => {
      if (path === "/cluster/resources") return Response.json({ data: [
        { vmid: 101, node: "pve1", type: "qemu", status: "stopped" },
        { vmid: 102, node: "pve2", type: "lxc", status: "running" },
      ] });
      return Response.json({ data: null });
    }));
    expect(await locateResource(client, 101)).toMatchObject({ node: "pve1", type: "qemu" });
    expect(await locateResource(client, 102)).toMatchObject({ node: "pve2", type: "lxc" });
    await expect(locateResource(client, 999)).rejects.toThrow(/Aucune VM ou CT 999/);
  });
});
