#!/usr/bin/env node
import { chmodSync, existsSync, writeFileSync } from "node:fs";
import { createInterface, emitKeypressEvents } from "node:readline";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { requestMobileToken, verifyToken } from "./auth.js";
import { configPath, loadConfig, normalizeBaseUrl } from "./config.js";
import { configureClient, installBinary, installedBinaryPath, type ClientName } from "./install.js";
import { createServer } from "./server.js";

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
  if (!process.stdin.isTTY) throw new Error("L'installation interactive demande un terminal. Sinon, configurez MOODLE_URL et MOODLE_TOKEN dans votre client MCP.");
  const path = configPath();
  const rl = createInterface({ input: process.stdin, output: process.stderr });
  let url: string;
  let method: string;
  let useCredentials = false;
  let username = "";
  const exists = existsSync(path);
  try {
    if (exists) {
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
  writeFileSync(path, JSON.stringify({ url, token: token.trim() }, null, 2) + "\n", { mode: 0o600, flag: exists ? "w" : "wx" });
  chmodSync(path, 0o600);
  process.stderr.write(`Connexion vérifiée. Token enregistré dans ${path}. Le mot de passe n'a pas été enregistré.\n`);
}

async function install(): Promise<void> {
  const bun = (globalThis as typeof globalThis & { Bun?: { isStandaloneExecutable?: boolean } }).Bun;
  if (!bun?.isStandaloneExecutable) {
    throw new Error("La commande install utilise l'exécutable GitHub. Avec npm, lancez `student-mcp init`.");
  }
  await init();
  const executable = installBinary(process.execPath, process.env.MOODLE_INSTALL_PATH || installedBinaryPath());
  process.stderr.write(`Connecteur installé dans ${executable}.\n`);
  const rl = createInterface({ input: process.stdin, output: process.stderr });
  let choice: string;
  try {
    choice = (await new Promise<string>((resolve) => rl.question("Votre application IA : 1 OpenCode, 2 Claude Desktop, 3 autre [1] : ", resolve))).trim();
  } finally {
    rl.close();
  }
  if (choice === "3") {
    process.stderr.write(`Ajoutez ${executable} comme serveur MCP local (stdio) dans votre application IA.\n`);
    return;
  }
  if (!["", "1", "2"].includes(choice)) throw new Error(`Choix incorrect; le connecteur est installé dans ${executable}.`);
  const client: ClientName = choice === "2" ? "claude" : "opencode";
  const path = configureClient(client, executable);
  process.stderr.write(`${client === "claude" ? "Claude Desktop" : "OpenCode"} configuré dans ${path}. Redémarrez l'application et demandez « Vérifie ma connexion Moodle ».\n`);
}

async function main(): Promise<void> {
  const command = process.argv[2];
  if (command === "init") return init();
  if (command === "install") return install();
  if (command === "--help" || command === "-h") {
    process.stderr.write("Utilisation : student-mcp [install|init]\nDepuis GitHub, utilisez install; avec npm, init.\n");
    return;
  }
  if (command) throw new Error("Unknown command. Use --help or init.");
  const server = createServer(loadConfig());
  await server.connect(new StdioServerTransport());
}

main().catch((error: unknown) => {
  process.stderr.write(`${error instanceof Error ? error.message : "Failed to start Moodle MCP"}\n`);
  process.exitCode = 1;
});
