import { z } from "zod";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { loadCatalog, FunctionIndex, toJsonSchema } from "./catalog.js";
import { requestMobileToken, TokenError } from "./auth.js";
import { MoodleApiError, MoodleClient, type FetchLike } from "./client.js";
import type { AppSettings, MoodleConfig, ProxmoxConfig } from "./config.js";
import { normalizeBaseUrl, normalizeProxmoxUrl, moodleConfigFrom, proxmoxConfigFrom, readSettings } from "./config.js";
import { locateResource, ProxmoxApiError, ProxmoxClient, type ProxmoxFetcher } from "./proxmox.js";

export type ServiceId = "moodle" | "proxmox";

export interface ServiceField {
  id: string;
  label: string;
  type: "url" | "text" | "password";
  placeholder?: string;
  help?: string;
  required: boolean;
}

export interface ServiceTool {
  name: string;
  description: string;
  inputSchema?: z.ZodRawShape;
  annotations?: { title?: string; readOnlyHint?: boolean; destructiveHint?: boolean };
  run(args: Record<string, unknown>): Promise<unknown>;
}

export interface ServiceDeps {
  moodleFetch?: FetchLike;
  proxmoxFetch?: ProxmoxFetcher;
}

export interface ServiceDescriptor<Creds> {
  id: ServiceId;
  name: string;
  tagline: string;
  notes: string[];
  /** One-letter or glyph mark shown inside the colored logo circle. */
  mark: string;
  accent: string;
  fields: ServiceField[];
  allowWriteOptional: boolean;
  writeLabel: string;
  /** Validate raw form values, fetch tokens when possible and return saved credentials. */
  activate(raw: Record<string, unknown>, deps?: ServiceDeps): Promise<{ creds: Creds; lines: string[] }>;
  /** Quick check for the dashboard "Tester" button. */
  status(creds: Creds, deps?: ServiceDeps): Promise<string[]>;
  connect(creds: Creds, env: NodeJS.ProcessEnv): ServiceTool[];
}

export type AnyServiceDescriptor = ServiceDescriptor<MoodleConfig> | ServiceDescriptor<ProxmoxConfig>;

type SiteInfo = { userid?: number; sitename?: string; username?: string; release?: string; functions?: Array<{ name: string }> };

/* ---------------------------------------------------------------- Moodle */

function moodleLines(info: SiteInfo): string[] {
  const lines = [`Connecté à ${info.sitename ?? "Moodle"}.`];
  if (info.username) lines.push(`Compte : ${info.username}.`);
  lines.push(`Fonctions disponibles pour ce token : ${info.functions?.length ?? 0}.`);
  return lines;
}

/** Fetch the mobile-app token once from the site, confirming it works. */
async function registerMoodleToken(baseUrl: string, token: string, allowWrite: boolean, deps?: ServiceDeps): Promise<{ creds: MoodleConfig; lines: string[] }> {
  if (!token.trim()) throw new TokenError("Token vide.");
  const client = new MoodleClient({ baseUrl, token: token.trim(), timeoutMs: 30_000, allowWrite: false }, deps?.moodleFetch);
  try {
    const info = await client.siteInfo();
    const site = info as SiteInfo;
    if (!site.userid) throw new TokenError("Ce token n'identifie aucun compte Moodle.");
    return { creds: { baseUrl, token: token.trim(), timeoutMs: 30_000, allowWrite }, lines: moodleLines(site) };
  } catch (error) {
    if (error instanceof TokenError) throw error;
    throw new TokenError("Le token ne peut pas accéder à ce site Moodle. Vérifiez ses permissions et l'adresse du site.");
  }
}

