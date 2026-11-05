# Narrator text for Mistral Voxtral TTS (tags checked against Mistral's docs page)

Generate each paragraph as its own audio file, in order. Paste the text exactly, tags included. Tags are not spoken.

Rules from the docs: `[emotion=value]` and `[pace=value]` go before the text they change and hold until the next tag. Do not use two tags of the same type on the same stretch of text. A wrong value is silently ignored.
Emotions used here: excited, anxious, resigned, tired, warm, playful, curious, proud, confident. All are in the official list of 34.
Pace values: very-slow, slow, moderate, fast, very-fast.

1. [emotion=excited]Letting an AI agent write your app feels like magic. Everything builds. Everything runs. [emotion=anxious][pace=slow]Until someone asks you how login works... [emotion=resigned]and you realize you have no idea.
2. [emotion=tired]You end up clicking through file after file you did not write, trying to piece it together in your head.
3. [emotion=warm]That is why you keep Barnaby on your desk. He reads your codebase and explains it to you out loud, in plain English.
4. [emotion=playful]Instead of digging through files, you just ask him.
5. [emotion=curious]Give him about twenty seconds to read the project, and he starts a tour.
6. [emotion=excited]Watch what happens in VS Code. He does not just dump text at you. He opens each file, highlights the exact lines, and tells you why they matter.
7. [emotion=proud][pace=slow]Now you actually understand the code you are shipping.
8. [emotion=confident]And you stay in control. Barnaby only reads the folder you give him. Don't want him touching your editor? Flip one switch, and he will just tell you where to look.
9. [emotion=hopeful]He even keeps an ear on your coding agents. When a long Claude Code task wraps up, Barnaby tells you, so you do not have to babysit the terminal.
10. [emotion=confident][pace=fast]Under the hood, Barnaby uses Muse Spark through the Meta Model API to understand your project, while his voice runs locally on your machine.
11. [emotion=warm][pace=slow]Barnaby. Understand the code you ship. Find it at the link below.

## Dev lines (Alfred records)

1. Hey Barnaby, how does login actually work in here? Show me.
2. Oh, okay. Got it. (optional)
3. Nice. Thanks, Barnaby. (optional)
