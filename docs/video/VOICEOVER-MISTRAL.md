# Voice-over for Mistral Voxtral TTS (paste-ready)

Generate each block as its own audio file. Name them `vo-01`, `vo-02`, and so on. One file per block lets me line each one up with the screen recording.

## What I could and could not confirm

I could not open Mistral's docs page from this container. These facts come from search results that quote Mistral:

- Tags go in square brackets before the text they change, like `[emotion=sad][pace=slow]` (Mistral's own post on X).
- A tag holds until the next tag of the same kind, or the end of the text.
- There are two pace values, `slow` and `fast`.
- There is no pause tag and no emphasis tag. Use full stops and commas for pauses.

I could **not** confirm the full list of emotion names. `calm` and `warm` appeared in search results, not on the docs page.
Check the real list here before you generate: https://docs.mistral.ai/studio/audio/text-to-speech/emotion-tags

Test block 1 first. If the voice reads a tag out loud, that tag name is not valid. Delete it, or use the plain version at the bottom.

## Version with tags

**vo-01** (0:00 to 0:12)
```
[emotion=calm][pace=slow] An AI agent just wrote your whole app. It works. But if someone asks how login works, could you explain it?
```

**vo-02** (0:12 to 0:25)
```
[emotion=calm] Most of us open file after file, and still are not sure.
```

**vo-03** (0:25 to 0:40)
```
[emotion=warm] Barnaby fixes that. He reads your project, and explains it, out loud, in plain words.
```

**vo-04** (0:40 to 1:20)
```
[emotion=calm] I ask: how does login work? Show me. About twenty seconds later, I get a spoken answer, and a step by step tour.
```

**vo-05** (1:20 to 1:55)
```
[emotion=warm] He walks me through it. Each file opens, the right lines are marked, and he tells me why they matter. Now I understand the code I am shipping.
```

**vo-06** (1:55 to 2:15)
```
[emotion=calm] I stay in control. He only reads the folder I choose. And one switch tells him to just point, and leave my editor alone.
```

**vo-07** (2:15 to 2:40, optional)
```
[emotion=calm] He also listens to my coding agents. When Claude Code finishes, he tells me, so I can step away from the terminal.
```

**vo-08** (2:40 to 2:52)
```
[emotion=calm][pace=fast] Muse Spark, through the Meta Model API, reads the project. The voice runs on my own computer.
```

**vo-09** (2:52 to 3:00)
```
[emotion=warm][pace=slow] Barnaby. Understand the code you ship. Link below.
```

## Plain version (no tags)

1. An AI agent just wrote your whole app. It works. But if someone asks how login works, could you explain it?
2. Most of us open file after file, and still are not sure.
3. Barnaby fixes that. He reads your project, and explains it, out loud, in plain words.
4. I ask: how does login work? Show me. About twenty seconds later, I get a spoken answer, and a step by step tour.
5. He walks me through it. Each file opens, the right lines are marked, and he tells me why they matter. Now I understand the code I am shipping.
6. I stay in control. He only reads the folder I choose. And one switch tells him to just point, and leave my editor alone.
7. He also listens to my coding agents. When Claude Code finishes, he tells me, so I can step away from the terminal.
8. Muse Spark, through the Meta Model API, reads the project. The voice runs on my own computer.
9. Barnaby. Understand the code you ship. Link below.

## Notes

- Blocks 4, 5 and 6 sit over screen recording where Barnaby also speaks. When I cut the video, I will keep your voice-over out of his speech.
- Block 5 says "the right lines are marked". Test the tour with VS Code moving before you rely on it.
- Block 7 is optional.
