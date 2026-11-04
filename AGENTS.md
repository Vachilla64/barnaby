# Instructions for coding agents

You are building **Barnaby**, a Windows desktop pet app that speaks for coding agents and walks the user through their own codebase. The full product spec is [docs/SPEC.md](docs/SPEC.md). Read it completely before writing code. Progress lives in [docs/TASKS.md](docs/TASKS.md).

## How to work

1. **Work phase by phase** (P0 to P7 in SPEC section 9). Do not start a phase before the previous one meets its "Done when" line.
2. **At the start of a session**, read `docs/TASKS.md` and continue from the first unchecked task.
3. **After each task**, tick it in `docs/TASKS.md`. Add a short note under "Notes" if you learned something the next agent needs (an API detail, a workaround, a decision).
4. **Commit after each phase** (and after any large task) with a message like `P2: speech queue with throttle`. Push to `origin main`.
5. **Type-check often:** `cargo check` in `src-tauri`, `npx tsc --noEmit` in the frontend and in `vscode-extension`.
6. **The running exe is locked.** Close Barnaby before a full rebuild, and rebuild before asking the user to test. Never report a feature as done after only compiling.
7. **Test what is pure** (command matcher, sentence splitter, throttle, hook mapping, tour validation) with `cargo test`. Test the desktop behaviour by hand, with the user driving. **Never move the user's mouse or type into their windows** to test.
8. **If the spec is unclear or wrong**, pick the simplest option that serves the demo story (SPEC section 2), write the decision under "Notes" in TASKS.md, and keep going. Ask the user only when the choice is expensive to undo.
9. **Time is short (deadline 2am).** Tick TASKS.md in batches, not per task. Commit per phase. Hand-test once per phase, not per task. No spoken progress updates while building.
10. Barnaby's voice is a British male voice from Phonon's catalog (`Toby` by default; `Master Aldric` and `Sir Bramwell` suit the old-gentleman character). The product speaks for agents; the build process does not need to.

## Rules

- Windows only. Rust stable, Tauri 2, Node 20+, TypeScript strict mode.
- Keep native code small: plain FFI through the `windows` crate for window APIs, one module per concern.
- **No dead code.** If something cannot be made to work, remove it and note it in TASKS.md.
- `prototype/` is reference only. Port ideas, not code. Do not build or edit it.
- Bind servers to `127.0.0.1` only. Every endpoint except `/health` checks the token.
- Never log or commit the Meta API key or the local token. The key lives in Windows Credential Manager.
- Never send window titles or anything outside the chosen project folder to the Meta API.
- Do not install software on the user's machine without asking (the Phonon setup asks through the settings window).
- Ask before changing the user's `~/.claude` or `~/.gemini` settings; merge, never overwrite.
- Read [docs/PLAYBOOK.md](docs/PLAYBOOK.md) before P1: it records the lessons from a previous desktop pet (click-through races, locked binaries, and so on).

## Useful facts

- Phonon TTS is installed on the dev machine as a uv tool (`ptts`). The server script is `resources/phonon/phonon_server.py`. Run it by hand with `uv tool run --from ptts python resources/phonon/phonon_server.py`. It listens on `127.0.0.1:8001`; first start takes about 2 minutes. Barnaby's default voice is a British male voice from Phonon's default catalog (`Toby`).
- Meta Model API: base `https://api.meta.ai/v1`, `Authorization: Bearer $MODEL_API_KEY`, model `muse-spark-1.3`, OpenAI-compatible Chat Completions. Docs: https://dev.meta.ai/docs/
- Hackathon deadline: **2026-11-04, 9:00 PM ET**. Scope cuts are listed at the end of SPEC section 9.
