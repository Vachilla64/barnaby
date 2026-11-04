# Devy: product spec

Devy is a Windows desktop app that puts a small animated pet on screen for each AI coding agent you run (Claude Code, Gemini CLI, Antigravity). The agents tell their pet what they are doing, and the pet tells you, out loud. You can also ask a pet about your own codebase: it explains the code, out loud and in VS Code, opening files and highlighting lines as it talks, as if you were walking through the code yourself.

Built for the Meta Global AI Developer Hackathon (indie track). **Submissions close 2026-11-04, 9:00 PM ET.** Scope is cut to fit that deadline. See [Phases](#9-phases-and-acceptance-criteria).

---

## 1. The problem

People who build with coding agents ("vibe coding") hit two walls:

1. **They lose track of what the agents are doing.** Agents run in terminals that are hidden behind other windows. You find out the agent stopped 20 minutes ago to ask for a permission.
2. **They stop understanding their own codebase.** The agent wrote most of it. When something breaks, the person no longer knows where anything lives.

Devy fixes both with a companion that is always on screen, speaks, and knows the code.

## 2. Users and the demo story

**User:** a solo developer on Windows who runs one or two coding agents and VS Code.

**The demo (about 2 minutes) must show, in this order:**

1. Devy is running. Two pets sit on the bottom edge of the screen: Claude's (orange) and Gemini's (blue).
2. The user gives Claude Code a task in a terminal and switches to the browser. Claude's pet says "Starting on the login form" and later "Done. I added a form and a validation helper."
3. Claude needs a permission. Its pet bounces and says "Claude needs your approval in the terminal."
4. The user holds the push-to-talk key and asks Gemini's pet: "Where is the login validation handled?"
5. The pet answers out loud. VS Code opens `src/auth/validate.ts`, scrolls to the function, and highlights it while the pet explains it, then moves on to the next file in the tour.
6. The user says "next" and "stop", and the tour obeys.

Everything that does not serve this story is out of scope.

## 3. Decisions already made

| Topic | Decision |
| :-- | :-- |
| Platform | Windows 10/11 only |
| App shell | **Tauri 2** (Rust backend, web frontend in Vite + TypeScript, no UI framework) |
| Overlay | One full-screen, transparent, always-on-top, click-through window, as in [PLAYBOOK.md](PLAYBOOK.md) |
| Agent link | **MCP server** (Streamable HTTP) **plus HTTP hook endpoints**, both on one local server |
| Pets | **One pet per connected agent**, each with its own colour and voice |
| Pet art | Drawn in code (SVG and CSS), no image assets |
| Voice out | **Phonon TTS** (`ptts`, model `gradium/phonon-31m-en`), run locally by Devy |
| Voice in | Push-to-talk, transcribed by Meta's `muse-voice-transcribe-1.0` |
| Brain | **Muse Spark** (`muse-spark-1.3`) through the Meta Model API. Devy asks it directly; it does not relay questions to the coding agents |
| Code walkthrough | A VS Code extension that Devy drives over a WebSocket |
| Existing code | The pure-Rust prototype in `prototype/` is reference only. Port ideas, not code |

## 4. Architecture

```
 Claude Code ─┐  MCP (HTTP) + hooks (curl)
 Gemini CLI  ─┼──────────────────────────────┐
 Antigravity ─┘                              ▼
                              ┌────────────────────────────────┐
                              │ Devy app (Tauri 2, Rust)       │
   Meta Model API ◀── HTTPS ──│  local server 127.0.0.1:4747   │
   (Muse Spark, transcribe)   │   /mcp  /api/v1/*  /ws         │
                              │  agents · brain · tours · voice│
   Phonon server ◀── HTTP ────│  phonon manager (sidecar)      │
   127.0.0.1:8001             └──────┬─────────────────┬───────┘
                                     │ Tauri events    │ WebSocket
                              ┌──────▼──────┐   ┌──────▼──────────┐
                              │ Overlay page│   │ VS Code ext.    │
                              │ pets, bubble│   │ opens, highlights│
                              └─────────────┘   └─────────────────┘
```

### 4.1 Repo layout (target)

```
/                      README.md, AGENTS.md, GEMINI.md, package.json
/docs                  SPEC.md (this file), TASKS.md, PLAYBOOK.md
/src                   overlay + settings pages (TypeScript, HTML, CSS)
/src-tauri             Rust backend
  /src
    main.rs            setup, window creation, tray
    server/            axum server: mod.rs, mcp.rs, hooks.rs, ws.rs, auth.rs
    agents.rs          agent registry (one pet per agent)
    overlay/           clickthrough.rs (cursor watcher), monitors.rs
    voice/             phonon.rs (sidecar + client), queue.rs (speech queue), playback.rs
    brain/             muse.rs (API client), index.rs (repo index), tools.rs, ask.rs
    tours.rs           tour engine
    config.rs          settings file + API key in Windows Credential Manager
/resources/phonon      phonon_server.py (ships with the app)
/vscode-extension      the VS Code extension (TypeScript)
/prototype             the old pure-Rust pet, reference only
```

### 4.2 Processes and ports

| Process | Port | Started by |
| :-- | :-- | :-- |
| Devy app | `127.0.0.1:4747` (HTTP + WebSocket) | the user |
| Phonon TTS server | `127.0.0.1:8001` | Devy, if the port is not already answering `GET /health` |
| VS Code extension | client only | VS Code |

Bind to `127.0.0.1` only, never `0.0.0.0`. If 4747 is taken, try 4748 to 4757 and write the port used to the settings file.

### 4.3 Local auth

On first run Devy creates a random 32-byte token (hex) and stores it in `%APPDATA%\Devy\config.json`. Every request to `/mcp`, `/api/v1/*` and `/ws` must send `Authorization: Bearer <token>`, or `?token=<token>` for the WebSocket. `GET /health` needs no token. Reject anything else with 401. This stops web pages in the user's browser from driving the pet.

## 5. Components

### 5.1 Overlay and pets (frontend `src/overlay`)

Follow [PLAYBOOK.md](PLAYBOOK.md) sections "The shape of the solution" and "How each piece was made to work", rows *Full-screen overlay*, *Click-through* and *Dragging*. Window pinning, virtual desktops and hiding behind windows are **out of scope** for the hackathon.

- **Window:** created in Rust, sized to the union of all monitors, `transparent: true`, `decorations: false`, `alwaysOnTop: true`, `skipTaskbar: true`, `focus: false`. It never moves.
- **Click regions:** the page sends `set_click_regions([{x, y, w, h}])` (physical pixels) whenever a pet, bubble or menu moves, appears or disappears. A Rust thread polls the cursor every 8 ms and calls `set_ignore_cursor_events(false)` only while the cursor is inside a region. Use a generation counter so only one watcher runs. While a drag is in progress the window stays interactive.
- **Pet:** an SVG blob about 72 px wide, with eyes and a mouth, coloured by agent. States, each a CSS class with its own animation:
  `idle` (slow breathing), `working` (small bounce, a spinning gear above it), `talking` (mouth opens and closes while audio plays), `thinking` (eyes look up, three dots), `alert` (fast bounce, `!` above it), `happy` (jump once), `sleeping` (eyes closed, `z`, when its agent disconnected).
- **Placement:** pets sit on the bottom edge of the primary monitor above the taskbar, spaced 120 px apart, from the right. The user can drag them; the position is saved per agent kind.
- **Speech bubble:** above the pet, max 320 px wide, shows the sentence being spoken. Under it, small buttons when relevant: `Next`, `Back`, `Stop` (during a tour), `Open terminal` (on an alert).
- **Pet menu** (left-click a pet): `Ask…` (opens a text box), `Mute`, `Repeat last`, `Hide for 1 hour`. Right-click: `Settings`, `Quit`.
- **Activity feed:** hovering a pet for 1 s shows its last 5 status lines with times.

### 5.2 Settings window (frontend `src/settings`)

A normal small window (640×560), opened from the tray icon or the pet menu. Sections:

1. **Meta API key:** password field, `Test` button (calls the API with a 5-token prompt), saved in Windows Credential Manager (`keyring` crate, service `Devy`).
2. **Voice:** status of Phonon (`installing / starting / ready / failed`), voice per agent kind (defaults: Claude `Freya`, Gemini `Easton`, other agents `Freya`), volume, mute all.
3. **Push-to-talk key:** default `Ctrl+Alt+Space` (hold to talk).
4. **Connect agents:** for Claude Code and Gemini CLI, a `Connect` button that runs the setup in [5.4](#54-agent-link-mcp-and-hooks), and a `Copy instructions` button that copies the snippet from [5.4.4](#544-instructions-snippet-for-the-agents-memory-file).
5. **Projects:** list of known project folders (added automatically by the VS Code extension or MCP calls, or by `Add folder`).
6. **Privacy note:** "Questions and the code needed to answer them are sent to Meta's Model API. Nothing else leaves this computer."

### 5.3 Voice out: Phonon

**Setup on first run** (Rust `voice/phonon.rs`), shown as a progress line in settings and spoken by nobody until done:

1. If `GET http://127.0.0.1:8001/health` answers `ok`, use it and stop here.
2. Find `uv` (`where uv`). If missing, install it: `powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"`. Ask the user first, in the settings window, with an `Install` button. Never install anything silently.
3. If `uv tool list` does not show `ptts`, run `uv tool install ptts`.
4. Start the server as a hidden child process (`CREATE_NO_WINDOW`): `uv tool run --from ptts python <resource dir>/phonon/phonon_server.py`. Kill it when Devy quits (Windows Job Object so it dies even on a crash).
5. Poll `/health` every second for up to 180 s. The first start downloads and loads the model, which takes about 2 minutes. After that it is cached.

**Synthesis:** `POST /tts` with form fields `text` and `voice_url` (the voice name, for example `Freya`). The reply is a streamed WAV, 24 kHz, mono, 16-bit. First audio arrives in about 50 ms.

**Speech queue** (`voice/queue.rs`), one queue for the whole app, so pets never talk over each other:

- Each item: `{agent_id, text, priority: low|normal|high, kind: status|answer|tour|alert}`.
- Split text into sentences. Synthesize sentence N+1 while sentence N plays.
- Play with `rodio` in Rust. Emit Tauri events `speech:start {agent_id, sentence}` and `speech:end {agent_id}` so the page animates the mouth and fills the bubble.
- **Throttle:** a pet speaks at most one `low` or `normal` status per 20 s. Extra statuses only update the bubble and feed. `high` (alerts, answers, tours) always speaks and interrupts lower priority speech.
- `stop_speech()` clears the queue and stops playback at once (used by "stop" and barge-in).
- If Phonon is not ready, show the bubble without audio. Never block.

### 5.4 Agent link: MCP and hooks

#### 5.4.1 Agent registry (`agents.rs`)

An agent is one connected coding-agent session. Fields: `id` (MCP session id, or hook `session_id`), `kind` (`claude-code`, `gemini-cli`, `antigravity`, `other`, from the MCP `initialize` `clientInfo.name` or the hook URL), `display_name`, `colour`, `voice`, `project_root`, `state`, `last_seen`, `feed` (last 20 lines).

- New `kind` that has no pet yet → create a pet. Several sessions of the same kind share one pet (the pet stands for "Claude", not for one terminal).
- No message for 10 minutes → `sleeping`. 60 minutes → remove the pet.

#### 5.4.2 MCP server (`server/mcp.rs`)

Streamable HTTP transport at `POST /mcp` (JSON-RPC 2.0, MCP protocol `2025-06-18` or newer). Use the `rmcp` crate if it fits the axum server; otherwise implement `initialize`, `tools/list` and `tools/call` by hand (that is all that is needed). Tools:

| Tool | Input | What Devy does | Returns |
| :-- | :-- | :-- | :-- |
| `devy_status` | `summary` (≤ 25 words), `phase`: `planning \| working \| testing \| blocked \| done`, `project_root?` | Pet state from phase; add to feed; speak at `normal` priority (throttled) | `"ok"` |
| `devy_say` | `text` (≤ 60 words), `urgent?` bool | Speak, `high` if urgent | `"ok"` |
| `devy_done` | `summary` (≤ 40 words), `files_changed?` [paths] | `happy` state, speak at `high` | `"ok"` |
| `devy_ask_user` | `question`, `options?` [strings, ≤ 4] | Speak the question, show options as bubble buttons, wait up to 5 min for a click or spoken answer | the answer text, or `"no answer"` |
| `devy_tour` | `title`, `steps`: [`{path, start_line, end_line, narration}`] (≤ 12) | Start a code tour ([5.7](#57-tours)) | `"started"` |

Tool descriptions must tell the model when to call them, for example: "Call devy_status when you start a task, at each milestone, and when you are blocked. Keep summaries short and spoken-style. Do not call it more than once a minute."

#### 5.4.3 Hooks (`server/hooks.rs`)

`POST /api/v1/hooks/{kind}` takes the hook's JSON payload as the body unchanged. Map:

| Claude Code hook | Devy reaction |
| :-- | :-- |
| `SessionStart` | Register agent, pet wakes up, no speech |
| `UserPromptSubmit` | State `working`, no speech |
| `Notification` (permission or waiting for input) | State `alert`, speak at `high`: "Claude needs your approval in the terminal." / "Claude is waiting for you." |
| `PostToolUse` with `Edit`, `Write`, `MultiEdit` | Add "edited `<file>`" to feed, no speech |
| `Stop` | State `idle`; if no `devy_done` came in the last 30 s, speak "Claude has finished." |
| `SessionEnd` | State `sleeping` |

Gemini CLI: check its current hooks documentation and map the equivalent events. If it has no hooks, MCP alone is enough.

**Connect button for Claude Code** (writes config; show the user exactly what will change and ask before writing):

- MCP: run `claude mcp add --transport http --scope user devy http://127.0.0.1:4747/mcp --header "Authorization: Bearer <token>"`.
- Hooks: merge into `%USERPROFILE%\.claude\settings.json` under `hooks`, one entry per event above, each a command hook:
  `curl -s -m 2 -X POST http://127.0.0.1:4747/api/v1/hooks/claude-code -H "Authorization: Bearer <token>" -H "Content-Type: application/json" --data-binary @-`
  (`curl` ships with Windows 10+. `-m 2` keeps a closed Devy from slowing the agent down.) Keep existing hooks; never overwrite the file.

**Connect button for Gemini CLI:** merge into `%USERPROFILE%\.gemini\settings.json`:
`"mcpServers": {"devy": {"httpUrl": "http://127.0.0.1:4747/mcp", "headers": {"Authorization": "Bearer <token>"}}}`.

**Antigravity:** document the manual MCP setup in the README; no button.

#### 5.4.4 Instructions snippet for the agent's memory file

Copied by the `Copy instructions` button, for the user to paste into `CLAUDE.md`, `GEMINI.md` or `AGENTS.md`:

```
## Devy (desktop pet)
A desktop pet reads your progress to the user out loud. Use the `devy` MCP tools:
- devy_status at the start of a task, at each milestone, and when blocked. One short spoken sentence.
- devy_done when the task is finished: what changed, in plain words.
- devy_ask_user instead of stopping when you need a quick decision.
- devy_tour when the user asks you to walk them through code you wrote.
Never put secrets or long code in these calls.
```

### 5.5 Brain: Muse Spark (`brain/`)

**API** (verified 2026-11-03 from dev.meta.ai docs):

- Base URL `https://api.meta.ai/v1`, header `Authorization: Bearer <key>`. Env var name in Meta's docs: `MODEL_API_KEY` (Devy reads it as a fallback if no key is saved).
- Use the **OpenAI-compatible Chat Completions** endpoint (`POST /chat/completions`) with `model: "muse-spark-1.3"`, tools, and streaming. Check the Chat Completions and Tool calling reference pages before writing the client and note any differences in `docs/TASKS.md`.
- Context window: 1,048,576 tokens. Price per million tokens: $1.25 input, $0.15 cached input, $4.25 output.
- Model id is a setting (`brain.model`), so it can switch to `muse-spark-1.3-contributor` (cheaper) or `muse-wonder-1.0`.

**Repo index** (`brain/index.rs`), built when a project is first used and refreshed on file change (debounced 5 s, `notify` crate):

- Walk the project with the `ignore` crate (respects `.gitignore`). Skip binaries, lock files, `node_modules`, build output, and files over 200 KB.
- Keep: path, size, line count, language, and a symbol list (functions, classes, exports) found with simple per-language regexes (TS/JS, Python, Rust, Go are enough).
- Produce a **repo map**: the file tree with the symbols of each file, as text.
- Estimate tokens as `chars / 4`. If the whole project is under 400,000 tokens, also keep the full text of every file.

**Answering a question** (`brain/ask.rs`):

1. Input: question text, `project_root`, and optional VS Code context (active file, selection, cursor line).
2. Messages: a system prompt (below), the repo map, and either the full source (small repo) or nothing (large repo), then the question and the editor context.
3. Tools the model can call (large repos, and allowed on small ones too): `read_file(path, start_line?, end_line?)` (max 400 lines per call), `search(pattern, glob?)` (regex, max 50 hits, via the `grep` crate), `list_dir(path)`. Paths must stay inside `project_root`. Max 8 tool rounds.
4. Final answer is structured JSON (use structured output if the API supports it; otherwise ask for JSON and validate with `serde`, retry once on bad JSON):

```json
{
  "speech": "Short spoken answer, 2-4 sentences, no code, no markdown.",
  "tour": [
    {"path": "src/auth/validate.ts", "start_line": 12, "end_line": 40,
     "narration": "This is where the email and password are checked. ..."}
  ]
}
```

`tour` may be empty. Validate every step: the file exists, lines are in range (clamp them), at most 12 steps.

5. Speak `speech` (`high`). If `tour` is not empty and VS Code is connected, ask in the bubble "Want me to show you?" with `Show me` / `No thanks`, or start it at once if the question said "show me" or "walk me through".

**System prompt (starting point):**

> You are Devy, a friendly guide who explains a codebase to the person who owns it. They may not have written most of it. Answer in plain spoken English, as if talking next to them: short sentences, no markdown, no code read aloud, file names said simply ("the validate file in auth"). Only state what the code shows; read files when unsure. When the answer lives in specific places, return a tour: an ordered walk through the files, each step with a narration of one to three sentences that says what this piece does and how it connects to the next.

**Pet voice:** questions are answered by the pet the user clicked or spoke to; any pet can answer about any project. If several projects are known, use the one open in VS Code; else the most recent one.

### 5.6 Voice in: push-to-talk

- Global shortcut (Tauri `global-shortcut` plugin), default `Ctrl+Alt+Space`. Hold to record; release to send. Recording under 0.4 s is ignored.
- Record in the overlay page with `getUserMedia` + `MediaRecorder` (WebView2 supports it; grant the permission in the Tauri config), send the audio bytes to Rust.
- The nearest pet to the cursor shows `thinking` while listening and transcribing. If the user clicked a pet in the last 10 s, that pet answers instead.
- Transcribe with `muse-voice-transcribe-1.0`. Check Meta's docs for the endpoint (probably OpenAI-style `POST /audio/transcriptions`) and accepted formats; convert to WAV 16 kHz mono in Rust if needed.
- Pressing push-to-talk stops any speech at once (barge-in).
- **Command words first:** before sending to the brain, match the text against tour and control commands: `next`, `back`, `previous`, `stop`, `pause`, `continue`, `repeat`, `mute`, `be quiet`, and answers to an open `devy_ask_user`. Only claim a sentence when it really is a command (whole utterance, or starts with the command word and is under 5 words). Otherwise it is a question for the brain. This matcher is pure Rust and must have unit tests.
- Fallback when no key or transcription fails: the pet says "I didn't catch that" and opens the `Ask…` text box.

### 5.7 Tours (`tours.rs` and the VS Code extension)

A tour is a list of steps `{path, start_line, end_line, narration}` from the brain or from `devy_tour`. The tour engine runs one tour at a time:

1. Send `tour.step` to VS Code. Wait for `tour.step.shown` (timeout 3 s; go on anyway).
2. Speak the narration (`tour` kind). Bubble shows `Back` `Next` `Stop`.
3. When speech ends, wait 1.2 s, then go to the next step. `pause` holds; `next`/`back` jump at once and cut speech.
4. After the last step, the pet says "That's the tour." and VS Code clears highlights.

If VS Code is not connected, the tour still runs: the bubble shows `path:start_line` with each narration.

### 5.8 VS Code extension (`vscode-extension/`)

TypeScript, `engines.vscode` `^1.90.0`, activates on startup (`onStartupFinished`). Packaged with `vsce package` to a `.vsix` the user installs by hand.

- **Connection:** reads the port and token from `%APPDATA%\Devy\config.json`. Connects to `ws://127.0.0.1:<port>/ws?token=...`, reconnects every 3 s. Status bar item: `$(hubot) Devy` (green when connected, grey when not).
- **On connect** it sends `hello {workspace_folders, vscode_version}`; Devy adds the folders as projects.
- **Context:** sends `editor.context {path, selection_text (≤ 4000 chars), start_line, end_line, cursor_line}` on selection change (debounced 300 ms).
- **Commands:** `Devy: Ask about this` (input box, sends `ask {question}` with the context), `Devy: Next step`, `Devy: Previous step`, `Devy: Stop tour`. Keybindings: `Alt+]`, `Alt+[`, `Alt+Escape`.
- **`tour.step` handler** (the "walking through the code" feeling):
  1. Open the file in the main editor group (`preserveFocus: false`, `preview: true`).
  2. Smooth scroll: reveal the line 10 lines above `start_line`, then step the visible range down by 2 lines every 30 ms until `start_line` is near the top third (`revealRange` with `InCenterIfOutsideViewport`).
  3. Highlight the range with a decoration (semi-transparent background in the pet's colour, plus a gutter dot), and move the cursor to `start_line`.
  4. Reply `tour.step.shown`.
- `tour.end` removes the decoration.

### 5.9 WebSocket protocol (`/ws`)

JSON messages, each `{ "type": string, ...fields }`.

Devy → VS Code: `tour.step {tour_id, index, total, path, start_line, end_line, colour}`, `tour.end {tour_id}`.

VS Code → Devy: `hello {...}`, `editor.context {...}`, `ask {question}`, `tour.step.shown {tour_id, index}`, `tour.control {action: next|back|stop|pause|resume}`.

Paths are absolute Windows paths on the wire.

### 5.10 REST endpoints (`/api/v1`)

| Method and path | Purpose |
| :-- | :-- |
| `GET /health` | `{"ok": true, "version": "..."}`, no token |
| `POST /api/v1/hooks/{kind}` | Hook events ([5.4.3](#543-hooks-serverhooksrs)) |
| `POST /api/v1/say` | `{agent_kind?, text}`: for scripts and testing |
| `POST /api/v1/ask` | `{question, project_root?}`: same as asking a pet; returns the answer JSON |
| `GET /api/v1/agents` | Current agents and states, for debugging |

## 6. Config file

`%APPDATA%\Devy\config.json` (the API key is **not** in it):

```json
{
  "port": 4747,
  "token": "hex...",
  "brain": {"model": "muse-spark-1.3"},
  "voice": {"enabled": true, "volume": 0.9, "voices": {"claude-code": "Freya", "gemini-cli": "Easton", "other": "Freya"}},
  "push_to_talk": "Ctrl+Alt+Space",
  "pets": {"claude-code": {"colour": "#E8875B", "x": null, "y": null}, "gemini-cli": {"colour": "#5B8DEF", "x": null, "y": null}},
  "projects": ["C:\\Users\\me\\code\\my-app"]
}
```

## 7. Errors the user must see (spoken or in the bubble)

| Situation | Pet says (bubble, and voice if Phonon is up) |
| :-- | :-- |
| No API key when asked a question | "I need a Meta API key first. I opened settings." (opens settings) |
| API error or timeout (60 s) | "I couldn't reach my brain just now. Try again in a moment." |
| No project known | "Which project? Open it in VS Code and ask again." |
| Phonon failed to start | Bubble only: "Voice is off: Phonon didn't start. See settings." |
| Port in use by another app | Settings shows the error and the port that was used instead |

Log everything to `%APPDATA%\Devy\logs\devy.log` (`tracing` with a rolling file appender).

## 8. Out of scope (for the hackathon)

Window pinning, virtual desktop following, hiding behind windows, roaming, macOS and Linux, an installer with auto-update, multiple monitors with different scaling (assume one scale factor), sprite art, relaying questions to the coding agents, cloud sync, telemetry.

## 9. Phases and acceptance criteria

Each phase must build, run and be committed before the next starts. Track progress in [TASKS.md](TASKS.md).

**P0. Scaffold.** Move the old prototype (`Cargo.toml`, `Cargo.lock`, `src/`) into `prototype/` with `git mv`. Create the Tauri 2 app (Vite, vanilla TS) at the root. Add the local server with `GET /health` and token auth, config file, tray icon with `Settings` and `Quit`, logging.
*Done when:* `npm run tauri dev` opens with a tray icon; `curl http://127.0.0.1:4747/health` answers; a request without the token to `/api/v1/agents` gets 401.

**P1. Overlay and pets.** Full-screen click-through overlay, one test pet, all states, drag, bubble, menu. Cursor watcher with generation counter.
*Done when:* clicks pass through everywhere except the pet; the pet can be dragged; `POST /api/v1/say` shows text in the bubble; closing the overlay never leaves a window that eats clicks.

**P2. Voice out.** Phonon setup flow, sidecar, speech queue with throttle and priorities, mouth animation, mute.
*Done when:* `POST /api/v1/say` is heard within 1 s once Phonon is ready; two quick `say` calls play one after the other, not on top of each other; quitting Devy kills the Phonon process.

**P3. Agent link.** MCP server with the five tools, hook endpoint, agent registry, one pet per agent kind, Connect buttons, instructions snippet.
*Done when:* after `Connect` for Claude Code, a real Claude Code session shows its own orange pet, `devy_status` calls are spoken, a permission prompt makes the pet alert and say so, and `Stop` is announced.

**P4. Brain.** API key in settings, Muse Spark client, repo index, ask flow with tools and JSON answer, `Ask…` text box.
*Done when:* typing "where is the server started?" about this repo gives a correct spoken answer and a tour with real paths and line numbers.

**P5. VS Code extension and tours.** Extension, WebSocket, tour engine, step controls, `devy_tour` tool.
*Done when:* a tour from P4 opens each file in VS Code, scrolls smoothly, highlights the range in the pet's colour in step with the voice, and Next/Back/Stop work from the bubble and from VS Code.

**P6. Voice in.** Push-to-talk, transcription, command matcher with tests, barge-in.
*Done when:* holding the key and saying "where is the login handled?" gets an answer; saying "next" during a tour moves to the next step; `cargo test` passes.

**P7. Demo and submission.** README (install, connect, demo), a demo project (a small web app with a login form) for the video, record the 2-minute demo from [section 2](#2-users-and-the-demo-story), check the hackathon's official rules for the submission form, video length and repo requirements.

**If time runs out**, cut in this order: P6 command words (keep plain questions), smooth scrolling (plain reveal is fine), Gemini Connect button (manual setup in README), activity feed.

## 10. Open questions

Defaults are chosen; change them here if the answer differs.

1. Product name: **Devy** (default).
2. Hackathon submission requirements (video length, public repo, form fields) are not yet checked: do it at the start of P7, or earlier if possible.
3. `muse-voice-transcribe-1.0` endpoint and audio formats: verify in P6.
4. Whether Gemini CLI hooks exist in the installed version: verify in P3.
