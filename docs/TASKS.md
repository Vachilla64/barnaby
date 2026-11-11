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
- [x] Global push-to-talk shortcut (hold)
- [x] Mic recording in page (MediaRecorder), bytes to Rust
- [x] Transcription with `muse-voice-transcribe-1.0` (verify endpoint; note it below)
- [x] Command matcher (next, back, stop, pause, continue, repeat, mute, ask_user answers) with unit tests
- [x] Barge-in: push-to-talk stops speech
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
- 2026-11-04: INCIDENT: a dev run with the Vite server down made the overlay show a full-screen browser error page over the user's desktop. Fix: the overlay window now starts hidden and is shown only when the page calls `overlay_ready`; if that does not happen within 10 s the backend closes it. Also fixed `capabilities/default.json`, which listed a window `main` that is closed at startup instead of `overlay` and `settings`. Rule for agents: do not launch the overlay with `cargo run --no-default-features` unless the Vite server is up; prefer `pnpm tauri dev`, and tell the user before launching a visible window.

## Handoff (2026-11-04, ~23:55 local; deadline 02:00 local = 9 PM ET)

State: P0 to P5 and P7 are built and committed (up to 5ca7f5a). P6 voice-in is in progress by ANOTHER session in the same checkout (uncommitted: `src-tauri/src/voice_in.rs`, `src/ptt.ts`, `src/wav.ts`, edits to `lib.rs`, `main.ts`, `Cargo.toml`). Pet art is in `character-design/` by yet another agent (README there says it is isolated from `src/`). Do not edit those files from a second session.

Uncommitted by the P3-P5 session (safe to commit): `agents.rs` (sleep after 10 min, remove after 60, with a unit test), `server.rs` (30 s sweep task), `voice/queue.rs` (`stop_speech` no longer clears `is_playing`, which allowed two pipelines), `brain/mod.rs` (the other session split `say_error` into `speak_error` and `open_settings_window`).

Run it safely: `pnpm build`, then in `src-tauri` run `cargo build --features tauri/custom-protocol` and start `target/debug/barnaby.exe`. Never run `cargo run --no-default-features` without a live Vite server: the overlay then shows a browser error page over the whole screen (this happened once). The overlay is now hidden until the page calls `overlay_ready`, with a 10 s watchdog.

Inspect the UI without touching the mouse: start the exe with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`, then drive it over CDP (screenshot, evaluate JS, read console).

Open items, most valuable first:
- [ ] Real Meta API key test (none on this machine). Settings > Brain > Test, then ask a question about `demo-project/`.
- [ ] Real VS Code or Antigravity test of the `.vsix` (`vscode-extension/barnaby-vscode-0.1.0.vsix`); only a fake client was used.
- [ ] Real Claude Code MCP call (CLI here is signed out). Hooks were verified live: SessionEnd moved the pet to `sleeping`.
- [ ] P6 voice in (other session). Meta transcription: `POST https://api.meta.ai/v1/asr/transcribe`, multipart `request` JSON + `audio` WAV mono 16-bit 16/24 kHz.
- [ ] One pet per agent kind in the overlay (backend registry is ready; overlay shows one pet).
- [ ] Gemini Connect button (README has the manual steps; Gemini MCP URL needs `?kind=gemini-cli`).
- [ ] Demo recording and submission form (user).
- Known flaws: a port fallback is saved to config.json and never returns to 4747; `barnaby_ask_user` returns "no answer" (no bubble buttons yet).
- 2026-11-04: P6 built. Verified without the app: command matcher unit tests (questions are not claimed), a Rust test that posts the multipart request to a local fake `/v1/asr/transcribe`, and a Node check of the WAV encoder (16 kHz mono 16-bit, correct header). NOT verified: the global shortcut firing, the WebView2 microphone permission prompt, and real transcription (no Meta key here). Recording uses AudioContext + ScriptProcessor, not MediaRecorder, so no conversion is needed. `ask_user` spoken answers are not wired (the tool returns "no answer"). First use will show the Windows/WebView2 microphone permission; if it never appears, grant microphone access to Barnaby in Windows privacy settings.
- 2026-11-04: Visual character design and animatable SVG vectorization for Barnaby active in `character-design/`. Work is strictly isolated from `src/` and `src-tauri/`.

