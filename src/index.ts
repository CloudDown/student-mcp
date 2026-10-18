#!/usr/bin/env node
import { spawn } from "node:child_process";
import { createInterface, emitKeypressEvents } from "node:readline";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { requestMobileToken, verifyToken } from "./auth.js";
import { VERSION, gatewayConfigFrom, loadConfig, normalizeBaseUrl, readSettings, writeSettings } from "./config.js";
import { configureClient, installBinary, installedBinaryPath, type ClientName } from "./install.js";
import { createServer } from "./server.js";
import { startWebServer, type WebServer } from "./web.js";
import { publicConnectorUrl, tailscaleInfo } from "./funnel.js";

async function readSecret(label: string): Promise<string> {
  if (!process.stdin.isTTY || !process.stdin.setRawMode) {
    throw new Error("L'installation interactive demande un terminal. Sinon, configurez MOODLE_URL et MOODLE_TOKEN dans votre client MCP.");
  }
  process.stderr.write(`${label} (hidden): `);
  emitKeypressEvents(process.stdin);
  process.stdin.setRawMode(true);
  process.stdin.resume();
  return new Promise((resolve, reject) => {
    let token = "";
    const finish = (error?: Error) => {
      process.stdin.off("keypress", onKey);
      process.stdin.setRawMode(false);
      process.stdin.pause();
      process.stderr.write("\n");
      if (error) reject(error);
      else resolve(token);
    };
    const onKey = (character: string, key: { name?: string; ctrl?: boolean }) => {
      if (key.ctrl && key.name === "c") return finish(new Error("Installation annulée."));
      if (key.name === "return" || key.name === "enter") return finish();
      if (key.name === "backspace") token = token.slice(0, -1);
      else if (character && character >= " " && character !== "\u007f") token += character;
    };
    process.stdin.on("keypress", onKey);
  });
}

async function init(): Promise<void> {
  if (!process.stdin.isTTY) throw new Error("L'installation interactive demande un terminal. Sinon, lancez `student-mcp up` et configurez sur la page.");
  const rl = createInterface({ input: process.stdin, output: process.stderr });
  let url: string;
  let method: string;
  let useCredentials = false;
  let username = "";
  try {
    if (readSettings(process.env).moodle?.token) {
      const answer = await new Promise<string>((resolve) => rl.question("Remplacer la configuration Moodle actuelle ? [o/N] ", resolve));
      if (!/^(?:o|oui|y|yes)$/i.test(answer.trim())) {
        process.stderr.write("Configuration conservée.\n");
        return;
      }
    }
    const site = await new Promise<string>((resolve) => rl.question("Adresse de votre Moodle [https://moodle.polymtl.ca] : ", resolve));
    url = normalizeBaseUrl(site.trim() || "https://moodle.polymtl.ca");
    method = (await new Promise<string>((resolve) => rl.question("Obtenir un token avec votre identifiant et mot de passe Moodle ? [O/n] ", resolve))).trim().toLowerCase();
    if (!["", "o", "oui", "y", "yes", "n", "non", "no"].includes(method)) throw new Error("Répondez o ou n pour choisir la méthode.");
    useCredentials = !method || ["o", "oui", "y", "yes"].includes(method);
    if (useCredentials) {
      username = await new Promise<string>((resolve) => rl.question("Identifiant Moodle (ID) : ", resolve));
      if (!username.trim()) throw new Error("Identifiant vide : rien n'a été enregistré.");
    }
  } finally {
    rl.close();
  }
  const token = useCredentials
    ? await requestMobileToken(url, username, await readSecret("Mot de passe Moodle"))
    : await readSecret("Collez la valeur de token (pas privatetoken)");
  if (!token.trim()) throw new Error("Token vide : rien n'a été enregistré.");
  await verifyToken(url, token);
  writeSettings({ moodle: { url, token: token.trim() } });
  process.stderr.write(`Connexion vérifiée. Token enregistré dans ~/.student-mcp.json. Le mot de passe n'a pas été enregistré.\n`);
}

async function install(): Promise<void> {
  const bun = (globalThis as typeof globalThis & { Bun?: { isStandaloneExecutable?: boolean } }).Bun;
  if (!bun?.isStandaloneExecutable) {
    throw new Error("La commande install utilise l'exécutable GitHub. Avec npm, lancez `student-mcp up` pour la page de configuration.");
  }
  const executable = installBinary(process.execPath, process.env.MOODLE_INSTALL_PATH || installedBinaryPath());
  const rl = createInterface({ input: process.stdin, output: process.stderr });
  let choice: string;
  try {
    choice = (await new Promise<string>((resolve) => rl.question("Mode : 1 client IA local (stdio), 2 page web + connecteur [2] : ", resolve))).trim();
  } finally {
    rl.close();
  }
  if (choice === "" || choice === "2") {
    launchDetached(executable);
    return;
  }
  if (choice !== "1") throw new Error(`Choix incorrect; le connecteur est installé dans ${executable}.`);
  await init();
  const rl2 = createInterface({ input: process.stdin, output: process.stderr });
  let inner: string;
  try {
    inner = (await new Promise<string>((resolve) => rl2.question("Votre application IA : 1 OpenCode, 2 Claude Desktop, 3 autre [1] : ", resolve))).trim();
  } finally {
    rl2.close();
  }
  if (inner === "3") {
    process.stderr.write(`Ajoutez ${executable} comme serveur MCP local (stdio) dans votre application IA.\n`);
    return;
  }
  if (!["", "1", "2"].includes(inner)) throw new Error(`Choix incorrect; le connecteur est installé dans ${executable}.`);
  const client: ClientName = inner === "2" ? "claude" : "opencode";
  const path = configureClient(client, executable);
  process.stderr.write(`${client === "claude" ? "Claude Desktop" : "OpenCode"} configuré dans ${path}. Redémarrez l'application et demandez « Vérifie ma connexion Moodle ».\n`);
}

