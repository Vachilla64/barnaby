import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { AfterSelect, AppConfig, PetState, PhononStatusInfo, SetupStatus, TourPace } from "./types";
import { PetComponent } from "./pet";
import { SpeechBubbleComponent } from "./bubble";
import { PetMenuManager } from "./menu";
import { ActivityFeedManager } from "./feed";
import { PetDragManager } from "./drag";
import { floorTop } from "./physics";
import { initRegionWatcher, updateClickRegions } from "./regions";
import { initSettingsView } from "./settings";
import { PushToTalk, Recording } from "./ptt";
import { toBase64 } from "./wav";
import { ChatChip, ChatPanel } from "./chat";
import { LookController } from "./look";
import { SetupCard } from "./setup";
import { TourCard, TourStepInfo } from "./tour";

/** What the buttons under "What would you like to know about it?" ask. */
const ABOUT_SELECTION: { label: string; question: string }[] = [
  { label: "Explain it", question: "Explain the code I have selected: what it does and how it fits into the rest of the project." },
  { label: "Why is it here?", question: "Why does the code I have selected exist? What problem does it solve?" },
  { label: "Where is it used?", question: "Show me where the code I have selected is used or called from." },
  { label: "What could break?", question: "What could break if I change the code I have selected?" },
];

async function main() {
  const currentWindow = getCurrentWebviewWindow();
  const appEl = document.getElementById("app") as HTMLElement;

  if (currentWindow.label === "settings") {
    document.body.className = "settings-window";
    initSettingsView(appEl);
    return;
  }

  // --- Overlay Mode ---
  document.body.className = "overlay-window";

  // 1. Fetch Config
  let config: AppConfig;
  try {
    config = await invoke<AppConfig>("get_config");
  } catch (e) {
    console.error("Failed to load config:", e);
    config = {
      port: 4747,
      token: "",
      brain: { model: "muse-spark-1.3" },
      voice: { enabled: true, volume: 0.9, voices: {} },
      push_to_talk: "Ctrl+Alt+Space",
      pets: {},
      projects: [],
    };
  }

  const agentKind = "claude-code";
  const petCfg = config.pets[agentKind] || { colour: "#E8875B", x: null, y: null };
  const petColor = petCfg.colour || "#E8875B";

  // Default placement: bottom right, standing on the floor (the top of the taskbar).
  // A saved position is kept, but never below the floor.
  const petHeight = 132;
  const floorY = floorTop(petHeight);
  const defaultX = Math.max(100, window.innerWidth - 140);
  const initialX = petCfg.x !== null && petCfg.x !== undefined ? petCfg.x : defaultX;
  const initialY = petCfg.y !== null && petCfg.y !== undefined ? Math.min(petCfg.y, floorY) : floorY;

  // 2. Instantiate Components
  const pet = new PetComponent(agentKind, petColor, initialX, initialY);
  const bubble = new SpeechBubbleComponent();
  const menuManager = new PetMenuManager();
  menuManager.setMuted(!config.voice.enabled);
  const feedManager = new ActivityFeedManager();
  const dragManager = new PetDragManager(pet);
  const chat = new ChatPanel();

  // Mount to DOM
  appEl.appendChild(pet.element);
  pet.setPosition(initialX, initialY); // sets which way he faces when guiding
  appEl.appendChild(bubble.element);
  appEl.appendChild(chat.element);
  appEl.appendChild(menuManager.el);
  appEl.appendChild(feedManager.element);
  const look = new LookController(pet, appEl);

  // Helper to sync companion elements with pet position
  const syncPositions = (x: number, y: number) => {
    // Bubble sits above the pet
    bubble.element.style.left = `${x + 36}px`;
    bubble.element.style.top = `${y}px`;

    // The conversation stands beside him, and the pet menu opens above him
    chat.reposition(x, y, pet.element.offsetWidth || 96, pet.element.offsetHeight || petHeight);
    menuManager.reposition(x, y);
  };

  syncPositions(initialX, initialY);

  // 3. Connect Drag & Click Interactions
  dragManager.onMove((x, y) => {
    syncPositions(x, y);
    menuManager.hideAll();
    feedManager.onPetMouseLeave();
  });

  dragManager.onClick((e) => {
    // His monocle starts look mode; the rest of him opens the menu.
    if (look.isOnMonocle(e.clientX, e.clientY)) {
      void toggleLook();
      return;
    }
    const pos = pet.getPosition();
    menuManager.showMenu(pos.x, pos.y);
  });

  // Right-click on pet opens the same menu
  pet.element.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    const pos = pet.getPosition();
    menuManager.showMenu(pos.x, pos.y);
  });

  // Pet Hover for Activity Feed (1s delay per SPEC 5.1)
  pet.element.addEventListener("mouseenter", () => {
    const pos = pet.getPosition();
    feedManager.onPetMouseEnter(pos.x + 80, Math.max(10, pos.y - 20));
  });

  pet.element.addEventListener("mouseleave", () => {
    feedManager.onPetMouseLeave();
  });

  const repeatLast = () => {
    const items = feedManager.getItems();
    if (items.length > 0) {
      pet.setState("talking");
      bubble.show(items[0].text, [], 4000);
      setTimeout(() => pet.setState("idle"), 3000);
    }
  };

  const setVoiceMuted = async (muted: boolean) => {
    try {
      await invoke("set_voice_enabled", { enabled: !muted });
      menuManager.setMuted(muted);
      bubble.show(muted ? "Voice notifications muted." : "Voice is back on.", [], 2500);
      feedManager.addItem(muted ? "Voice muted" : "Voice unmuted");
    } catch (e) {
      console.error("Failed to change voice mute:", e);
    }
  };
  // The Mute tile toggles; the "mute" voice command only ever mutes.
  const muteVoice = () => setVoiceMuted(true);

  // --- Asking and the conversation ---

  /** Send a question. The backend echoes it as `chat:user`, which puts it on the list. */
  const ask = (question: string, shown?: string) => {
    pet.setState("thinking");
    invoke("ask_question", { question, shown: shown ?? null }).catch((e) => {
      console.error("ask_question failed:", e);
      chat.setThinking(false);
      chat.addError("I couldn't ask just now.");
      pet.setState("idle");
    });
  };

  // --- Voice in: push-to-talk, and listening by the microphone button ---
  const ptt = new PushToTalk();

  const sendRecording = async (recording: Recording | null) => {
    chat.setListening(false);
    if (!recording) {
      bubble.hide();
      pet.setState("idle");
      return;
    }
    bubble.show("One moment...", [], 0);
    try {
      await invoke("ptt_audio", { wavBase64: toBase64(recording.wav) });
    } catch (e) {
      console.error("ptt_audio failed:", e);
      bubble.show("I didn't catch that.", [], 3000);
      pet.setState("idle");
    }
  };

  /** Hands-free: record until the person has spoken and gone quiet. Pressing the button again stops it. */
  const listenOnce = async () => {
    if (ptt.isRecording()) {
      const recording = ptt.stop();
      if (!recording) chat.addNote("I did not hear anything.");
      await sendRecording(recording);
      return;
    }
    chat.open();
    chat.setListening(true);
    bubble.hide();
    pet.setState("thinking");
    try {
      await ptt.start({
        silenceMs: 1200,
        noSpeechMs: 8000,
        maxMs: 25000,
        onDone: () => {
          const recording = ptt.stop();
          if (!recording) chat.addNote("I did not hear anything.");
          void sendRecording(recording);
        },
      });
    } catch (e) {
      console.error("Microphone unavailable:", e);
      chat.setListening(false);
      chat.addError("I can't use the microphone. Check the Windows microphone permission.");
      pet.setState("idle");
    }
  };

  chat.setCallbacks({
    onSend: (text) => ask(text),
    onMic: () => void listenOnce(),
    onNewChat: () => void invoke("reset_conversation"),
    onSetup: () => showSetup(),
    onStop: () => {
      invoke("stop_speech").catch(console.error);
      chat.setSpeaking(false);
      pet.setState("idle");
    },
  });

  // --- First-run guide ---
  let setupCard: SetupCard | null = null;
  const showSetup = () => {
    chat.open();
    if (setupCard?.element.isConnected) return;
    setupCard = new SetupCard({
      onReady: () => {
        invoke("say_text", {
          text: "Splendid. That is everything. Click my monocle, select some code in VS Code, and I shall take a look.",
        }).catch(console.error);
      },
      onLeave: () => {
        setupCard?.element.remove();
        setupCard = null;
        chat.unpin();
        invoke("finish_onboarding").catch(console.error);
        updateClickRegions();
      },
      openSettings: () => void invoke("open_settings"),
    });
    chat.addElement(setupCard.element, true);
  };

  // Menu Callbacks
  menuManager.setCallbacks({
    onChat: () => chat.open(true),
    onRepeat: () => repeatLast(),
    onMute: () => void setVoiceMuted(!menuManager.isMuted()),
  });

  // --- Backend events: the conversation ---
  await listen<{ text: string }>("chat:user", (event) => {
    bubble.hide();
    chat.open();
    chat.addUser(event.payload.text);
    chat.setThinking(true);
    pet.setState("thinking");
    feedManager.addItem(`Asked: "${event.payload.text}"`);
  });

  const followUps = (): ChatChip[] => [
    { label: "Tell me more", onClick: () => ask("Tell me more about that.", "Tell me more") },
    { label: "Why?", onClick: () => ask("Why is it done that way?", "Why?") },
  ];

  await listen<{ speech: string; agenda: string[]; tour_started: boolean }>("chat:answer", (event) => {
    const a = event.payload;
    chat.setThinking(false);
    const chips: ChatChip[] = [];
    if (a.agenda.length > 0 && !a.tour_started) {
      chips.push({
        label: a.agenda.length === 1 ? "Show me" : `Show me (${a.agenda.length} stops)`,
        onClick: () => {
          chat.addUser("Show me");
          invoke("accept_tour").catch(console.error);
        },
      });
    }
    chips.push(...followUps());
    // When the walk starts by itself the card takes over, so the map is not drawn twice.
    chat.addBarnaby(a.speech, chips, a.tour_started ? [] : a.agenda);
  });

  await listen<{ text: string }>("chat:error", (event) => {
    chat.setThinking(false);
    chat.open();
    chat.addError(event.payload.text);
  });

  await listen("chat:reset", () => {
    chat.clear();
    chat.addBarnaby("A fresh start. What shall we look at?");
  });

  // --- Backend events: look mode ---
  const toggleLook = async () => {
    try {
      await look.toggle();
    } catch (e) {
      if (String(e) === "vscode") {
        chat.addBarnaby("I cannot see VS Code yet, so I have nothing to look at. Let us link it first.");
        showSetup();
      } else {
        console.error("set_look_mode failed:", e);
      }
    }
  };

  await listen<{ on: boolean; reason?: string }>("look:state", (event) => {
    const { on, reason } = event.payload;
    look.setOn(on, reason);
    if (on) {
      bubble.show("Select some code in VS Code and I shall take a look. Click my monocle again to cancel.", [], 7000);
    } else if (reason === "timeout") {
      bubble.show("No code today? Another time, then.", [], 3500);
    } else if (reason === "disconnected") {
      bubble.show("VS Code has gone. Nothing to look at.", [], 3500);
    } else {
      bubble.hide();
    }
  });

  await listen<{ x: number; y: number }>("look:cursor", (event) => look.moveRing(event.payload.x, event.payload.y));

  await listen<{ name: string; start_line: number; end_line: number; after: AfterSelect }>("look:selected", (event) => {
    const { name, start_line, end_line, after } = event.payload;
    bubble.hide();
    chat.open();
    chat.addNote(start_line === end_line ? `${name}, line ${start_line}` : `${name}, lines ${start_line} to ${end_line}`);
    pet.setState("happy");
    setTimeout(() => {
      if (pet.state === "happy") pet.setState("idle");
    }, 1200);

    if (after === "explain") {
      ask(ABOUT_SELECTION[0].question, ABOUT_SELECTION[0].label);
    } else if (after === "listen") {
      chat.addBarnaby("I am listening. What would you like to know about it?");
      void listenOnce();
    } else {
      chat.addBarnaby(
        "What would you like to know about it?",
        ABOUT_SELECTION.map((s) => ({ label: s.label, onClick: () => ask(s.question, s.label) })),
      );
      invoke("say_text", { text: "I see it. What would you like to know?" }).catch(console.error);
    }
  });

  // 4. Listen for backend speech events (P2 Voice Out)
  let tourActive = false;

  await listen<{ agent_id: string; sentence: string; kind?: string }>("speech:start", (event) => {
    const { sentence, agent_id, kind } = event.payload;
    if (!sentence) return;
    chat.touch();
    if (tourActive) {
      pet.setState("guiding");
      return; // the tour bubble already shows this narration with its Back/Next/Stop buttons
    }

    pet.setState("talking");
    feedManager.addItem(sentence);
    // His own answers are on the conversation list already; only other speech gets a bubble.
    if (agent_id === "barnaby" && (kind === "answer" || chat.isOpen())) {
      chat.setSpeaking(true);
      return;
    }
    bubble.show(sentence, [
      {
        id: "stop",
        label: "Stop",
        onClick: async () => {
          await invoke("stop_speech");
          bubble.hide();
          pet.setState("idle");
        },
      },
    ], 15000);
  });

  await listen<{ agent_id: string }>("speech:end", () => {
    if (pet.state === "talking") {
      pet.setState("idle");
    }
    chat.setSpeaking(false);
    if (tourActive) return;
    setTimeout(() => {
      if (pet.state !== "talking" && !tourActive) {
        bubble.hide();
      }
    }, 1500);
  });

  // Tours: a card in the conversation shows the plan, where he is in it, and Back / Next / Stop.
  // He speaks each stop; the words are written out only when his voice is off.
  const tourControl = (action: string) => invoke("tour_control", { action }).catch(console.error);
  let tourCard: TourCard | null = null;

  const voiceIsOn = async (): Promise<boolean> => {
    if (menuManager.isMuted()) return false;
    try {
      return (await invoke<PhononStatusInfo>("get_phonon_status")).state === "ready";
    } catch {
      return false;
    }
  };

  await listen<{ labels: string[]; pace: TourPace }>("tour:start", (event) => {
    const { labels, pace } = event.payload;
    tourActive = true;
    bubble.hide();
    tourCard = new TourCard(labels, pace, tourControl);
    const card = tourCard;
    void voiceIsOn().then((on) => card.setShowWords(!on));
    chat.open();
    chat.setFocus(true);
    chat.addElement(card.element, true);
  });

  await listen<TourStepInfo>("tour:step", (event) => {
    const t = event.payload;
    tourActive = true;
    pet.setState("guiding"); // turns his back and gestures at the screen
    tourCard?.setStep(t);
    chat.touch();
    feedManager.addItem(`Tour ${t.index + 1}/${t.total}: ${t.title}`);
  });

  // His turn is over and it is yours: bring the card back if you tucked it away.
  await listen("tour:waiting", () => {
    chat.open();
    tourCard?.setWaiting();
  });

  await listen<{ finished: boolean; recap: string }>("tour:end", (event) => {
    const { finished, recap } = event.payload;
    tourActive = false;
    pet.setState("idle");
    tourCard?.finish(finished);
    tourCard = null;
    chat.unpin();
    chat.setFocus(false);
    if (finished && recap) chat.addBarnaby(`The thing to remember: ${recap}`, followUps());
    else if (!finished) chat.addNote("Walk ended. Ask me anything else.");
    else chat.setChips(followUps());
  });

  await listen<{ agent_id: string; text: string }>("speech:bubble", (event) => {
    const text = event.payload.text;
    if (!text) return;
    bubble.show(text, [], 4000);
    feedManager.addItem(text);
  });

  // Agent link (P3): hooks and MCP tools move the pet between states
  await listen<{ agent_kind: string; state: string; text?: string | null }>("agent:state", (event) => {
    const { state, text } = event.payload;
    if ((pet.state === "talking" || pet.state === "guiding") && state !== "alert") return;
    pet.setState(state as PetState);
    if (text) feedManager.addItem(text);
  });

  // Fallback for direct pet:say events
  await listen<{ agent_kind?: string; text: string }>("pet:say", (event) => {
    const text = event.payload.text;
    if (!text) return;

    pet.setState("talking");
    const duration = Math.max(3000, text.length * 65);
    bubble.show(text, [], duration);
    feedManager.addItem(text);

    setTimeout(() => {
      if (pet.state === "talking") {
        pet.setState("idle");
      }
    }, Math.max(2500, text.length * 60));
  });

  // Voice in (P6): hold the push-to-talk key to speak; release to send
  await listen("ptt:start", async () => {
    chat.open();
    chat.setListening(true);
    bubble.hide();
    pet.setState("thinking");
    try {
      await ptt.start();
    } catch (e) {
      console.error("Microphone unavailable:", e);
      chat.setListening(false);
      chat.addError("I can't use the microphone. Check the Windows microphone permission.");
      pet.setState("idle");
    }
  });
  await listen("ptt:stop", async () => {
    await sendRecording(ptt.stop());
  });
  await listen("ptt:fallback", () => {
    pet.setState("idle");
    chat.open(true);
  });
  await listen("command:repeat", () => repeatLast());
  await listen("command:mute", () => void muteVoice());

  // Start region watcher for click-through
  initRegionWatcher();

  // Everything mounted and listening: now the backend may show the overlay window.
  await invoke("overlay_ready");

  // Welcome. A first run gets the setup guide; after that he just says what he can do.
  setTimeout(async () => {
    feedManager.addItem("Barnaby started");
    let firstRun = !config.onboarding_done;
    try {
      const status = await invoke<SetupStatus>("setup_status");
      firstRun = !status.onboarding_done;
    } catch (e) {
      console.error("setup_status failed:", e);
    }
    if (firstRun) {
      showSetup();
    } else {
      bubble.show("Good day! Click my monocle, select some code, and I shall take a look.", [], 6000);
    }
    updateClickRegions();
  }, 300);

  // The speech engine can take a while to load. Say so, start it if it is down, and greet when ready.
  const warmUpVoice = async () => {
    let announced = false;
    let offlinePolls = 0;
    let startRequested = false;
    for (let i = 0; i < 150; i++) {
      let info: PhononStatusInfo | null = null;
      try {
        info = await invoke<PhononStatusInfo>("get_phonon_status");
      } catch (e) {
        console.error("get_phonon_status failed:", e);
      }
      if (info?.state === "ready") {
        if (announced) {
          pet.setState("happy");
          bubble.show("Ah, that's better. My voice is ready.", [], 4000);
          setTimeout(() => pet.setState("idle"), 1800);
          invoke("say_text", { text: "Ahem. Good evening. Barnaby, at your service." }).catch(console.error);
        }
        return;
      }
      if (!announced) {
        announced = true;
        pet.setState("thinking");
        bubble.show("Clearing my throat... one moment.", [], 0);
      }
      const down = info?.state === "offline" || info?.state === "failed";
      offlinePolls = down ? offlinePolls + 1 : 0;
      if (down && offlinePolls >= 2 && !startRequested && info?.uv_installed && info?.ptts_installed) {
        startRequested = true; // the backend normally starts it; this is the safety net
        invoke("start_phonon").catch(console.error);
      }
      await new Promise((r) => setTimeout(r, 2000));
    }
    pet.setState("idle");
    bubble.show("My voice would not start. Open Settings to see why.", [], 8000);
  };
  setTimeout(() => void warmUpVoice(), 5600);
}

window.addEventListener("DOMContentLoaded", main);