### Update (2026-11-05, pet owner = the P3-P5 session)
- Real Meta brain verified: key saved to Windows Credential Manager via the app's `save_api_key` (also settable in Settings > Brain). Settings Test returned OK. `POST /api/v1/ask` on `demo-project/` returned a spoken answer and a 6-step tour in about 19 s. `brain.base_url` in config.json defaults to Meta; `projects` in config.json holds the demo project.
- Fixed: models prefix tour paths with the project folder name; `resolve_path` now accepts that. `reasoning_effort: "low"` is sent to Meta to cut latency.
- `.vsix` install is manual: VS Code or Antigravity > Extensions > ... menu > Install from VSIX > `vscode-extension/barnaby-vscode-0.1.0.vsix`. Antigravity has no CLI installer.
- NEXT (not started on disk): replace the blob pet in `src/pet.ts` with pixel art of the reference in `character-design/reference.png` (old gentleman, top hat, grey moustache and beard, pinstripe waistcoat, bow tie, glowing cyan monocle). Plan: ASCII-grid generator in `tools/` that writes `<rect>` pixels (32 x 44 grid, scale 3, `shape-rendering: crispEdges`); keep the existing CSS class hooks (`pet-body`, `eye-open`, `eye-closed`, `mouth-idle`, `mouth-open`, `eye-pupil`, `gear-badge`, `alert-badge`, `thinking-dots`, `sleep-container`) so the 7 states keep working; add `monocle` group with a cyan glow; hat tip on talking. Check by CDP screenshot only.
- VS Code integration ideas, cheapest first: highlight a whole function by name (document symbol provider), inline note text after the highlighted line, a fake cursor drawn as a moving decoration, peek definition and diffs. Not possible through the API: a real mouse pointer or drawing over the editor.
- 2026-11-05: Pixel pet built. `tools/build-pixel-pet.py` writes `src/pet-art.ts` (32 x 44 grid of `<rect>` pixels, scale 3, crisp edges); `src/pet.ts` uses it; `src/styles.css` has the new sizes, badge positions, monocle glow and hat nod. Checked by injecting it into the running overlay and screenshotting all 7 states (idle, working, talking, thinking, alert, happy, sleeping). To change the art, edit the grids in the script and run `python tools/build-pixel-pet.py --preview out.png`. The running app still shows the old pet until rebuilt: `pnpm build`, then `cargo build --features tauri/custom-protocol` in `src-tauri`, then restart `target/debug/barnaby.exe` (quit the old one from the tray first). Open: left-facing/walking frames, hands, and the pet colour setting is unused.
- 2026-11-05 (later): Pet physics and poses. `src/physics.ts` gives him gravity: after a drag he falls to the floor (top of the taskbar, from `screen.availHeight`), bounces twice at most, squashes and slides; a mouse flick throws him (speed of the last 100 ms, capped). Catching him in mid-air works. CSS classes on the pet: `grabbed` (held: wide eyes, round mouth, flailing arms, kicking legs, swinging), `falling` (same, no swing), `landed` (squash, eyes shut, hat pop). New pet state `guiding`: set on `tour:step` and while a tour speaks; he turns his back (the `view-back` group in the SVG) and one arm cycles point, present, rest every 3 s. He is mirrored when standing in the left half of the screen so the arm points toward the middle. The art for all of this is in `tools/build-pixel-pet.py` (front arms and legs are separate groups; back view is `BACK_ROWS` plus `ARM_*` layers). The `guiding` pose is not yet used for "telling you what to do" outside tours; wire it to `speech:start` for instruction-type messages if wanted.
- 2026-11-05: Settings had two Meta key cards; the top one had no handlers, so its Test button did nothing. Removed it; Test now saves a pasted key first and shows errors in red. Voice now streams: `PhononManager::synthesize_stream` hands PCM chunks to `StreamSource` in `playback.rs`, which starts after ~150 ms of audio (Phonon makes audio about 7x faster than real time). `fix_wav_header` is gone; the streamed header is parsed instead. Default voice is `Sir Bramwell` (`config::DEFAULT_VOICE`); Barnaby had no entry in `voices`, so he was falling back to Toby. He now calls himself Sir Barnaby. The click menu is a half-circle radial menu above the pet (`menu.ts`); right-click opens the same menu. Note: port 8001 was served by another project's Phonon server (Bella), which Barnaby reuses if it answers `/health`.
- 2026-11-05 (capabilities round): (1) Chat list: `src/chat.ts` stands beside him (left of him when he is in the right half), bottom on his feet, growing toward the top of the screen. The backend echoes every question as `chat:user` (typed, spoken, from a button, from VS Code), then `chat:answer` / `chat:error`; his own answers are no longer shown a sentence at a time in the bubble (the bubble is for status, alerts and tours). (2) Conversation memory: `brain::Conversation` keeps the last 8 question/answer turns for the current project (forgets on a project change or "new chat"); the project goes in the system message, history after it. (3) Selection: `brain::editor_note` tells the model which lines are selected and that "this/it/here" mean them; a file outside the project folder is never described. (4) Look mode: click the monocle (`LookController.isOnMonocle`, hit box = monocle rect + 12 px), `look.rs` waits for a VS Code selection to stay put for 500 ms (the extension adds 300 ms), the cursor watcher sends `look:cursor` so the page draws a lens on the mouse. One look per click; ends on selection, second click, Alt+Escape (existing Stop), "stop" by voice, 90 s timeout, or VS Code disconnecting. `look.after_select` in config: `choices` (default), `explain`, `listen` (hands-free recording with silence detection in `ptt.ts`). (5) First-run guide: `src/setup.ts` card in the chat, polls `setup_status`; needs key + VS Code, voice and talk key are optional. `install_vscode_extension` runs `code --install-extension` on the `.vsix` found next to the exe or in `vscode-extension/` (a packaged installer would need to bundle it as a resource). (6) Push-to-talk `Ctrl+Alt+Space` was never registered on this machine ("HotKey already registered": on Windows that is what a refused `RegisterHotKey` is called; probably the Claude desktop app's quick-entry key). Settings and the setup card can now pick another key (`set_push_to_talk`), and the chat box has a microphone button that needs no global key.

- 2026-11-05 (extension link): the real extension in the user's VS Code activated (exthost.log shows `onStartupFinished`) but never reached the app, and it failed silently (`ws.on("error")` was empty). Run outside VS Code with a stubbed `vscode` module, the same bundle connected fine, so the cause is in the VS Code environment. 0.1.2 writes an output channel (`Barnaby: Show log`) and puts the reason in the status bar tooltip; the setup guide now says "installed but not talking" separately from "not installed". A new install only takes effect after Reload Window. ROOT CAUSE (found from the 0.1.2 log: `ENOENT ... AppData\Roaming\Barnaby\config.json`): the Barnaby that the agent launched ran under the Claude desktop app's MSIX file virtualization, so its config was in `%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming\Barnaby` and invisible to VS Code. Run Barnaby from the user's own terminal to test the link (see AGENTS.md). The `.vsix` files are build output and not tracked in git.

- 2026-11-05 (audio device, second fix): following the default device by NAME was not enough. Plugging in headphones can leave the device name unchanged (a headphone jack, or a virtual device such as FxSound) while the cpal stream behind it dies; the sink then never drains and the speech queue hangs. `voice/playback.rs` now keeps each sentence's PCM in `Shared` and runs a watchdog: `Shared::pulls` counts every request the output makes, and if it freezes for 0.7 s (3 s for a freshly opened output) the worker closes the output, opens the current default device and carries on from `Shared::handed` minus 200 ms. A change of default device name is handled the same way, mid-sentence. Pure parts (source, resume position, watchdog) have unit tests; the real unplug has not been tested by an agent (no way to unplug from here). If it still goes silent, the log says "Audio output stopped responding" or "changed".

- 2026-11-05 (teaching round, tours): tours were too much at once (up to 12 steps, line numbers read aloud, auto-advancing, the same words spoken and shown). Rebuilt on what is known about teaching: (1) overview first: `speech` is the answer plus the map ("I will show you three things: A, B, C"), and the chat shows the same stops as a numbered list under it (`chat:answer.agenda`); the tour starts only after that, on `Show me`. (2) few chunks: prompt asks for 3 to 4 steps, never more than 5; `tours::MAX_STEPS` is 6; each step has a plain 2 to 5 word `title` and a narration of at most about 30 words. (3) plain English: the system prompt in `brain/mod.rs` has a PLAIN ENGLISH section (short sentences, purpose before mechanism, jargon explained when used, no code, symbols or line numbers read aloud). (4) learner-paced: `config.tour.pace` is `wait` (default) or `auto`; in `wait` he explains a step, then stops and waits for Next / "next" / "go on"; the Next button pulses (`tour:waiting`). Auto mode waits 2.5 s between steps (was 1.2). (5) no duplicate channels: while VS Code follows along the highlight shows where to look, so he no longer says "look at lines 12 to 20"; the narration is not written out on screen either (a "The words" fold on the tour card, open only when his voice is off). (6) the tour is one card in the chat (`src/tour.ts`, events `tour:start` / `tour:step` / `tour:waiting` / `tour:end`); while it runs the chat list shrinks to that card (`.is-touring`) so less code is covered; the speech bubble is no longer used for tours. (7) recap: `recap` in the reply is one sentence he says after the last step ("The thing to remember: ...") and the chat shows it. (8) the stops he offered are remembered in the conversation (`Turn.tour`), so "tell me more about the second one" works. Agents' tours (`barnaby_tour`) get the same card and pacing; `title` and `recap` are optional there. Research notes: advance organizer / pre-training, segmenting (learner-paced chunks), signaling, the redundancy effect (narration plus identical on-screen text), and top-down comprehension for newcomers all point the same way; sources are in the session that made this change. Checked with a real Meta ask (3 to 4 stops, titles, plain narration, recap); the card, Next / Back / Stop, the recap and a one-minute hold at a step (panel stays open, hint shows) checked in the running app with a fake VS Code and CDP, not yet by the user with real VS Code. One unexplained reading: once the chat panel's class list showed `hidden` six seconds into a tour started from the "Show me" chip (not reproduced in three later runs); `tour:waiting` now re-opens the panel as a safety net. Voice with the real Phonon: each step is spoken, then the Next button pulses; with voice off the words fold opens by itself.
- 2026-11-11 (extension & avatar update, link verification): (1) Removed the brass border from the profile picture/extension icon and made the background 100% transparent with just Sir Barnaby's face/top hat/monocle/beard (`tools/build-extension-icon.py`). (2) Created codebase-helper icon variations in `tools/generate-icon-variations.py` and rendered an interactive carousel in `profile_image_variations.md` (code brackets `< / >`, monocle code inspection beam, mini IDE editor card, and illustrated Victorian gentleman coder studio portrait). (3) Bumped extension to version `0.1.3`, added `onDidChangeWorkspaceFolders` dynamic refresh, `file://` scheme filter, and APPDATA/LOCALAPPDATA fallback path resolution in `vscode-extension/src/extension.ts`. (4) Packaged `barnaby-vscode-0.1.3.vsix` and installed it into VS Code (`code --install-extension ... --force`). (5) Verified live link over WebSocket (`/health` OK, `ws` connected to port 4747 with `hello` handshake, and 63 unit tests passed).

## Backlog (ideas, not started)
- Idle animations: after a few quiet minutes he pulls out a chair and sits down; stands up when clicked or when an agent speaks.
- Narrate progress while he thinks ("searching for login... reading the validate file"); the tool loop in `brain::ask` knows each step.
- "Take me to X": jump to one place without a full tour. "Where is this used": ask VS Code's reference provider (`vscode.executeReferenceProvider`) instead of the model searching.
- Mark the selected lines in VS Code in his colour while he talks about them.
- A VS Code panel with the whole conversation, for long answers.
- Window title / project switching: pick the project from the active VS Code window when several are open.
