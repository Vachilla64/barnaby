import { invoke } from "@tauri-apps/api/core";
import { updateClickRegions } from "./regions";

/**
 * Barnaby's click menu: round buttons fanned out in a half circle above him (he stands on the floor,
 * so only the top half is used). The label of the button under the cursor shows on a little top hat.
 */

const NAME = "Sir Barnaby";
const RADIUS = 118;
const BTN = 46;
const ASK_BTN = 56;
/** Angles in degrees, 180 = left of him, 0 = right of him. */
const ANGLES = [165, 135, 105, 75, 45, 15];
const EDGE_GAP = 8;
/** The overlay is laid out in CSS pixels; the pet sprite is drawn at this size (pet-art.ts). */
const PET_W = 96;
const PET_H = 132;

const WIDTH = 2 * RADIUS + 90;
const CENTRE_X = WIDTH / 2;
/** The centre of the arc sits at his chest; the hat label and the ask box are above the arc. */
const CENTRE_Y = RADIUS + 82;
const HEIGHT = CENTRE_Y + BTN / 2 + 8;

/** A half circle from the left of the pet over his head to the right. */
function arc(r: number): string {
  return `M ${CENTRE_X - r} ${CENTRE_Y} A ${r} ${r} 0 0 1 ${CENTRE_X + r} ${CENTRE_Y}`;
}

const ICONS = {
  ask: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M21 12a8 8 0 0 1-11.6 7.1L4 20.5l1.5-4.6A8 8 0 1 1 21 12z"/><path d="M9.5 10a2.5 2.5 0 1 1 3.6 2.2c-.7.4-1.1.8-1.1 1.6"/><circle cx="12" cy="16.7" r=".6"/></svg>`,
  mute: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M11 5 6 9H3v6h3l5 4z"/><path d="M15.5 9a4 4 0 0 1 0 6"/><path d="M18.5 6.5a8 8 0 0 1 0 11"/></svg>`,
  unmute: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M11 5 6 9H3v6h3l5 4z"/><path d="m16 9 5 6"/><path d="m21 9-5 6"/></svg>`,
  repeat: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 12a9 9 0 1 0 3-6.7"/><path d="M3 4v5h5"/></svg>`,
  hide: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M21 13.5A8.5 8.5 0 1 1 10.5 3a7 7 0 0 0 10.5 10.5z"/></svg>`,
  settings: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/></svg>`,
  quit: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><path d="m16 17 5-5-5-5"/><path d="M21 12H9"/></svg>`,
};

interface Item {
  action: string;
  icon: string;
  label: string;
  extra?: string;
}

/** Left to right along the arc. */
const ITEMS: Item[] = [
  { action: "quit", icon: ICONS.quit, label: "Quit", extra: "danger" },
  { action: "repeat", icon: ICONS.repeat, label: "Say that again" },
  { action: "chat", icon: ICONS.ask, label: "Chat with me", extra: "primary" },
  { action: "mute", icon: ICONS.mute, label: "Mute" },
  { action: "hide", icon: ICONS.hide, label: "Hide for an hour" },
  { action: "settings", icon: ICONS.settings, label: "Settings" },
];

export class PetMenuManager {
  public el: HTMLElement;
  private plateText: HTMLElement;
  private leaveTimer: number | null = null;
  private muted = false;
  private onChatCallback?: () => void;
  private onRepeatCallback?: () => void;
  private onMuteCallback?: () => void;

  constructor() {
    this.el = document.createElement("div");
    this.el.className = "radial-menu hidden";
    this.el.style.width = `${WIDTH}px`;
    this.el.style.height = `${HEIGHT}px`;

    const buttons = ITEMS.map((item, i) => {
      const size = item.extra === "primary" ? ASK_BTN : BTN;
      const rad = (ANGLES[i] * Math.PI) / 180;
      const x = CENTRE_X + RADIUS * Math.cos(rad);
      const y = CENTRE_Y - RADIUS * Math.sin(rad);
      // Offset back to the middle of the arc: where each button grows out of.
      const dx = CENTRE_X - x;
      const dy = CENTRE_Y - y;
      return `<button type="button" class="radial-btn interactive ${item.extra ?? ""}" data-action="${item.action}"
        data-label="${item.label}" aria-label="${item.label}"
        style="left:${x - size / 2}px; top:${y - size / 2}px; width:${size}px; height:${size}px; --dx:${dx}px; --dy:${dy}px; --i:${i};">
        <span class="radial-icon">${item.icon}</span>
      </button>`;
    }).join("");

    this.el.innerHTML = `
      <svg class="radial-track" width="${WIDTH}" height="${HEIGHT}" viewBox="0 0 ${WIDTH} ${HEIGHT}" aria-hidden="true">
        <path d="${arc(RADIUS)}" class="track-edge"/>
        <path d="${arc(RADIUS)}" class="track-body"/>
        <path d="${arc(RADIUS - 30)}" class="track-band"/>
      </svg>
      <div class="hat-plate" style="left:${CENTRE_X}px;">
        <div class="hat-crown"><span class="hat-text">${NAME}</span></div>
        <div class="hat-band"></div>
        <div class="hat-brim"></div>
      </div>
      ${buttons}
    `;
    this.plateText = this.el.querySelector(".hat-text") as HTMLElement;
    this.setupEvents();
  }

