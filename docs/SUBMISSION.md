# Submission text for the Meta Global AI Developer Hackathon

Form: https://cerebralvalley.ai/e/meta-ai-developer-hackathon/hackathon/submit
Deadline: Nov 4, 9:00 PM ET (3:00 AM Nov 5 in Alfred's time). Demo video: 3 minutes, YouTube link.

Fields that are Alfred's to answer (not drafted here): Team Name, Team Members, track (Indie Developer Track fits: one builder), problem statement (Build for Your World fits), the optional X post link, the consent checkbox, and all the survey radio buttons (opinions about Meta's API and Muse Code).

## Project Description (limit 3,000 characters)

Barnaby is a desktop pet for developers. He is an old British gentleman in a top hat and a glowing monocle, drawn in pixel art, and he lives on your Windows desktop.

The problem: you open a codebase you did not write, or one an AI agent just changed, and you have no idea where to start. Reading it alone is slow. Asking a chatbot in a browser tab means copying code around.

With Barnaby you ask a question about your project. You type it, or you hold a key and speak it. He reads the project himself, answers out loud in a British voice, and can walk you through the answer. During a tour, VS Code opens each file and highlights the lines while he explains them. He also says where to look in words, like "Look at lines 12 to 20 of login dot ts". A setting turns the navigation off, so he only tells you where to look and never moves your editor.

He also keeps an eye on your coding agents. Through Claude Code hooks and MCP tools, he tells you when an agent has finished, is blocked, or needs your approval. You can look away from the terminal.

He has some character too. Pick him up and he panics. Drop him and he falls, bounces and lands in a heap. During a tour he turns his back to you and points at the screen.

Almost everything runs on your machine. Two things leave it: your question with the code needed to answer it goes to Meta's Model API, and a short voice recording goes to Meta's transcription endpoint. Nothing outside the project folder you choose is read or sent. The API key is kept in Windows Credential Manager.

Built with Tauri 2 (Rust and TypeScript), a local server on 127.0.0.1 with token checks, a VS Code extension, and local Phonon text-to-speech.

## Describe your use of Meta products to build the submitted project (limit 3,000 characters)

Barnaby's brain is Muse Spark (muse-spark-1.3) through the Meta Model API, using the Chat Completions endpoint with tool calling.

When you ask a question, Barnaby sends it with a map of your project: the file tree and function names, built locally and respecting .gitignore. Muse Spark can then call three tools we wrote: read_file, search and list_dir. They are limited to the project folder. The model can call them for up to eight rounds, reading what it needs, before it answers.

It must answer in JSON: one short spoken answer, and an optional tour of up to twelve steps. Each step has a file, a line range and one sentence. Barnaby checks every step before using it. The file must exist, the line numbers are clamped, and the tour is capped. If the JSON is bad, he retries once.

Meta's API has some specifics we built around. The tool_choice setting only accepts "auto". The full assistant message with its tool calls must be sent back before the tool results. Some parameters are not supported. We send reasoning_effort "low" to keep answers quick. On our demo project, one question returned a spoken answer and a six-step tour in about 19 seconds.

For voice input, push-to-talk records a short WAV and sends it to Meta's transcription endpoint (muse-voice-transcribe-1.0). The text then goes to Muse Spark like a typed question.

We did not use Muse Code, Muse Wonder, Muse Image or SAM in this project.

## Why are Meta's models important to your solution? (limit 3,000 characters)

Barnaby has to understand a real codebase, decide what matters, and point to exact lines. That needs a model that handles tool calling and reads in several steps. Muse Spark chose which files to open and followed our JSON format. We check every line range it gives before we use it.

The Model API is OpenAI-compatible, so we could build and test Barnaby's tool loop quickly. The endpoint is a setting. A local Meta open-weights model can take Muse Spark's place for people who do not want code to leave their machine. That matters for a tool that reads private code.

Speed matters because a spoken guide that thinks for a minute is not a guide. Low reasoning effort kept the wait to about 19 seconds for a full tour.

Finally, one Meta account covers both the reasoning model and the transcription model. Barnaby needs one key for the whole voice loop: speak, understand, answer.

## Provide feedback from your experience using partner technologies (limit 150 characters)

Not used. Voice runs locally with Phonon, and the brain runs on the Meta Model API.

## Facts for the survey questions (Alfred answers these himself)

- Slow responses: about 19 seconds for a six-step tour with low reasoning effort.
- Integration friction we hit: tool_choice only "auto", assistant tool_calls must be replayed, reasoning_effort "none" rejected, no stop, n greater than 1, or logprobs.
- No Fireworks, Modal or Baseten credits were used.
- Muse Code was not used.

## Before submitting

1. The public repo must have the final code (push first).
2. The demo video must be 3 minutes or less and on YouTube.
3. Only keep the push-to-talk sentences if push-to-talk works on Alfred's microphone.
4. Only keep the Claude Code sentences if Connect Claude Code works with a real session.
