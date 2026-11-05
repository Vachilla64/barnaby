# Demo video script (3 minutes maximum)

The form wants a YouTube link to a video of 3 minutes or less. Aim for 2:45.
Record the screen first, then the voice-over, so a stumble does not cost the take.

## Before you press record

1. Plug the laptop in. Close other apps. Turn on Do Not Disturb.
2. Open `C:\Users\USER\Dev\Oscar\demo-project` in VS Code. Reload the window so the 0.1.1 extension (Barnaby's face icon) loads. The status bar shows "Barnaby" when it is linked.
3. Start Barnaby: `src-tauri\target\debug\barnaby.exe`. Wait for "Ah, that's better. My voice is ready." Barnaby says good evening.
4. Test once with a throwaway question so the first real take is not the slow first call.
5. Record with OBS (add "Desktop Audio" so Barnaby's voice is on the video) or Windows Game Bar (Win+Alt+R).

## Shot list

| Time | On screen | What you say (plain words) |
|---|---|---|
| 0:00 to 0:15 | Desktop. Barnaby stands at the bottom. | "Opening code you did not write is hard. Meet Barnaby. He lives on my desktop and he reads code out loud." |
| 0:15 to 0:35 | Drag Barnaby up with the mouse, let go. He panics, falls, bounces. | "He is a pet, so he reacts. Pick him up, he panics. Let go, he falls." |
| 0:35 to 1:15 | VS Code with the demo project. Click Barnaby, Ask: "How does login work? Show me." Thinking dots, then he speaks. | "I ask a question about my project. Muse Spark reads the files itself and answers out loud." |
| 1:15 to 1:50 | The tour. VS Code opens files and highlights lines. Barnaby has his back turned and points. | "He walks me through it. He opens each file, highlights the lines, and tells me where to look." |
| 1:50 to 2:10 | Settings window. Switch tours to "Just tell me". Run the question again. VS Code stays still. | "If you do not want him touching your editor, one switch turns it off. He only tells you." |
| 2:10 to 2:30 | Optional: Claude Code finishes a task and Barnaby says "Claude has finished." | "He also watches coding agents, so I can look away from the terminal." Skip this beat if Connect Claude Code is not set up. |
| 2:30 to 2:50 | Slide or README. Four boxes: Muse Spark, Phonon voice, VS Code extension, Tauri overlay. | "Muse Spark through the Meta Model API is the brain. Voice is local. Only the project folder is ever read." |
| 2:50 to 3:00 | Barnaby tips his hat (happy). Show the link. | "Barnaby. Link below." |

## Questions that work on the demo project

- "How does login work? Show me."
- "Where is the password checked?"
- "What happens after the user logs in? Show me."

Words that start a tour: "show me", "walk me through", "take me through".

## If something goes wrong on camera

- No sound: open Settings, press Hear next to Barnaby's voice, check the Phonon status says Ready.
- No tour in VS Code: check the status bar says Barnaby. Reload the VS Code window.
- Slow answer (about 19 seconds): cut the wait when you edit. Do not talk over it.
