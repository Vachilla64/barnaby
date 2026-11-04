# Desktop pet playbook: how it was built, what was skipped

A generic recap of how Bella's pet mode was upgraded, written so it can seed a new project. Nothing here is specific to Bella except the examples.

## The shape of the solution

1. **One full-screen, transparent, click-through window** (Tauri 2) instead of a small window that moves and resizes. The character is just an element drawn inside it. The OS window never moves.
2. **Click-through on demand.** The window ignores the mouse by default. The page reports the rectangles that should take clicks (the character, open menus, speech bubble). A native thread polls the cursor every 8 ms and switches click-through off only while the cursor is inside one of those rectangles.
3. **Pinned, not perched.** The character belongs to a window. Her place is stored as a fraction of that window's size, so she moves and resizes with it. Rust reports the window's rectangle about 100 times a second.
4. **Virtual desktops.** The overlay lives on one desktop. When the user speaks, the app moves the overlay to the user's current desktop and she slides in from the edge matching the swipe direction.
5. **Agency.** Spoken commands become desktop actions: go to a window, come to the cursor, bring a window forward.

## How each piece was made to work

| Feature | Technique | Lesson |
| :-- | :-- | :-- |
| Full-screen overlay | Size the window to the union of all monitors, transparent, no decorations, always on top | Do not resize or move it from the page; let the page place things with CSS variables |
| Click-through | `set_ignore_cursor_events` toggled by a Rust watcher thread comparing cursor to page-reported regions | Start the watcher once per entry (use a generation counter), and always clear click-through when leaving the mode, or the normal window goes dead to clicks |
| Dragging | While dragging, force the window interactive | The cursor can outrun the character; otherwise the drag drops |
| Window list | `EnumWindows` plus `DwmGetWindowAttribute` (extended frame bounds, cloaked) through plain FFI, no extra crates | Skip tool windows, untitled windows, the taskbar and cloaked windows (those are on other virtual desktops) |
| Pinning | Pass the pinned window handle to the watcher; it emits `{state, rect}` only on change | States: ok, minimized, hidden, gone. Gone means she stays put, free |
| Hidden when covered | Check the z-order every 200 ms; hide unless she is speaking | An announcement must still be visible |
| Virtual desktops | COM `IVirtualDesktopManager` (documented) plus two registry values for desktop order | Order from the registry tells you which edge to peek in from |
| Bring window forward | `AttachThreadInput` to the foreground thread, then `SetForegroundWindow` | A plain `SetForegroundWindow` is refused; do not fake an Alt key press (it opens menus) |
| Voice commands | Page sends window titles to the backend; backend matches the sentence against them and only claims it on a real match | Never claim a sentence you cannot act on, or you steal it from the model and other features |
| Privacy | Titles stay on the computer | Never send them to a cloud model |

## Process that worked

- **Phases, each shippable:** spike, remove the old system, scanner and pinning, desktop follow, agency. Commit after each.
- **Port ideas, not code.** The reference repo was studied for techniques; the implementation was rewritten to fit the existing app.
- **Dead code is a no-go.** If a feature could not be made to work, it was removed, not left behind.
- **Type-check often** (`cargo check`), but remember the running exe is locked: close the app before a full rebuild, and rebuild before asking the user to test. (An early "I don't see any of these" was just an old binary.)
- **Generated import blocks** in the UI scripts needed a link-updater run after any shared name changed.

## Mistakes worth not repeating

- Driving the user's mouse during testing while they were using the PC. Ask, or let them test.
- Reporting a feature as done after compiling but before rebuilding.
- A race in the click-through watcher left the normal window ignoring clicks. Fix: generation counter and an explicit reset on exit.
- A power outage left some files zero-filled. Scan tracked files for NUL bytes after a crash.
- Shared files in a repo with several agents: stage only your own hunks (build the index version from HEAD plus your edits).

## Skipped or not done

- **Commenting when the user switches windows** (optional idea from the plan).
- **Sleeping when idle** as an agency behaviour.
- **Phase 5, mood behaviours** tied to her states.
- **Different display scaling per monitor.** The page assumes one device pixel ratio.
- **Non-rectangular monitor layouts** leave dead zones she can be dragged into.
- **Live check of "show me" on every app.** Windows can still refuse to raise some windows (elevated apps, for example).
- **The 3D tour track.** Deliberately out of scope.
- **Roaming is off by default** (built, but the user turns it on).

## Starting a new project from this

1. Decide the character is a floating element in one transparent overlay; draw everything with CSS variables.
2. Build the click-region report plus cursor watcher first. Everything else depends on it.
3. Add the window scanner and pinning next, then desktops, then voice.
4. Keep the native code small: plain FFI for window APIs, one module per concern.
5. Write tests for anything pure (command matching). Test native behaviour by hand, on the real desktop, with the user driving.
