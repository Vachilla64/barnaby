import { invoke } from "@tauri-apps/api/core";
import { updateClickRegions } from "./regions";

export class PetMenuManager {
  public leftMenuEl: HTMLElement;
  public rightMenuEl: HTMLElement;
  private leaveTimer: number | null = null;
  private onAskCallback?: (question: string) => void;
  private onRepeatCallback?: () => void;
  private onMuteCallback?: () => void;

  constructor() {
    // 1. Create Left-Click Pet Menu
    this.leftMenuEl = document.createElement("div");
    this.leftMenuEl.className = "pet-menu left-menu interactive hidden";
    this.leftMenuEl.innerHTML = `
      <div class="menu-items">
        <div class="menu-item" data-action="ask">Ask…</div>
        <div class="menu-item" data-action="mute">Mute</div>
        <div class="menu-item" data-action="repeat">Repeat last</div>
        <div class="menu-item" data-action="hide">Hide for 1 hour</div>
      </div>
      <div class="menu-ask-box hidden">
        <input type="text" class="ask-input interactive" placeholder="Ask about this repo..." />
        <button class="ask-submit-btn interactive">Ask</button>
      </div>
    `;

    // 2. Create Right-Click Context Menu
    this.rightMenuEl = document.createElement("div");
    this.rightMenuEl.className = "pet-menu context-menu interactive hidden";
    this.rightMenuEl.innerHTML = `
      <div class="menu-items">
        <div class="menu-item" data-action="settings">Settings</div>
        <div class="menu-divider"></div>
        <div class="menu-item menu-item-danger" data-action="quit">Quit</div>
      </div>
    `;

    this.setupEvents();
  }

  public setCallbacks(callbacks: {
    onAsk?: (question: string) => void;
    onRepeat?: () => void;
    onMute?: () => void;
  }): void {
    this.onAskCallback = callbacks.onAsk;
    this.onRepeatCallback = callbacks.onRepeat;
    this.onMuteCallback = callbacks.onMute;
  }

  private setupEvents(): void {
    // Stop propagation so clicking inside menu doesn't trigger window drag
    this.leftMenuEl.addEventListener("mousedown", (e) => e.stopPropagation());
    this.rightMenuEl.addEventListener("mousedown", (e) => e.stopPropagation());

    // 200ms mouseLeave debounce grace period per user rule
    const attachDebounce = (el: HTMLElement) => {
      el.addEventListener("mouseenter", () => {
        if (this.leaveTimer) {
          clearTimeout(this.leaveTimer);
          this.leaveTimer = null;
        }
      });

      el.addEventListener("mouseleave", () => {
        if (this.leaveTimer) clearTimeout(this.leaveTimer);
        this.leaveTimer = window.setTimeout(() => {
          this.hideAll();
        }, 200);
      });
    };

    attachDebounce(this.leftMenuEl);
    attachDebounce(this.rightMenuEl);

    // Left menu clicks
    this.leftMenuEl.addEventListener("click", (e) => {
      e.stopPropagation();
      const target = (e.target as HTMLElement).closest(".menu-item") as HTMLElement | null;
      if (!target) return;
      const action = target.dataset.action;

      if (action === "ask") {
        const askBox = this.leftMenuEl.querySelector(".menu-ask-box") as HTMLElement;
        const input = askBox.querySelector(".ask-input") as HTMLInputElement;
        askBox.classList.toggle("hidden");
        updateClickRegions();
        if (!askBox.classList.contains("hidden")) {
          setTimeout(() => input.focus(), 50);
        }
      } else if (action === "mute") {
        if (this.onMuteCallback) this.onMuteCallback();
        this.hideAll();
      } else if (action === "repeat") {
        if (this.onRepeatCallback) this.onRepeatCallback();
        this.hideAll();
      } else if (action === "hide") {
        // Hide pet for 1 hour
        const petContainer = document.querySelector(".pet-container") as HTMLElement;
        if (petContainer) {
          petContainer.style.display = "none";
          setTimeout(() => {
            petContainer.style.display = "";
            updateClickRegions();
          }, 3600000);
        }
        this.hideAll();
      }
    });

    // Ask input submission
    const askBox = this.leftMenuEl.querySelector(".menu-ask-box") as HTMLElement;
    const askInput = askBox.querySelector(".ask-input") as HTMLInputElement;
    const askBtn = askBox.querySelector(".ask-submit-btn") as HTMLElement;

    const submitAsk = () => {
      const q = askInput.value.trim();
      if (q && this.onAskCallback) {
        this.onAskCallback(q);
        askInput.value = "";
        this.hideAll();
      }
    };

    askBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      submitAsk();
    });

    askInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.stopPropagation();
        submitAsk();
      }
    });

    // Right menu clicks
    this.rightMenuEl.addEventListener("click", (e) => {
      e.stopPropagation();
      const target = (e.target as HTMLElement).closest(".menu-item") as HTMLElement | null;
      if (!target) return;
      const action = target.dataset.action;

      if (action === "settings") {
        invoke("open_settings").catch(console.error);
        this.hideAll();
      } else if (action === "quit") {
        invoke("exit_app").catch(console.error);
        this.hideAll();
      }
    });
  }

  public showLeftMenu(x: number, y: number): void {
    this.rightMenuEl.classList.add("hidden");
    this.leftMenuEl.style.left = `${x}px`;
    this.leftMenuEl.style.top = `${y}px`;
    this.leftMenuEl.classList.remove("hidden");
    updateClickRegions();
  }

  public showRightMenu(x: number, y: number): void {
    this.leftMenuEl.classList.add("hidden");
    this.rightMenuEl.style.left = `${x}px`;
    this.rightMenuEl.style.top = `${y}px`;
    this.rightMenuEl.classList.remove("hidden");
    updateClickRegions();
  }

  public hideAll(): void {
    if (this.leaveTimer) {
      clearTimeout(this.leaveTimer);
      this.leaveTimer = null;
    }
    const askBox = this.leftMenuEl.querySelector(".menu-ask-box") as HTMLElement;
    if (askBox) askBox.classList.add("hidden");

    this.leftMenuEl.classList.add("hidden");
    this.rightMenuEl.classList.add("hidden");
    updateClickRegions();
  }
}
