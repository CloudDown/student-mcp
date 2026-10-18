# Student MCP

> **English** — Student MCP is an MCP (Model Context Protocol) server that lets OpenCode or Claude Desktop read your Moodle courses, assignments, deadlines and grades. No Node.js or npm needed.
>
> **Install:** macOS/Linux: `curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash` — Windows (PowerShell): `irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.ps1 | iex`. The wizard asks for your Moodle URL (default `https://moodle.polymtl.ca`, any Moodle works) and credentials, then configures your app.
>
> *Not an official Polytechnique Montréal tool.* The rest of this README is in French.

Consulte tes cours Moodle avec OpenCode ou Claude Desktop. **Une commande, pas
de Node.js ni de npm à installer.**

## Installer

**Mac ou Linux** — colle ceci dans le Terminal :

```sh
curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash
```

**Windows** — colle ceci dans PowerShell :

```powershell
irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.ps1 | iex
```

Le programme choisit automatiquement la version pour ton ordinateur, vérifie
le téléchargement et lance l'assistant. Appuie sur **Entrée** pour garder
`https://moodle.polymtl.ca`, entre ton identifiant et ton mot de passe Moodle,
puis choisis **OpenCode** ou **Claude Desktop**. Redémarre l'application et
demande-lui « Vérifie ma connexion Moodle ».

Autre établissement ? Entre son adresse Moodle à la première question. Si tu
préfères télécharger un ZIP manuellement : [versions Windows, Mac et Linux](https://github.com/CloudDown/student-mcp/releases/latest).

## Quel token utiliser ?

L'assistant demande lui-même le token à Moodle en **POST HTTPS** : le mot de
passe n'est pas enregistré. Si tu obtiens une réponse JSON manuellement :

```json
{ "token": "EXEMPLE", "privatetoken": "AUTRE_VALEUR" }
```

**Utilise uniquement `token`**, jamais `privatetoken`. Ne mets pas ton vrai
mot de passe dans l'URL
`login/token.php?username=ID&password=PWD&service=moodle_mobile_app` : il
resterait dans l'historique. Si ton école utilise le SSO et refuse la connexion
dans l'assistant, relance l'installation, réponds **n** à la question sur
l'identifiant et colle un token personnel fourni par l'assistance Moodle.

Le connecteur n'accède qu'aux données autorisées par ton propre compte.
Les opérations qui modifient Moodle sont désactivées par défaut. Les
exécutables Windows et Mac ne sont pas signés : ton système peut demander une
confirmation au premier lancement.

Code source sous licence MIT; catalogue issu de Moodle sous GPL :
[licences](../catalog/LICENSE.md). Projet indépendant de Moodle et de
Polytechnique Montréal.
