import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { AppConfig, PetState } from "./types";
import { PetComponent } from "./pet";
import { SpeechBubbleComponent } from "./bubble";
import { PetMenuManager } from "./menu";
import { ActivityFeedManager } from "./feed";
import { PetDragManager } from "./drag";
import { initRegionWatcher, updateClickRegions } from "./regions";
import { initSettingsView } from "./settings";

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

  // Default placement: bottom right above taskbar
  const defaultX = Math.max(100, window.innerWidth - 140);
  const defaultY = Math.max(100, window.innerHeight - 120);
  const initialX = petCfg.x !== null && petCfg.x !== undefined ? petCfg.x : defaultX;
  const initialY = petCfg.y !== null && petCfg.y !== undefined ? petCfg.y : defaultY;

  // 2. Instantiate Components
  const pet = new PetComponent(agentKind, petColor, initialX, initialY);
  const bubble = new SpeechBubbleComponent();
  const menuManager = new PetMenuManager();
  const feedManager = new ActivityFeedManager();
  const dragManager = new PetDragManager(pet);

  // Mount to DOM
  appEl.appendChild(pet.element);
  appEl.appendChild(bubble.element);
  appEl.appendChild(menuManager.leftMenuEl);
  appEl.appendChild(menuManager.rightMenuEl);
  appEl.appendChild(feedManager.element);

  // Helper to sync companion elements with pet position
  const syncPositions = (x: number, y: number) => {
    // Bubble sits above the pet
    bubble.element.style.left = `${x + 36}px`;
    bubble.element.style.top = `${y}px`;

    // If menus are open, update them
    if (!menuManager.leftMenuEl.classList.contains("hidden")) {
      menuManager.leftMenuEl.style.left = `${x}px`;
      menuManager.leftMenuEl.style.top = `${Math.max(10, y - 140)}px`;
    }
  };

  syncPositions(initialX, initialY);

  // 3. Connect Drag & Click Interactions
  dragManager.onMove((x, y) => {
    syncPositions(x, y);
    menuManager.hideAll();
    feedManager.onPetMouseLeave();
  });

  dragManager.onClick(() => {
    // Left-click on pet: toggle pet menu
    const pos = pet.getPosition();
    menuManager.showLeftMenu(pos.x, Math.max(10, pos.y - 140));
  });

  // Right-click on pet: context menu
  pet.element.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    menuManager.showRightMenu(e.clientX, e.clientY);
  });

  // Pet Hover for Activity Feed (1s delay per SPEC 5.1)
  pet.element.addEventListener("mouseenter", () => {
    const pos = pet.getPosition();
    feedManager.onPetMouseEnter(pos.x + 80, Math.max(10, pos.y - 20));
  });

  pet.element.addEventListener("mouseleave", () => {
    feedManager.onPetMouseLeave();
  });

  // Menu Callbacks
  menuManager.setCallbacks({
    onAsk: (question) => {
      pet.setState("thinking");
      bubble.show(`Thinking about: "${question}"...`, [
        {
          id: "stop",
          label: "Stop",
          onClick: async () => {
            await invoke("stop_speech");
            bubble.hide();
            pet.setState("idle");
          },
        },
      ], 60000);
      feedManager.addItem(`Asked: "${question}"`);
      invoke("ask_question", { question }).catch((e) => {
        console.error("ask_question failed:", e);
        bubble.show("I couldn't ask just now.", [], 3000);
        pet.setState("idle");
      });
    },
    onRepeat: () => {
      const items = feedManager.getItems();
      if (items.length > 0) {
        const last = items[0].text;
        pet.setState("talking");
        bubble.show(last, [], 4000);
        setTimeout(() => pet.setState("idle"), 3000);
      }
    },
    onMute: async () => {
      try {
        await invoke("set_voice_enabled", { enabled: false });
        bubble.show("Voice notifications muted.", [], 2500);
        feedManager.addItem("Voice muted");
      } catch (e) {
        console.error("Failed to mute voice:", e);
      }
    },
  });

  // 4. Listen for backend speech events (P2 Voice Out)
  let tourActive = false;

  await listen<{ agent_id: string; sentence: string }>("speech:start", (event) => {
    const { sentence } = event.payload;
    if (!sentence) return;
    if (tourActive) {
      pet.setState("talking");
      return; // the tour bubble already shows this narration with its Back/Next/Stop buttons
    }

    pet.setState("talking");
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
    feedManager.addItem(sentence);
  });

  await listen<{ agent_id: string }>("speech:end", () => {
    if (pet.state === "talking") {
      pet.setState("idle");
    }
    if (tourActive) return;
    setTimeout(() => {
      if (pet.state !== "talking" && !tourActive) {
        bubble.hide();
      }
    }, 1500);
  });

  // Tours (P5): the bubble shows each step with Back / Next / Stop
  const tourControl = (action: string) => invoke("tour_control", { action }).catch(console.error);

  await listen<{
    index: number;
    total: number;
    path: string;
    start_line: number;
    narration: string;
    vscode: boolean;
  }>("tour:step", (event) => {
    const t = event.payload;
    tourActive = true;
    pet.setState("talking");
    const shortPath = t.path.split(/[\\/]/).slice(-2).join("/");
    const where = t.vscode ? "" : `${shortPath}:${t.start_line} - `;
    bubble.show(
      `${t.index + 1}/${t.total}  ${where}${t.narration}`,
      [
        { id: "back", label: "Back", onClick: () => tourControl("back") },
        { id: "next", label: "Next", onClick: () => tourControl("next") },
        { id: "stop", label: "Stop", onClick: () => tourControl("stop") },
      ],
      0,
    );
    feedManager.addItem(`Tour ${t.index + 1}/${t.total}: ${t.narration}`);
  });

  await listen("tour:end", () => {
    tourActive = false;
    pet.setState("idle");
    setTimeout(() => {
      if (!tourActive) bubble.hide();
    }, 2500);
  });

  await listen<{ steps: number }>("tour:offer", (event) => {
    bubble.show(
      `I can walk you through it in ${event.payload.steps} steps. Want me to show you?`,
      [
        {
          id: "show",
          label: "Show me",
          onClick: () => {
            bubble.hide();
            invoke("accept_tour").catch(console.error);
          },
        },
        { id: "no", label: "No thanks", onClick: () => bubble.hide() },
      ],
      30000,
    );
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
    if (pet.state === "talking" && state !== "alert") return;
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

  // Start region watcher for click-through
  initRegionWatcher();

  // Initial welcome greeting
  setTimeout(() => {
    bubble.show("Hi! I'm Barnaby. Drag me anywhere!", [], 5000);
    feedManager.addItem("Barnaby started");
    updateClickRegions();
  }, 300);
}

window.addEventListener("DOMContentLoaded", main);
