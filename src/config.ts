import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync, chmodSync } from "node:fs";
import { randomBytes } from "node:crypto";
import { homedir } from "node:os";
import { dirname, join } from "node:path";

export const VERSION = "0.2.0";

export interface MoodleConfig {
  baseUrl: string;
  token: string;
  timeoutMs: number;
  allowWrite: boolean;
}

export interface ProxmoxConfig {
  baseUrl: string;
  tokenId: string;
  secret: string;
  timeoutMs: number;
  allowWrite: boolean;
}

export interface GatewayConfig {
  port: number;
  host: string;
  token: string;
}

export interface AppSettings {
  gateway?: { port?: number; token?: string };
  moodle?: { url?: string; token?: string; username?: string; allowWrite?: boolean };
  proxmox?: { url?: string; tokenId?: string; secret?: string; allowWrite?: boolean };
}

/** Like AppSettings, but a null block deletes that service's stored credentials. */
export interface SettingsUpdate {
  gateway?: AppSettings["gateway"] | null;
  moodle?: AppSettings["moodle"] | null;
  proxmox?: AppSettings["proxmox"] | null;
}

export class ConfigError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ConfigError";
  }
}

export function configPath(env: NodeJS.ProcessEnv = process.env): string {
  return env.STUDENT_MCP_CONFIG_FILE || env.MOODLE_CONFIG_FILE || join(homedir(), ".student-mcp.json");
}

/** Read the saved settings; throw a clear ConfigError when the file is corrupt. */
export function readSettings(env: NodeJS.ProcessEnv = process.env): AppSettings {
  const path = configPath(env);
  if (!existsSync(path)) return {};
  try {
    const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("expected an object");
    return parsed as AppSettings;
  } catch {
    throw new ConfigError(`Unable to read student-mcp configuration at ${path}. Fix or delete that file and try again.`);
  }
}

/** Merge-then-save settings atomically. A null block clears that service. */
export function writeSettings(update: SettingsUpdate, env: NodeJS.ProcessEnv = process.env): AppSettings {
  const path = configPath(env);
  const current = readSettings(env);
  const merged: AppSettings = { ...current, ...update } as AppSettings;
  for (const section of ["gateway", "moodle", "proxmox"] as const) {
    const value = update[section];
    if (value === null) {
      delete merged[section];
      continue;
    }
    if (value && typeof value === "object") {
      merged[section] = { ...current[section], ...value } as AppSettings[typeof section];
    }
  }
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  const temp = `${path}.tmp-${process.pid}`;
  try {
    writeFileSync(temp, JSON.stringify(merged, null, 2) + "\n", { mode: 0o600, flag: "wx" });
    renameSync(temp, path);
  } finally {
    if (existsSync(temp)) rmSync(temp);
  }
  chmodSync(path, 0o600);
  return merged;
}

function timeoutFrom(env: NodeJS.ProcessEnv, fallback: number): number {
  const value = Number(env.MOODLE_TIMEOUT_MS);
  return Number.isFinite(value) && value > 0 ? value : fallback;
}

/** Moodle credentials from env first, then settings; null when incomplete. */
export function moodleConfigFrom(settings: AppSettings, env: NodeJS.ProcessEnv = process.env): MoodleConfig | null {
  const url = env.MOODLE_URL || settings.moodle?.url;
  const token = env.MOODLE_WSTOKEN || env.MOODLE_TOKEN || settings.moodle?.token;
  if (typeof url !== "string" || !url.trim() || typeof token !== "string" || !token.trim()) return null;
  return {
    baseUrl: normalizeBaseUrl(url),
    token: token.trim(),
    timeoutMs: timeoutFrom(env, 30_000),
    allowWrite: env.MOODLE_ALLOW_WRITE === "true" || settings.moodle?.allowWrite === true,
  };
}

/** stdio mode requires Moodle credentials (kept for local AI clients). */
export function loadConfig(env: NodeJS.ProcessEnv = process.env): MoodleConfig {
  const config = moodleConfigFrom(readSettings(env), env);
  if (!config) {
    throw new ConfigError("Configurez MOODLE_URL et MOODLE_TOKEN, ou lancez `student-mcp up` depuis un terminal et configurez Moodle sur la page.");
  }
  return config;
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

/** Proxmox credentials from env first, then settings; null when incomplete. */
export function proxmoxConfigFrom(settings: AppSettings, env: NodeJS.ProcessEnv = process.env): ProxmoxConfig | null {
  const url = env.PROXMOX_URL || settings.proxmox?.url;
  const tokenId = env.PROXMOX_TOKEN_ID || settings.proxmox?.tokenId;
  const secret = env.PROXMOX_TOKEN_SECRET || settings.proxmox?.secret;
  if (typeof url !== "string" || !url.trim() || typeof tokenId !== "string" || !tokenId.trim() || typeof secret !== "string" || !secret.trim()) return null;
  return {
    baseUrl: normalizeProxmoxUrl(url),
    tokenId: tokenId.trim(),
    secret: secret.trim(),
    timeoutMs: timeoutFrom(env, 15_000),
    allowWrite: env.PROXMOX_ALLOW_WRITE === "true" || settings.proxmox?.allowWrite === true,
  };
}

export function normalizeProxmoxUrl(input: string): string {
  const raw = input.trim().replace(/\/+$/, "").replace(/\/api2\/json$/i, "");
  const url = new URL(/^https?:\/\//i.test(raw) ? raw : `https://${raw}`);
  if (url.username || url.password || url.search || url.hash) {
    throw new ConfigError("L'adresse Proxmox ne doit pas contenir d'identifiants, de requête ou de fragment.");
  }
  if (url.pathname.replace(/\/+$/, "")) {
    throw new ConfigError("L'adresse Proxmox doit pointer vers la racine de l'hôte (ex. https://mon-pve:8006).");
  }
  if (url.protocol !== "https:" && !(url.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))) {
    throw new ConfigError("L'adresse Proxmox doit utiliser HTTPS (HTTP seulement pour localhost).");
  }
  return url.origin;
}

/** Gateway settings, generating and persisting the public token on first run. */
export function gatewayConfigFrom(settings: AppSettings, env: NodeJS.ProcessEnv = process.env): GatewayConfig {
  const port = Number(env.STUDENT_MCP_PORT || settings.gateway?.port || 8420) || 8420;
  const host = env.STUDENT_MCP_HOST || "127.0.0.1";
  let token = env.STUDENT_MCP_TOKEN || settings.gateway?.token;
  if (!token) {
    const generated = randomBytes(24).toString("hex");
    writeSettings({ gateway: { token: generated } }, env);
    return { port, host, token: generated };
  }
  return { port, host, token };
}

/** The public MCP endpoint for a given host, token included when required. */
export function mcpEndpoint(origin: string, gateway: { token: string }): string {
  return `${origin.replace(/\/+$/, "")}/mcp/${gateway.token}`;
}