  public setCallbacks(callbacks: {
    onChat?: () => void;
    onRepeat?: () => void;
    onMute?: () => void;
  }): void {
    this.onChatCallback = callbacks.onChat;
    this.onRepeatCallback = callbacks.onRepeat;
    this.onMuteCallback = callbacks.onMute;
  }

  /** Keep the Mute button honest about what pressing it does. */
  public setMuted(muted: boolean): void {
    this.muted = muted;
    const btn = this.el.querySelector<HTMLElement>('[data-action="mute"]');
    if (!btn) return;
    btn.classList.toggle("is-on", muted);
    btn.dataset.label = muted ? "Unmute" : "Mute";
    btn.setAttribute("aria-label", btn.dataset.label);
    (btn.querySelector(".radial-icon") as HTMLElement).innerHTML = muted ? ICONS.unmute : ICONS.mute;
  }

  public isMuted(): boolean {
    return this.muted;
  }

  private setupEvents(): void {
    // Clicking inside the menu must not start a window drag.
    this.el.addEventListener("mousedown", (e) => e.stopPropagation());

    // The buttons grow out of the pet, so measure their click areas again once they settle.
    this.el.addEventListener("animationend", () => updateClickRegions());

    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape") this.hideAll();
    });

    // A short grace period so the cursor can cross the gap between two buttons.
    const cancelLeave = () => {
      if (this.leaveTimer) {
        clearTimeout(this.leaveTimer);
        this.leaveTimer = null;
      }
    };
    const startLeave = () => {
      cancelLeave();
      this.leaveTimer = window.setTimeout(() => this.hideAll(), 450);
    };

    for (const el of this.el.querySelectorAll<HTMLElement>(".radial-btn")) {
      el.addEventListener("mouseenter", () => {
        cancelLeave();
        if (el.dataset.label) this.plateText.textContent = el.dataset.label;
      });
      el.addEventListener("mouseleave", () => {
        this.plateText.textContent = NAME;
        startLeave();
      });
    }

    this.el.addEventListener("click", (e) => {
      e.stopPropagation();
      const target = (e.target as HTMLElement).closest(".radial-btn") as HTMLElement | null;
      if (!target) return;
      this.runAction(target.dataset.action ?? "");
    });
  }

  private runAction(action: string): void {
    switch (action) {
      case "chat":
        this.onChatCallback?.();
        this.hideAll();
        break;
      case "mute":
        this.onMuteCallback?.();
        this.hideAll();
        break;
      case "repeat":
        this.onRepeatCallback?.();
        this.hideAll();
        break;
      case "hide": {
        // Hide the pet for 1 hour
        const petContainer = document.querySelector(".pet-container") as HTMLElement | null;
        if (petContainer) {
          petContainer.style.display = "none";
          setTimeout(() => {
            petContainer.style.display = "";
            updateClickRegions();
          }, 3600000);
        }
        this.hideAll();
        break;
      }
      case "settings":
        invoke("open_settings").catch(console.error);
        this.hideAll();
        break;
      case "quit":
        invoke("exit_app").catch(console.error);
        this.hideAll();
        break;
    }
  }

  /** Centre the arc on the pet's chest, and keep the whole menu on screen. */
  public reposition(petX: number, petY: number): void {
    if (this.el.classList.contains("hidden")) return;
    const petEl = document.querySelector<HTMLElement>(".pet-container");
    const w = petEl?.offsetWidth || PET_W;
    const h = petEl?.offsetHeight || PET_H;
    const left = petX + w / 2 - CENTRE_X;
    const top = petY + h / 2 - CENTRE_Y;
    this.el.style.left = `${Math.max(EDGE_GAP, Math.min(left, window.innerWidth - WIDTH - EDGE_GAP))}px`;
    this.el.style.top = `${Math.max(EDGE_GAP, top)}px`;
  }

  /** `petX`/`petY` are the pet's top-left corner. Called again, it closes the menu. */
  public showMenu(petX: number, petY: number): void {
    if (this.isOpen()) {
      this.hideAll();
      return;
    }
    this.el.classList.remove("hidden");
    this.reposition(petX, petY);
    // Restart the grow-out animation each time.
    this.el.classList.remove("opening");
    void this.el.offsetWidth;
    this.el.classList.add("opening");
    updateClickRegions();
  }

  public isOpen(): boolean {
    return !this.el.classList.contains("hidden");
  }

  public hideAll(): void {
    if (this.leaveTimer) {
      clearTimeout(this.leaveTimer);
      this.leaveTimer = null;
    }
    this.el.classList.remove("opening");
    this.el.classList.add("hidden");
    this.plateText.textContent = NAME;
    updateClickRegions();
  }
}