export function moodleTools(config: MoodleConfig, client: MoodleClient): ServiceTool[] {
  const bundle = loadCatalog();
  const catalog = FunctionIndex.fromCatalog(bundle);
  let siteInfoPromise: Promise<SiteInfo> | undefined;
  const siteInfo = () => {
    // Retry on a transient failure rather than caching it until restart.
    siteInfoPromise ??= client.siteInfo().then((info) => info as SiteInfo).catch((error: unknown) => {
      siteInfoPromise = undefined;
      throw error;
    });
    return siteInfoPromise;
  };
  const availability = async () => new Set(((await siteInfo()).functions ?? []).map((fn) => fn.name));
  const call = async (name: string, args: Record<string, unknown> = {}) => {
    const available = await availability();
    if (!available.has(name)) throw new MoodleApiError(`Function ${name} is not available to this token.`, 0);
    const definition = catalog.get(name);
    if ((!definition || definition.type === "write") && !config.allowWrite) {
      throw new MoodleApiError(`Function ${name} may change Moodle data. Set MOODLE_ALLOW_WRITE=true to enable it.`, 0);
    }
    return client.call(name, args);
  };
  const tools: ServiceTool[] = [
    {
      name: "moodle_status",
      description: "Check the Moodle connection and show which site and account this token belongs to.",
      annotations: { title: "Vérifier la connexion Moodle", readOnlyHint: true },
      run: async () => {
        const { sitename, username, userid, release, functions } = await siteInfo();
        return { sitename, username, userid, release, availableFunctions: functions?.length ?? 0 };
      },
    },
    {
      name: "moodle_my_courses",
      description: "List the courses in which the connected student is enrolled.",
      annotations: { title: "Mes cours", readOnlyHint: true },
      run: async () => {
        const { userid } = await siteInfo();
        if (!userid) throw new MoodleApiError("Moodle did not provide a user ID for this token.", 0);
        return call("core_enrol_get_users_courses", { userid });
      },
    },
    {
      name: "moodle_course_content",
      description: "Get the sections, activities and resources in a course.",
      inputSchema: { courseid: z.number().int().positive() },
      annotations: { title: "Contenu d'un cours", readOnlyHint: true },
      run: async ({ courseid }) => call("core_course_get_contents", { courseid }),
    },
    {
      name: "moodle_assignments",
      description: "List visible assignments; optionally filter to specific course IDs.",
      inputSchema: { courseids: z.array(z.number().int().positive()).optional() },
      annotations: { title: "Travaux et dépôts", readOnlyHint: true },
      run: async ({ courseids }) => call("mod_assign_get_assignments", courseids ? { courseids } : {}),
    },
    {
      name: "moodle_quizzes",
      description: "List visible quizzes; optionally filter to specific course IDs.",
      inputSchema: { courseids: z.array(z.number().int().positive()).optional() },
      annotations: { title: "Quiz", readOnlyHint: true },
      run: async ({ courseids }) => call("mod_quiz_get_quizzes_by_courses", courseids ? { courseids } : {}),
    },
    {
      name: "moodle_grades",
      description: "Get the connected student's grade items for a course.",
      inputSchema: { courseid: z.number().int().positive() },
      annotations: { title: "Notes d'un cours", readOnlyHint: true },
      run: async ({ courseid }) => call("gradereport_user_get_grade_items", { courseid }),
    },
    {
      name: "moodle_calendar",
      description: "Get the upcoming Moodle calendar view; optionally filter to one course.",
      inputSchema: { courseid: z.number().int().positive().optional() },
      annotations: { title: "Calendrier", readOnlyHint: true },
      run: async ({ courseid }) => call("core_calendar_get_calendar_upcoming_view", courseid ? { courseid } : {}),
    },
    {
      name: "ws_discover",
      description: "Search Moodle Web Service functions this token can actually call, with descriptions and access type.",
      inputSchema: { query: z.string().default(""), limit: z.number().int().min(1).max(100).default(20) },
      annotations: { readOnlyHint: true },
      run: async ({ query: queryArg, limit: limitArg }) => {
        const query = typeof queryArg === "string" ? queryArg : "";
        const limit = typeof limitArg === "number" ? limitArg : 20;
        const available = await availability();
        const matches = catalog.search(query, catalog.size).filter((fn) => available.has(fn.name)).slice(0, limit);
        return { totalAvailable: available.size, results: matches.map(({ name, description, type }) => ({ name, description, type })) };
      },
    },
    {
      name: "ws_schema",
      description: "Show the parameters and usage details for an available Moodle Web Service function.",
      inputSchema: { name: z.string() },
      annotations: { readOnlyHint: true },
      run: async ({ name: nameArg }) => {
        const name = typeof nameArg === "string" ? nameArg : "";
        const available = await availability();
        if (!available.has(name)) throw new MoodleApiError(`Function ${name} is not available to this token.`, 0);
        const fn = catalog.get(name);
        return fn ? { name: fn.name, description: fn.description, type: fn.type, inputSchema: toJsonSchema(fn), capabilities: fn.capabilities, catalogVersion: bundle.moodle?.release }
          : { name, type: "unknown", note: "Custom or newer function: consult your Moodle site's API documentation. Calls require MOODLE_ALLOW_WRITE=true." };
      },
    },
    {
      name: "ws_call",
      description: "Call an available Moodle Web Service function by exact name. Find it with ws_discover and check parameters with ws_schema first. Writes require MOODLE_ALLOW_WRITE=true.",
      inputSchema: { name: z.string(), args: z.record(z.string(), z.unknown()).default({}) },
      annotations: { readOnlyHint: !config.allowWrite, destructiveHint: config.allowWrite },
      run: async ({ name, args }) => call(typeof name === "string" ? name : "", (args ?? {}) as Record<string, unknown>),
    },
  ];
  return tools;
}

