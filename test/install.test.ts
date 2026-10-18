import { describe, expect, it } from "vitest";
import { clientConfigPath, mergeMcpConfig } from "../src/install.js";

describe("student installer", () => {
  it("adds OpenCode without losing JSONC comments or other MCPs", () => {
    const existing = '{\n  // My other tool\n  "mcp": { "calendar": { "type": "local", "command": ["calendar"] } }\n}\n';
    const result = mergeMcpConfig(existing, "opencode", "/home/student/.student-mcp/student-mcp");
    expect(result).toContain("// My other tool");
    expect(result).toContain('"calendar"');
    expect(result).toContain('"moodle"');
    expect(result).not.toContain("MOODLE_TOKEN");
  });

  it("supports OpenCode v2 and Windows paths in Claude Desktop", () => {
    const v2 = mergeMcpConfig('{"mcp":{"servers":{"old":{"type":"local"}}}}', "opencode", "/usr/bin/student-mcp");
    expect(JSON.parse(v2).mcp.servers.moodle.command).toEqual(["/usr/bin/student-mcp"]);
    const claude = mergeMcpConfig("{}", "claude", "C:\\Users\\Ada\\student-mcp.exe");
    expect(JSON.parse(claude).mcpServers.moodle.command).toBe("C:\\Users\\Ada\\student-mcp.exe");
    expect(clientConfigPath("opencode", "/home/student", { XDG_CONFIG_HOME: "/home/student/.config" })).toContain("opencode.json");
  });

  it("does not rewrite malformed settings", () => {
    expect(() => mergeMcpConfig("{broken", "opencode", "/tmp/mcp")).toThrow("left unchanged");
    expect(() => mergeMcpConfig('{"mcp":[]}', "opencode", "/tmp/mcp")).toThrow("left unchanged");
  });
});
