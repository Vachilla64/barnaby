# Barnaby

A desktop pet for each of your AI coding agents. The agents tell their pet what they are doing, and the pet tells you out loud. Ask the pet about your codebase and it walks you through the code in VS Code, file by file, while it talks.

Built for the Meta Global AI Developer Hackathon with **Muse Spark** (Meta Model API) and **Phonon TTS** (runs locally).

Windows 10/11 only.

## What you need

| Thing | Why | How to get it |
|---|---|---|
| Node 20+ and pnpm | Build the app | `npm i -g pnpm` |
| Rust (stable) | Build the app | https://rustup.rs |
| uv | Runs the Phonon voice | Barnaby offers to install it in Settings |
| A Meta Model API key | Barnaby's brain | https://dev.meta.ai |
| VS Code | Code tours | https://code.visualstudio.com |

## Run it

```bash
pnpm install
pnpm tauri dev
```

The first start downloads the Phonon voice model, which takes a few minutes. Barnaby appears as a pet at the bottom right of your screen and as a tray icon.

## Set it up (Settings window: tray icon, then Settings)

1. **Voice:** if Phonon is not ready, press the install buttons. Pick a voice. The old-gentleman voices are `Master Aldric` and `Sir Bramwell`.
2. **Brain:** paste your Meta API key and press Test. The key is stored in Windows Credential Manager. The `MODEL_API_KEY` environment variable also works.
3. **Connect Claude Code:** press Connect. Barnaby shows exactly what it will change, then asks. It merges hooks into `~/.claude/settings.json` (a backup is saved) and registers an MCP server named `barnaby`. Restart your Claude Code sessions.
4. **Copy instructions snippet:** paste it into `CLAUDE.md` so the agent calls `barnaby_status` and `barnaby_done`.
5. **Gemini CLI or Antigravity (by hand):** add an MCP server named `barnaby` with URL `http://127.0.0.1:4747/mcp` and the header `Authorization: Bearer <token>`. The token is in `%APPDATA%\Barnaby\config.json`. The port is in the same file.

## Install the VS Code extension

```bash
cd vscode-extension
pnpm install
pnpm package
code --install-extension barnaby-vscode-0.1.0.vsix
```

A Barnaby item appears in the status bar. It turns green when the app is running.

## Use it

| You do | Barnaby does |
|---|---|
| Work with Claude Code | Speaks status updates, says when it needs your approval, and says when it has finished |
| Click the pet, then `Ask…`, or run `Barnaby: Ask about this` in VS Code | Answers out loud, then offers a tour |
| Say "show me" or "walk me through" in the question | Starts the tour at once |
| Press `Next`, `Back`, `Stop` in the bubble, or `Alt+]`, `Alt+[`, `Alt+Escape` in VS Code | Moves through the tour |

Try it on the sample app in `demo-project/`: open that folder in VS Code and ask "Where is the login handled?"

## How it works

- A Tauri 2 app (Rust and TypeScript) draws a transparent overlay across all monitors. Clicks pass through everywhere except the pet.
- A local server on `127.0.0.1:4747` (token required) takes MCP calls and Claude Code hooks from agents, and a WebSocket from the VS Code extension.
- Phonon runs as a child process in a Windows Job Object, so it dies when Barnaby quits.
- The brain sends a map of your project (and the full source if it is small) to Muse Spark, with tools to read files and search. It only reads inside the project folder you open in VS Code.

Product spec: [docs/SPEC.md](docs/SPEC.md). Build progress and known gaps: [docs/TASKS.md](docs/TASKS.md). Coding agents: start with [AGENTS.md](AGENTS.md).

The old pure-Rust pet prototype stays in `prototype/` as reference.