export const moodleService: ServiceDescriptor<MoodleConfig> = {
  id: "moodle",
  name: "Moodle",
  tagline: "Cours, travaux, échéances et notes.",
  notes: ["Fonctionne avec le Moodle de Polytechnique Montréal ou n'importe quel Moodle."],
  mark: "M",
  accent: "#f98012",
  allowWriteOptional: true,
  writeLabel: "Autoriser les modifications (dépôts de fichiers, posts…)",
  fields: [
    { id: "url", label: "Adresse du site Moodle", type: "url", placeholder: "https://moodle.polymtl.ca", required: true },
    { id: "username", label: "Identifiant Moodle", type: "text", placeholder: "prenom.nom (optionnel si token)", required: false },
    { id: "password", label: "Mot de passe Moodle", type: "password", placeholder: "Une seule utilisation, jamais enregistré", required: false, help: "Le serveur récupère automatiquement le token de l'application mobile." },
    { id: "token", label: "Token (privatetoken)", type: "password", placeholder: "Si tu en as déjà un, sinon laisse vide", required: false },
  ],
  async activate(raw, deps) {
    const urlField = typeof raw.url === "string" ? raw.url : "";
    const token = typeof raw.token === "string" ? raw.token.trim() : "";
    const username = typeof raw.username === "string" ? raw.username.trim() : "";
    const password = typeof raw.password === "string" ? raw.password : "";
    const allowWrite = raw.allowWrite === true;
    if (!urlField.trim()) throw new TokenError("Entrez l'adresse du site Moodle.");
    const baseUrl = normalizeBaseUrl(urlField);
    if (token) return registerMoodleToken(baseUrl, token, allowWrite, deps);
    if (!username || !password) throw new TokenError("Entrez votre identifiant et votre mot de passe Moodle, ou collez un token.");
    const fetched = await requestMobileToken(baseUrl, username, password, deps?.moodleFetch);
    return registerMoodleToken(baseUrl, fetched, allowWrite, deps);
  },
  async status(config, deps) {
    const client = new MoodleClient(config, deps?.moodleFetch);
    try {
      const info = await client.siteInfo();
      return moodleLines(info as SiteInfo);
    } catch {
      throw new MoodleApiError("Ce token ne peut plus accéder au site Moodle. Reconfigurez-le sur la page.", 0);
    }
  },
  connect(config) {
    return moodleTools(config, new MoodleClient(config));
  },
};

/* --------------------------------------------------------------- Proxmox */

function proxmoxLines(version: { version?: string; release?: string }, nodes: Array<{ node?: string; status?: string }>): string[] {
  const lines = [`Proxmox VE ${version.release ?? version.version ?? "inconnu"} détecté.`];
  const online = nodes.filter((node) => node.status === "online").map((node) => node.node).filter(Boolean);
  lines.push(online.length ? `Nœuds accessibles : ${online.join(", ")}.` : "Aucun nœud visible : le token a peut-être des permissions limitées.");
  return lines;
}

async function proxmoxCheck(config: ProxmoxConfig, deps?: ServiceDeps): Promise<string[]> {
  const client = new ProxmoxClient(config, deps?.proxmoxFetch);
  const version = await client.version();
  let nodes: Array<{ node?: string; status?: string }> = [];
  try {
    nodes = await client.nodes();
  } catch {
    nodes = [];
  }
  return proxmoxLines(version, nodes);
}