function launchDetached(executable: string): void {
  const child = spawn(executable, ["up", "--open"], { detached: true, stdio: "ignore", windowsHide: true });
  child.unref();
  process.stderr.write("student-mcp démarre : la page s'ouvre dans ton navigateur. Configure tes services là.\n");
}

async function openBrowser(url: string): Promise<void> {
  const binary = process.platform === "darwin" ? "open" : process.platform === "win32" ? "cmd" : "xdg-open";
  const args = process.platform === "win32" ? ["/c", "start", "", url] : [url];
  try {
    spawn(binary, args, { detached: true, stdio: "ignore", windowsHide: true }).unref();
  } catch {
    // Headless or exotic environment: the printed URL is the fallback.
  }
}

function envNumber(value: string | undefined): number | null {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

async function up(flags: string[] = []): Promise<void> {
  const settings = readSettings();
  const gateway = gatewayConfigFrom(settings);
  const explicitPort = flags.map((flag) => /--port=(\d+)/.exec(flag)?.[1]).find(Boolean);
  const port = (explicitPort ? Number(explicitPort) : null) ?? envNumber(process.env.STUDENT_MCP_PORT) ?? gateway.port;
  const host = process.env.STUDENT_MCP_HOST || "127.0.0.1";

  let web: WebServer;
  try {
    web = await startWebServer({ port, host });
  } catch (error) {
    if ((error as { code?: string }).code === "EADDRINUSE") {
      throw new Error(`Le port ${port} est déjà pris. Un student-mcp tourne peut-être déjà, sinon lance avec --port=12345.`);
    }
    throw error;
  }

  const tailscale = await tailscaleInfo();
  const publicUrl = tailscale.loggedIn && tailscale.host && tailscale.funnelOn ? publicConnectorUrl(tailscale.host, gateway.token) : null;
  process.stderr.write([
    `student-mcp v${VERSION}`,
    `  Page (configurer les services) : ${web.localUrl}`,
    `  Connecteur local  : http://localhost:${port}/mcp/${gateway.token}`,
    publicUrl ? `  Connecteur public : ${publicUrl}` : tailscale.present ? "  Public : expose le lien depuis la page (bouton Tailscale)." : "  Public : installe Tailscale, puis expose depuis la page.",
    `  Dans Claude : Paramètres → Connecteurs → Ajouter un connecteur personnalisé.`,
    `  Ctrl-C pour arrêter.`,
    "",
  ].join("\n"));
  if (!flags.includes("--no-open")) openBrowser(host === "127.0.0.1" ? web.localUrl : `${web.localUrl}/?token=${gateway.token}`);

  await new Promise<void>((resolve) => {
    const terminate = () => resolve();
    process.on("SIGINT", terminate);
    process.on("SIGTERM", terminate);
  });
  process.stderr.write("\nArrêt de student-mcp.\n");
  await web.stop();
}

async function main(): Promise<void> {
  const [command = "", ...rest] = process.argv.slice(2);
  if (command === "up") return up(rest);
  if (command === "init") return init();
  if (command === "install") return install();
  if (command === "version" || command === "--version") {
    process.stderr.write(`student-mcp ${VERSION}\n`);
    return;
  }
  if (command === "--help" || command === "-h") {
    process.stderr.write([
      "Utilisation :",
      "  student-mcp up [--port=N] [--open]   Page de configuration + MCP local (Tailscale pour le lien public)",
      "  student-mcp init                     Configure Moodle en terminal (mode stdio)",
      "  student-mcp install                  Installeur GitHub (binaire + configuration)",
      "  student-mcp (sans argument, pour un client IA local) : serveur stdio MCP",
      "",
      "Config : ~/.student-mcp.json · Env : MOODLE_URL/MOODLE_TOKEN, STUDENT_MCP_PORT, STUDENT_MCP_TOKEN",
    ].join("\n"));
    return;
  }
  if (command) throw new Error("Unknown command. Use --help or up.");
  const server = createServer(loadConfig());
  await server.connect(new StdioServerTransport());
}

main().catch((error: unknown) => {
  process.stderr.write(`${error instanceof Error ? error.message : "Failed to start student-mcp"}\n`);
  process.exitCode = 1;
});
