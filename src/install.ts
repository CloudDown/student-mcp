import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { applyEdits, modify, parse, type ParseError } from "jsonc-parser";

export type ClientName = "opencode" | "claude";

/** A stable path: the downloaded release ZIP can safely be deleted afterwards. */
export function installedBinaryPath(home = homedir(), platform = process.platform): string {
  return join(home, ".student-mcp", platform === "win32" ? "student-mcp.exe" : "student-mcp");
}

export function installBinary(source: string, destination = installedBinaryPath()): string {
  mkdirSync(dirname(destination), { recursive: true, mode: 0o700 });
  if (resolve(source) === resolve(destination)) return destination;
  const temp = `${destination}.tmp-${process.pid}`;
  try {
    copyFileSync(source, temp);
    chmodSync(temp, 0o755);
    renameSync(temp, destination);
  } finally {
    if (existsSync(temp)) rmSync(temp);
  }
  return destination;
}

export function clientConfigPath(client: ClientName, home = homedir(), env: NodeJS.ProcessEnv = process.env): string {
  if (client === "opencode") {
    const directory = join(env.XDG_CONFIG_HOME || join(home, ".config"), "opencode");
    const json = join(directory, "opencode.json");
    const jsonc = join(directory, "opencode.jsonc");
    return existsSync(json) || !existsSync(jsonc) ? json : jsonc;
  }
  if (process.platform === "win32") {
    return join(env.APPDATA || join(home, "AppData", "Roaming"), "Claude", "claude_desktop_config.json");
  }
  if (process.platform === "darwin") {
    return join(home, "Library", "Application Support", "Claude", "claude_desktop_config.json");
  }
  throw new Error("Claude Desktop does not have a standard Linux configuration location. Choose OpenCode instead.");
}

/** Edit one MCP entry, preserving unrelated settings and JSONC comments. */
export function mergeMcpConfig(contents: string, client: ClientName, executable: string): string {
  const original = contents.trim() ? contents : "{}\n";
  const errors: ParseError[] = [];
  const data: unknown = parse(original, errors, { allowTrailingComma: true });
  if (errors.length || !data || typeof data !== "object" || Array.isArray(data)) {
    throw new Error("This client's configuration could not be parsed; it was left unchanged.");
  }
  const root = data as Record<string, unknown>;
  if (client === "opencode" && root.mcp !== undefined && (!root.mcp || typeof root.mcp !== "object" || Array.isArray(root.mcp))) {
    throw new Error("Existing OpenCode MCP settings are not an object; configuration was left unchanged.");
  }
  if (client === "claude" && root.mcpServers !== undefined && (!root.mcpServers || typeof root.mcpServers !== "object" || Array.isArray(root.mcpServers))) {
    throw new Error("Existing Claude MCP settings are not an object; configuration was left unchanged.");
  }
  const mcp = root.mcp && typeof root.mcp === "object" ? root.mcp as Record<string, unknown> : {};
  const v2 = client === "opencode" && mcp.servers && typeof mcp.servers === "object";
  const path = client === "claude" ? ["mcpServers", "moodle"] : v2 ? ["mcp", "servers", "moodle"] : ["mcp", "moodle"];
  const value = client === "claude"
    ? { command: executable, args: [] }
    : v2
      ? { type: "local", command: [executable], disabled: false }
      : { type: "local", command: [executable], enabled: true };
  const edits = modify(original, path, value, { formattingOptions: { insertSpaces: true, tabSize: 2, eol: process.platform === "win32" ? "\r\n" : "\n" } });
  const updated = applyEdits(original, edits);
  if (parse(updated, errors, { allowTrailingComma: true }) === undefined || errors.length) {
    throw new Error("Could not update this client's configuration; it was left unchanged.");
  }
  return updated.endsWith("\n") ? updated : `${updated}\n`;
}

export function configureClient(client: ClientName, executable: string, path = clientConfigPath(client)): string {
  const previous = existsSync(path) ? readFileSync(path, "utf8") : "{}\n";
  const updated = mergeMcpConfig(previous, client, executable);
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  if (existsSync(path)) copyFileSync(path, `${path}.backup-${Date.now()}`);
  const temp = `${path}.tmp-${process.pid}`;
  try {
    writeFileSync(temp, updated, { mode: 0o600, flag: "wx" });
    renameSync(temp, path);
  } finally {
    if (existsSync(temp)) rmSync(temp);
  }
  return path;
}