export const proxmoxService: ServiceDescriptor<ProxmoxConfig> = {
  id: "proxmox",
  name: "Proxmox",
  tagline: "Serveurs, VM et conteneurs (LXC).",
  notes: ["Génère un token API sur ton Proxmox : Datacenter → Permissions → Token API.", "Crée un token à permissions limitées ; il reflète exactement ses droits numériques."],
  mark: "P",
  accent: "#e57000",
  allowWriteOptional: true,
  writeLabel: "Autoriser démarrage/extinction de machines",
  fields: [
    { id: "url", label: "Adresse du serveur Proxmox", type: "url", placeholder: "https://mon-pve.exemple.ca:8006", required: true },
    { id: "tokenId", label: "ID de token (utilisateur@domaine!token)", type: "text", placeholder: "etudiant@pam!mcp", required: true },
    { id: "secret", label: "Secret du token", type: "password", placeholder: "La valeur générée une seule fois", required: true },
  ],
  async activate(raw, deps) {
    const urlField = typeof raw.url === "string" ? raw.url : "";
    const tokenId = typeof raw.tokenId === "string" ? raw.tokenId.trim() : "";
    const secret = typeof raw.secret === "string" ? raw.secret.trim() : "";
    if (!urlField.trim()) throw new TokenError("Entrez l'adresse du serveur Proxmox.");
    if (!tokenId.includes("!")) throw new TokenError("L'ID de token ressemble à « etudiant@pam!mcp ».");
    if (!secret) throw new TokenError("Entrez le secret du token Proxmox.");
    const config: ProxmoxConfig = {
      baseUrl: normalizeProxmoxUrl(urlField),
      tokenId,
      secret,
      timeoutMs: 15_000,
      allowWrite: raw.allowWrite === true,
    };
    const lines = await proxmoxCheck(config, deps);
    return { creds: config, lines };
  },
  async status(config, deps) {
    return proxmoxCheck(config, deps);
  },
  connect(config) {
    const client = new ProxmoxClient(config);
    const requireWrite = () => {
      if (!config.allowWrite) throw new ProxmoxApiError("Les modifications Proxmox sont désactivées. Reconfigure Proxmox sur la page en autorisant l'écriture.", 403);
    };
    const servicePath = (type: string) => (type === "lxc" ? "lxc" : "qemu");
    const tools: ServiceTool[] = [
      {
        name: "proxmox_status",
        description: "Show the connected Proxmox server version and the nodes this token can see.",
        annotations: { title: "Connexion Proxmox", readOnlyHint: true },
        run: async () => ({ version: await client.version(), nodes: await client.nodes() }),
      },
      {
        name: "proxmox_list_vms",
        description: "List every VM and LXC container this Proxmox token can see, with node and status.",
        annotations: { title: "Machines visibles", readOnlyHint: true },
        run: async () => {
          const resources = await client.resources();
          return {
            count: resources.length,
            machines: resources.filter((entry) => entry.vmid && entry.node).map((entry) => ({
              vmid: entry.vmid,
              name: entry.name,
              node: entry.node,
              status: entry.status,
              type: entry.type,
            })),
          };
        },
      },
      {
        name: "proxmox_vm_status",
        description: "Show detailed status (memory, CPU, uptime) of one VM or LXC container by its VMID.",
        inputSchema: { vmid: z.number().int().positive() },
        annotations: { title: "Statut d'une machine", readOnlyHint: true },
        run: async ({ vmid }) => {
          const found = await locateResource(client, Number(vmid));
          const detail = await client.request(`/nodes/${found.node}/${servicePath(found.type)}/${found.vmid}/status/current`);
          return { vmid: found.vmid, node: found.node, type: found.type, status: found.status, detail };
        },
      },
      {
        name: "proxmox_vm_start",
        description: "Start a stopped VM or LXC container. Requires the Proxmox write toggle on the dashboard.",
        inputSchema: { vmid: z.number().int().positive() },
        annotations: { title: "Démarrer une machine", readOnlyHint: false },
        run: async ({ vmid }) => {
          requireWrite();
          const found = await locateResource(client, Number(vmid));
          if (found.status === "running") return { vmid: found.vmid, status: "running", note: "Machine déjà allumée." };
          const result = await client.request<{ upid?: string }>(`/nodes/${found.node}/${servicePath(found.type)}/${found.vmid}/status/start`, "POST");
          return { vmid: found.vmid, node: found.node, started: true, task: result?.upid ?? null };
        },
      },
      {
        name: "proxmox_vm_stop",
        description: "Stop (graceful shutdown, or hard poweroff with force=true) a running VM or LXC container. Destructive; requires the Proxmox write toggle.",
        inputSchema: { vmid: z.number().int().positive(), force: z.boolean().default(false) },
        annotations: { title: "Éteindre une machine", readOnlyHint: false, destructiveHint: true },
        run: async ({ vmid, force }) => {
          requireWrite();
          const found = await locateResource(client, Number(vmid));
          const result = await client.request<{ upid?: string }>(`/nodes/${found.node}/${servicePath(found.type)}/${found.vmid}/status/${force ? "stop" : "shutdown"}`, "POST");
          return { vmid: found.vmid, node: found.node, task: result?.upid ?? null, note: force ? "Poweroff demandé (extinction forcée)." : "Arrêt propre demandé." };
        },
      },
      {
        name: "proxmox_vm_reboot",
        description: "Reboot a running VM or LXC container. Requires the Proxmox write toggle.",
        inputSchema: { vmid: z.number().int().positive() },
        annotations: { title: "Redémarrer une machine", readOnlyHint: false, destructiveHint: true },
        run: async ({ vmid }) => {
          requireWrite();
          const found = await locateResource(client, Number(vmid));
          const result = await client.request<{ upid?: string }>(`/nodes/${found.node}/${servicePath(found.type)}/${found.vmid}/status/reboot`, "POST");
          return { vmid: found.vmid, node: found.node, task: result?.upid ?? null };
        },
      },
    ];
    return tools;
  },
};

