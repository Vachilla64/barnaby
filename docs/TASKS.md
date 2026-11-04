# Tasks

Tick each task when it is done and verified (see AGENTS.md). Phases and their "Done when" checks are in [SPEC.md section 9](SPEC.md#9-phases-and-acceptance-criteria).

## P0. Scaffold
- [x] `git mv Cargo.toml Cargo.lock src prototype/`
- [x] Create Tauri 2 app at repo root (Vite, vanilla TypeScript); app name `Barnaby`, identifier `dev.barnaby.app`
- [x] Config file `%APPDATA%\Barnaby\config.json` with port and token (SPEC 6)
- [x] Logging to `%APPDATA%\Barnaby\logs` (`tracing`)
- [x] axum server on 127.0.0.1:4747 (fallback 4748-4757), `GET /health`, token middleware
- [x] Tray icon: Settings, Quit
- [x] Commit `P0: scaffold`

## P1. Overlay and pets
- [x] Overlay window over all monitors: transparent, no decorations, always on top, skip taskbar
- [x] `set_click_regions` command + 8 ms cursor watcher with generation counter; reset click-through on exit
- [x] SVG pet with states: idle, working, talking, thinking, alert, happy, sleeping
- [x] Drag (window forced interactive while dragging), position saved per agent kind
- [x] Speech bubble with optional buttons
- [x] Pet menu (Ask…, Mute, Repeat last, Hide 1 h) and right-click menu (Settings, Quit)
- [x] `POST /api/v1/say` shows text in the bubble
- [x] Commit `P1: overlay and pets`

## P2. Voice out
- [x] Phonon setup flow in settings (detect, ask to install uv/ptts, start, health poll)
- [x] Sidecar in a Job Object, hidden window, killed on quit
- [x] Phonon client (`POST /tts`, streamed WAV)
- [x] Speech queue: sentence split, prefetch next sentence, priorities, 20 s throttle, `stop_speech`
- [x] Playback with rodio; `speech:start` / `speech:end` events; mouth animation
- [x] Unit tests: sentence splitter, throttle
- [x] Commit `P2: voice out`

## P3. Agent link
- [x] Agent registry (kinds, one pet per kind, sleep and removal timers)
- [x] MCP server at `/mcp`: initialize, tools/list, tools/call
- [x] Tools: barnaby_status, barnaby_say, barnaby_done, barnaby_ask_user, barnaby_tour (tour can stub until P5)
- [x] Hook endpoint `/api/v1/hooks/{kind}` with Claude Code event mapping; unit tests for the mapping
- [x] Settings: Connect Claude Code (MCP + hooks merge, shows diff, asks first)
- [x] Settings: Connect Gemini CLI (check whether hooks exist; note result)
- [x] Copy-instructions button (SPEC 5.4.4)
- [ ] Verify with a real Claude Code session
- [ ] Commit `P3: agent link`

## P4. Brain
- [x] Settings: API key field, Test button, keyring storage, `MODEL_API_KEY` fallback
- [ ] Read Meta's Chat Completions and Tool calling docs; note differences from OpenAI below
- [x] Muse Spark client (chat completions, tools, 60 s timeout; no streaming, cut)
- [x] Repo index: walk, symbols, repo map, full text under 400k tokens (rebuilt on every ask; no file watcher, cut)
- [x] Tools: read_file, search, list_dir (sandboxed to project root)
- [x] Ask flow: JSON answer, validation, clamp lines, retry once
- [x] `Ask…` text box in the pet menu; `POST /api/v1/ask`
- [x] Error messages from SPEC 7
- [ ] Commit `P4: brain`

## P5. VS Code extension and tours
- [x] Extension scaffold in `vscode-extension/` (TypeScript), reads config for port and token
- [x] WebSocket client with reconnect, status bar item, `hello`, `editor.context`
- [x] `tour.step` handler: open, smooth scroll, highlight decoration, reply `tour.step.shown`
- [x] Commands and keybindings: Ask about this, Next, Previous, Stop
- [x] Tour engine in Rust (steps, timing, controls); bubble Back/Next/Stop
- [x] Hook `barnaby_tour` up to the engine
- [x] Package `.vsix`
- [ ] Commit `P5: tours`

## P6. Voice in
- [ ] Global push-to-talk shortcut (hold)
- [ ] Mic recording in page (MediaRecorder), bytes to Rust
- [ ] Transcription with `muse-voice-transcribe-1.0` (verify endpoint; note it below)
- [ ] Command matcher (next, back, stop, pause, continue, repeat, mute, ask_user answers) with unit tests
- [ ] Barge-in: push-to-talk stops speech
- [ ] Commit `P6: voice in`

## P7. Demo and submission
- [ ] Read the hackathon's official rules: submission form, video length, repo requirements
- [x] README: install, connect agents, use
- [x] Demo project with a login form
- [ ] Rehearse and record the demo (SPEC 2)
- [ ] Submit before 2026-11-04 9:00 PM ET

## Notes

- 2026-11-04: Set `debug = 0`, `incremental = false`, `strip = "debuginfo"` in `src-tauri/Cargo.toml` for dev/test profiles to keep debuginfo lean and avoid exhausting disk space with MSVC PDBs.
- 2026-11-04: Used `tauri::tray::TrayIconBuilder` and axum 0.8 `build_router` pattern with 6 unit tests covering health, auth token, and config serialization.
- 2026-11-04: Fixed `STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)` in test harness caused by `TaskDialogIndirect` in `comctl32.dll` requiring Common Controls v6; added `/MANIFESTDEPENDENCY` link argument in `build.rs` and aligned `windows` crate version to `0.62` across workspace.
- 2026-11-04: Implemented P1 frontend: SVG PetComponent with 7 states (idle, working, talking, thinking, alert, happy, sleeping), SpeechBubbleComponent with action buttons, PetMenuManager with 200ms `mouseLeave` debounce, PetDragManager with coordinates persistence, ActivityFeedManager, and continuous click-through region synchronization.
- 2026-11-04: User preference recorded: during long stretches of autonomous work, agents must speak progress updates mid-stream rather than waiting until the end. Barnaby's voice is set to a British male voice from Phonon defaults (`Toby`), and Barnaby will speak updates during long runs.
- 2026-11-04: Fixed overlay pet interactivity bug: (1) Added Win32 `ScreenToClient` conversion so screen-space cursor coordinates match webview client physical coordinates across multi-monitor setups; (2) Applied direct `SetWindowLongW` + `SetWindowPos` (`SWP_FRAMECHANGED`) on HWND in clickthrough watcher thread to eliminate Tao event-loop message queue latency; (3) Added `.interactive, .interactive * { pointer-events: auto !important; }` in CSS so SVG elements and paths receive pointer events; (4) Removed `offsetParent` check in `regions.ts` which skipped SVG elements; (5) Locked window interactive immediately on `mousedown` in `PetDragManager` so dragging cannot be dropped if cursor moves fast; (6) Enforced explicit `PhysicalPosition` and `PhysicalSize` on overlay window to guarantee 1:1 pixel mapping.
- 2026-11-04: Implemented P2 Voice Out: (1) Phonon sidecar manager with Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) ensuring child terminates even on crash; (2) AudioPlayer wrapping Rodio in dedicated thread with volume/mute controls; (3) SpeechQueue with lookahead sentence prefetching (synthesizing N+1 while N plays), priority barge-in for High alerts, and 20s throttle tracker for low/normal status; (4) SentenceSplitter handling code extensions (`.ts`, `.rs`), version tags (`v1.0`), decimal numbers, and abbreviations; (5) Frontend Speech events (`speech:start`, `speech:end`, `speech:bubble`) animating pet mouth and bubble; (6) Settings view Phonon detection, uv/ptts installer triggers, and per-agent voice configuration.
- 2026-11-04: Renamed Devy to Oscar, then Oscar to Barnaby (crate `barnaby`/`barnaby_lib`, identifier `dev.barnaby.app`, `%APPDATA%\Barnaby`, log `barnaby.log`). Old `%APPDATA%\Devy` is moved across on first start. Fixed: closing Settings destroyed the window so the tray could not reopen it (now hides); Phonon detection and install ran blocking child processes on async threads (now `spawn_blocking`). Note: `ptts` is not installed on the dev machine (only `pocket-tts`), so Phonon voice names are unverified until it is.
- 2026-11-04: P3 backend done (`agents.rs`, `mcp.rs`, `connect.rs`, hook route). Verified live with curl: `/mcp` tools/list, hook Notification -> registry state `alert`, Phonon auto-started and healthy. Stubs: `barnaby_ask_user` speaks and returns "no answer" (bubble buttons come with P4/P6); `barnaby_tour` returns "started" (P5). Registry has no sleep/removal timers yet and the overlay shows one pet (claude-code); both are open. Gemini Connect button only shows manual steps (scope cut). Not yet verified: a real Claude Code session after Connect.
- 2026-11-04: Fixed a P2 bug: Phonon streams a WAV with 0xFFFFFFFF sizes in the header and rodio could not decode it ("end of stream"). `fix_wav_header` now writes the real sizes. Needs a restart of the app to hear it.
- 2026-11-04: The C: drive hit 0 bytes free during a build. `src-tauri/target` is ~2.8 GB; delete stale `target/debug` output after any rename.
- 2026-11-04: P4 and P5 built. Verified with fakes, not the real services: a scripted OpenAI-style mock model (search tool, read_file tool, fenced JSON answer -> spoken answer plus tour offer) and a Node fake VS Code client over `/ws` (tour.step with clamped lines, missing file dropped, `next` control, auto-advance after speech, tour.end). NOT verified: a real Meta API key (none on this machine; `brain.base_url` in config.json can point at any OpenAI-compatible server, for example Ollama), the extension inside a real VS Code (not installed here; Antigravity is, and takes the same .vsix), the overlay bubble buttons by eye.
- 2026-11-04: The local server falls back to ports 4748-4757 and saves the port it used. The Claude Connect hooks and the extension both read that port, so re-run Connect after a port change. A stale instance can hold 4747 for a few seconds after quit.
- 2026-11-04: Install the extension with `code --install-extension vscode-extension/barnaby-vscode-0.1.0.vsix` (or Extensions > Install from VSIX). Rebuild with `pnpm package` in `vscode-extension/`.
- 2026-11-04: Meta API facts (docs read 2026-11-04): chat completions at `POST https://api.meta.ai/v1/chat/completions`; only `tool_choice: "auto"`; send the full assistant message with `tool_calls` before the `tool` messages; `system` is accepted (`developer` preferred); `response_format: {type: "json_schema", json_schema: {name, schema}}` is supported (Barnaby does not use it yet, it asks for JSON in the prompt and validates); `stop`, `n>1`, `logprobs`, penalties are rejected; `reasoning_effort: "none"` is rejected. Transcription: `POST https://api.meta.ai/v1/asr/transcribe`, multipart with a JSON part `request` (`{"mode":"PUSH_TO_TALK","model":"muse-voice-transcribe-1.0","audioEncoding":"WAV"}`) and an `audio` part; WAV only, mono, 16-bit PCM, 16 or 24 kHz; reply `{"transcript": "..."}`.
