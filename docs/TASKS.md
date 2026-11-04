# Tasks

Tick each task when it is done and verified (see AGENTS.md). Phases and their "Done when" checks are in [SPEC.md section 9](SPEC.md#9-phases-and-acceptance-criteria).

## P0. Scaffold
- [x] `git mv Cargo.toml Cargo.lock src prototype/`
- [x] Create Tauri 2 app at repo root (Vite, vanilla TypeScript); app name `Devy`, identifier `dev.devy.app`
- [x] Config file `%APPDATA%\Devy\config.json` with port and token (SPEC 6)
- [x] Logging to `%APPDATA%\Devy\logs` (`tracing`)
- [x] axum server on 127.0.0.1:4747 (fallback 4748-4757), `GET /health`, token middleware
- [x] Tray icon: Settings, Quit
- [x] Commit `P0: scaffold`

## P1. Overlay and pets
- [ ] Overlay window over all monitors: transparent, no decorations, always on top, skip taskbar
- [ ] `set_click_regions` command + 8 ms cursor watcher with generation counter; reset click-through on exit
- [ ] SVG pet with states: idle, working, talking, thinking, alert, happy, sleeping
- [ ] Drag (window forced interactive while dragging), position saved per agent kind
- [ ] Speech bubble with optional buttons
- [ ] Pet menu (Ask…, Mute, Repeat last, Hide 1 h) and right-click menu (Settings, Quit)
- [ ] `POST /api/v1/say` shows text in the bubble
- [ ] Commit `P1: overlay and pets`

## P2. Voice out
- [ ] Phonon setup flow in settings (detect, ask to install uv/ptts, start, health poll)
- [ ] Sidecar in a Job Object, hidden window, killed on quit
- [ ] Phonon client (`POST /tts`, streamed WAV)
- [ ] Speech queue: sentence split, prefetch next sentence, priorities, 20 s throttle, `stop_speech`
- [ ] Playback with rodio; `speech:start` / `speech:end` events; mouth animation
- [ ] Unit tests: sentence splitter, throttle
- [ ] Commit `P2: voice out`

## P3. Agent link
- [ ] Agent registry (kinds, one pet per kind, sleep and removal timers)
- [ ] MCP server at `/mcp`: initialize, tools/list, tools/call
- [ ] Tools: devy_status, devy_say, devy_done, devy_ask_user, devy_tour (tour can stub until P5)
- [ ] Hook endpoint `/api/v1/hooks/{kind}` with Claude Code event mapping; unit tests for the mapping
- [ ] Settings: Connect Claude Code (MCP + hooks merge, shows diff, asks first)
- [ ] Settings: Connect Gemini CLI (check whether hooks exist; note result)
- [ ] Copy-instructions button (SPEC 5.4.4)
- [ ] Verify with a real Claude Code session
- [ ] Commit `P3: agent link`

## P4. Brain
- [ ] Settings: API key field, Test button, keyring storage, `MODEL_API_KEY` fallback
- [ ] Read Meta's Chat Completions and Tool calling docs; note differences from OpenAI below
- [ ] Muse Spark client (chat completions, tools, streaming, 60 s timeout)
- [ ] Repo index: walk, symbols, repo map, full text under 400k tokens, file watcher
- [ ] Tools: read_file, search, list_dir (sandboxed to project root)
- [ ] Ask flow: JSON answer, validation, clamp lines, retry once
- [ ] `Ask…` text box in the pet menu; `POST /api/v1/ask`
- [ ] Error messages from SPEC 7
- [ ] Commit `P4: brain`

## P5. VS Code extension and tours
- [ ] Extension scaffold in `vscode-extension/` (TypeScript), reads config for port and token
- [ ] WebSocket client with reconnect, status bar item, `hello`, `editor.context`
- [ ] `tour.step` handler: open, smooth scroll, highlight decoration, reply `tour.step.shown`
- [ ] Commands and keybindings: Ask about this, Next, Previous, Stop
- [ ] Tour engine in Rust (steps, timing, controls); bubble Back/Next/Stop
- [ ] Hook `devy_tour` up to the engine
- [ ] Package `.vsix`
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
- [ ] README: install, connect agents, use
- [ ] Demo project with a login form
- [ ] Rehearse and record the demo (SPEC 2)
- [ ] Submit before 2026-11-04 9:00 PM ET

## Notes

- 2026-11-04: Set `debug = 1` in `src-tauri/Cargo.toml` for dev/test profiles to keep debuginfo lean and avoid exhausting disk space with MSVC PDBs.
- 2026-11-04: Used `tauri::tray::TrayIconBuilder` and axum 0.8 `build_router` pattern with 5 unit tests covering health, auth token, and config serialization.