export const SERVICES: readonly AnyServiceDescriptor[] = Object.freeze([moodleService, proxmoxService]);

/** Services that are planned but have no public API (shown disabled on the page). */
export const PLANNED_SERVICES: readonly { id: string; name: string; tagline: string; mark: string; accent: string }[] = Object.freeze([
  { id: "dossier", name: "Dossier étudiant", tagline: "Inscriptions, relevé, attestations. Aucune API publique pour l'instant.", mark: "D", accent: "#4a5361" },
  { id: "courriel", name: "Courriel Poly", tagline: "Boîte Microsoft 365 étudiante. Une API est à l'étude.", mark: "@", accent: "#4a5361" },
]);

export function serviceById(id: string): AnyServiceDescriptor | null {
  return SERVICES.find((service) => service.id === id) ?? null;
}

export interface ServiceState {
  descriptor: AnyServiceDescriptor;
  creds: MoodleConfig | ProxmoxConfig;
}

/** Every service whose stored or environment credentials resolve. */
export function activeServiceStates(env: NodeJS.ProcessEnv = process.env, settings: AppSettings = readSettings(env)): ServiceState[] {
  const states: ServiceState[] = [];
  const moodle = moodleConfigFrom(settings, env);
  if (moodle) states.push({ descriptor: moodleService, creds: moodle });
  const proxmox = proxmoxConfigFrom(settings, env);
  if (proxmox) states.push({ descriptor: proxmoxService, creds: proxmox });
  return states;
}

/** Map activated credentials back into the persisted settings file. */
export function credentialsToSettings(id: ServiceId, creds: MoodleConfig | ProxmoxConfig, allowWrite: boolean): { moodle?: AppSettings["moodle"]; proxmox?: AppSettings["proxmox"] } {
  if (id === "moodle") {
    const moodle = creds as MoodleConfig;
    return { moodle: { url: moodle.baseUrl, token: moodle.token, allowWrite } };
  }
  const proxmox = creds as ProxmoxConfig;
  return { proxmox: { url: proxmox.baseUrl, tokenId: proxmox.tokenId, secret: proxmox.secret, allowWrite } };
}

/* --------------------------------------------------------- Tool plumbing */

/** Convert service errors into safe MCP text results without leaking secrets. */
export async function runServiceTool(tool: ServiceTool, args: Record<string, unknown>): Promise<CallToolResult> {
  try {
    const data = await tool.run(args ?? {});
    return { content: [{ type: "text", text: JSON.stringify(data, null, 2) ?? "null" }] };
  } catch (error) {
    if (error instanceof MoodleApiError || error instanceof ProxmoxApiError || error instanceof TokenError) {
      return { isError: true, content: [{ type: "text", text: error.message }] };
    }
    return { isError: true, content: [{ type: "text", text: "Le service a échoué. Vérifie la configuration sur la page student-mcp." }] };
  }
}
