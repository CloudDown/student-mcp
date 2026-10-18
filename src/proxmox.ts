import type { ProxmoxConfig } from "./config.js";
import { ConfigError, normalizeProxmoxUrl } from "./config.js";

export class ProxmoxApiError extends Error {
  constructor(message: string, readonly status: number) {
    super(message);
    this.name = "ProxmoxApiError";
  }
}

export type ProxmoxFetcher = (url: string, init?: RequestInit) => Promise<Response>;

interface ProxmoxEnvelope<T> { data?: T; errors?: string }

/** Proxmox VE speaks JSON under /api2/json and authenticates API tokens inline. */
export class ProxmoxClient {
  constructor(private readonly config: ProxmoxConfig, private readonly fetchImpl: ProxmoxFetcher = fetch) {}

  async request<T>(path: string, method = "GET", body?: unknown): Promise<T> {
    let response: Response;
    const headers: Record<string, string> = { Accept: "application/json", Authorization: `PVEAPIToken=${this.config.tokenId}=${this.config.secret}` };
    if (body !== undefined) headers["Content-Type"] = "application/json";
    try {
      response = await this.fetchImpl(`${this.config.baseUrl}/api2/json${path}`, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(this.config.timeoutMs),
        redirect: "error",
      });
    } catch (error) {
      const cause = error instanceof Error && error.name === "TimeoutError" ? "délai dépassé" : "connexion impossible";
      throw new ProxmoxApiError(`Proxmox : ${cause} (${this.config.baseUrl}). Vérifiez l'adresse, le port (souvent 8006) et votre réseau.`, 0);
    }
    let payload: unknown;
    try {
      payload = await response.json();
    } catch {
      throw new ProxmoxApiError(`Proxmox a renvoyé une réponse non JSON (${response.status}).`, response.status);
    }
    const data = (payload ?? {}) as ProxmoxEnvelope<T>;
    if (!response.ok) {
      const detail = data.errors?.trim();
      throw new ProxmoxApiError(detail ? `Proxmox a refusé la requête (${response.status}) : ${detail}` : `Proxmox a refusé la requête (${response.status}). Vérifiez l'ID de token et ses permissions.`, response.status);
    }
    if (payload === null || typeof payload !== "object") return {} as T;
    return data.data as T;
  }

  version(): Promise<{ version?: string; release?: string; repoid?: string }> {
    return this.request("/version");
  }

  nodes(): Promise<Array<{ node?: string; status?: string; mem?: number; maxmem?: number; cpu?: number; maxcpu?: number }>> {
    return this.request("/nodes");
  }

  /** Every qemu/lxc resource visible to this token across the cluster. */
  resources(): Promise<Array<{ vmid?: number; name?: string; node?: string; status?: string; type?: string; template?: 0 | 1 }>> {
    return this.request("/cluster/resources?type=vm");
  }
}

export interface ProxmoxResource { vmid: number; name?: string; node: string; status: string; type: string }

/** Resolve a VM/LXC id to its node using the token's visible resources. */
export async function locateResource(client: ProxmoxClient, vmid: number): Promise<ProxmoxResource> {
  const resources = await client.resources();
  const found = resources.filter((entry) => entry.vmid === vmid && entry.node && entry.type && !entry.template);
  if (!found.length) throw new ProxmoxApiError(`Aucune VM ou CT ${vmid} n'est visible avec ce token Proxmox.`, 404);
  return {
    vmid: vmid,
    node: found[0]!.node!,
    status: found[0]!.status ?? "unknown",
    type: found[0]!.type!,
  };
}

export function normalizeProxmoxUrlChecked(input: string): string {
  return normalizeProxmoxUrl(input);
}

export function configError(message: string): Error {
  return new ConfigError(message);
}
