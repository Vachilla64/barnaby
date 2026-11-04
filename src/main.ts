import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { AppConfig } from "./types";
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
          onClick: () => {
            bubble.hide();
            pet.setState("idle");
          },
        },
      ]);
      feedManager.addItem(`Asked: "${question}"`);
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
    onMute: () => {
      bubble.show("Voice notifications muted.", [], 2500);
      feedManager.addItem("Voice muted");
    },
  });

  // 4. Listen for backend events (POST /api/v1/say)
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
    bubble.show("Hi! I'm Devy. Drag me anywhere!", [], 5000);
    feedManager.addItem("Devy started");
    updateClickRegions();
  }, 300);
}

window.addEventListener("DOMContentLoaded", main);
