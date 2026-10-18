import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export interface MoodleConfig {
  baseUrl: string;
  token: string;
  timeoutMs: number;
  allowWrite: boolean;
}

export class ConfigError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ConfigError";
  }
}

export function configPath(env: NodeJS.ProcessEnv = process.env): string {
  return env.MOODLE_CONFIG_FILE || join(homedir(), ".student-mcp.json");
}

export function normalizeBaseUrl(input: string): string {
  const raw = input.trim().replace(/\/+$/, "").replace(/\/webservice\/rest\/server\.php$/i, "");
  const url = new URL(/^https?:\/\//i.test(raw) ? raw : `https://${raw}`);
  if (url.username || url.password || url.search || url.hash) {
    throw new ConfigError("MOODLE_URL must be a site URL without credentials, query or fragment.");
  }
  if (url.protocol !== "https:" && !(url.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))) {
    throw new ConfigError("MOODLE_URL must use HTTPS (HTTP is only supported for localhost).");
  }
  return url.origin + url.pathname.replace(/\/+$/, "");
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): MoodleConfig {
  let saved: Record<string, unknown> = {};
  const path = configPath(env);
  if (existsSync(path)) {
    try {
      const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("expected an object");
      saved = parsed as Record<string, unknown>;
    } catch {
      throw new ConfigError(`Unable to read Moodle configuration at ${path}.`);
    }
  }

  const url = env.MOODLE_URL || saved.url;
  const token = env.MOODLE_WSTOKEN || env.MOODLE_TOKEN || saved.token;
  if (typeof url !== "string" || !url.trim() || typeof token !== "string" || !token.trim()) {
    throw new ConfigError("Configurez MOODLE_URL et MOODLE_TOKEN, ou lancez student-mcp init depuis un terminal.");
  }

  const timeout = Number(env.MOODLE_TIMEOUT_MS || 30_000);
  return {
    baseUrl: normalizeBaseUrl(url),
    token: token.trim(),
    timeoutMs: Number.isFinite(timeout) && timeout > 0 ? timeout : 30_000,
    allowWrite: env.MOODLE_ALLOW_WRITE === "true",
  };
}
