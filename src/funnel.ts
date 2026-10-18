import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execute = promisify(execFile);
const run = (cmd: string, args: string[]): Promise<{ stdout: string; stderr: string } | null> =>
  execute(cmd, args, { timeout: 10_000, windowsHide: true }).catch(() => null);

export interface TailscaleInfo {
  present: boolean;
  loggedIn: boolean;
  /** Device DNS name inside the tailnet, e.g. laptop.tailXXXX.ts.net */
  host: string | null;
  /** Whether a funnel already proxies 443 to something. */
  funnelOn: boolean;
  message?: string;
}

function hostFromStatus(stdout: string): string | null {
  try {
    const status = JSON.parse(stdout) as { BackendState?: string; Self?: { DNSName?: string } };
    const dns = status.Self?.DNSName?.replace(/\.$/, "");
    return dns && status.BackendState === "Running" ? dns : null;
  } catch {
    return null;
  }
}

function funnelOnFromServe(stdout: string): boolean {
  return /funnel\s*(?:on|:|\s)*\b(?:on|true|enabled)/i.test(stdout) || /"funnel"\s*:\s*true/i.test(stdout);
}

/** Inspect the local tailscale CLI without ever throwing, for dashboard UX. */
export async function tailscaleInfo(): Promise<TailscaleInfo> {
  const version = await run("tailscale", ["version"]);
  if (!version) {
    return { present: false, loggedIn: false, host: null, funnelOn: false, message: "Tailscale n'est pas installé sur cet appareil." };
  }
  const status = await run("tailscale", ["status", "--json"]);
  const host = status ? hostFromStatus(status.stdout) : null;
  if (!host) {
    return { present: true, loggedIn: false, host: null, funnelOn: false, message: "Tailscale est installé mais pas connecté : exécute `tailscale login` (ou ouvre l'application)." };
  }
  const serve = await run("tailscale", ["serve", "status"]);
  const funnelOn = serve ? funnelOnFromServe(serve.stdout) : false;
  return { present: true, loggedIn: true, host, funnelOn };
}

/** Public HTTPS connector URL advertised through Tailscale Funnel. */
export function publicConnectorUrl(host: string, token: string): string {
  return `https://${host}/mcp/${token}`;
}

async function tailscaleCommand(args: string[]): Promise<boolean> {
  const direct = await run("tailscale", args);
  if (direct) return true;
  const privileged = await run("sudo", ["-n", "tailscale", ...args]);
  return privileged !== null;
}

/** Turn the funnel on: tailscale funnel --bg <port> (sudo fallback on Linux). */
export async function funnelEnable(port: number): Promise<{ ok: boolean; message: string }> {
  const info = await tailscaleInfo();
  if (!info.present) {
    return { ok: false, message: "Installe d'abord Tailscale (https://tailscale.com/download), connecte-toi, puis réessaie." };
  }
  if (!info.loggedIn || !info.host) {
    return { ok: false, message: "Connecte-toi d'abord à Tailscale : exécute `tailscale login` ou ouvre l'application Tailscale." };
  }
  const done = await tailscaleCommand(["funnel", "--bg", String(port)]);
  if (done) {
    return { ok: true, message: `Funnel actif : un certificat HTTPS pour ${info.host} est émis par Tailscale.` };
  }
  return { ok: false, message: `Tailscale n'a pas pu activer le funnel automatiquement. Exécute cette commande une fois, puis reteste :\n  sudo tailscale funnel --bg ${port}` };
}

/** Remove the published URL (funnel off) once the local server is stopped. */
export async function funnelDisable(): Promise<{ ok: boolean; message: string }> {
  await run("tailscale", ["funnel", "off"]);
  await run("sudo", ["-n", "tailscale", "funnel", "off"]);
  return { ok: true, message: "Funnel retiré : l'URL publique ne répond plus." };
}

