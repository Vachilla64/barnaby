import { PetState } from "./types";

export function createPetSvg(color: string): string {
  return `
    <svg viewBox="0 0 72 64" width="72" height="64" class="pet-svg" xmlns="http://www.w3.org/2000/svg">
      <!-- Defs for gradients & shadows -->
      <defs>
        <filter id="pet-shadow" x="-20%" y="-20%" width="140%" height="140%">
          <feDropShadow dx="0" dy="4" stdDeviation="3" flood-opacity="0.35" />
        </filter>
        <radialGradient id="pet-grad" cx="40%" cy="30%" r="70%">
          <stop offset="0%" stop-color="#ffffff" stop-opacity="0.3" />
          <stop offset="100%" stop-color="#000000" stop-opacity="0.15" />
        </radialGradient>
      </defs>

      <!-- Main Blob Body -->
      <path class="pet-body"
        d="M 12,36 C 8,16 24,8 36,8 C 48,8 64,16 60,36 C 58,52 48,56 36,56 C 24,56 14,52 12,36 Z"
        fill="${color}"
        filter="url(#pet-shadow)"
      />
      <!-- Body highlight overlay -->
      <path class="pet-body-highlight"
        d="M 12,36 C 8,16 24,8 36,8 C 48,8 64,16 60,36 C 58,52 48,56 36,56 C 24,56 14,52 12,36 Z"
        fill="url(#pet-grad)"
      />

      <!-- Left Eye -->
      <g class="pet-eye pet-eye-left">
        <!-- Open Eye -->
        <ellipse class="eye-open" cx="26" cy="28" rx="5" ry="6" fill="#ffffff" />
        <circle class="eye-pupil" cx="26" cy="28" r="3" fill="#1b1b1f" />
        <circle class="eye-shine" cx="24.5" cy="26" r="1.2" fill="#ffffff" />
        <!-- Sleeping Eye (hidden by default) -->
        <path class="eye-closed" d="M 21,29 Q 26,34 31,29" fill="none" stroke="#1b1b1f" stroke-width="2.5" stroke-linecap="round" />
      </g>

      <!-- Right Eye -->
      <g class="pet-eye pet-eye-right">
        <!-- Open Eye -->
        <ellipse class="eye-open" cx="46" cy="28" rx="5" ry="6" fill="#ffffff" />
        <circle class="eye-pupil" cx="46" cy="28" r="3" fill="#1b1b1f" />
        <circle class="eye-shine" cx="44.5" cy="26" r="1.2" fill="#ffffff" />
        <!-- Sleeping Eye (hidden by default) -->
        <path class="eye-closed" d="M 41,29 Q 46,34 51,29" fill="none" stroke="#1b1b1f" stroke-width="2.5" stroke-linecap="round" />
      </g>

      <!-- Mouth -->
      <g class="pet-mouth-group">
        <!-- Idle Smile -->
        <path class="mouth-idle" d="M 31,39 Q 36,44 41,39" fill="none" stroke="#1b1b1f" stroke-width="2.2" stroke-linecap="round" />
        <!-- Open Mouth (talking / happy / alert) -->
        <path class="mouth-open" d="M 31,38 Q 36,48 41,38 Z" fill="#b91c1c" stroke="#1b1b1f" stroke-width="1.8" />
      </g>

      <!-- Cheeks -->
      <ellipse class="pet-cheek" cx="19" cy="35" rx="3" ry="2" fill="#ff4d4d" opacity="0.35" />
      <ellipse class="pet-cheek" cx="53" cy="35" rx="3" ry="2" fill="#ff4d4d" opacity="0.35" />
    </svg>

    <!-- Overlay accessory badges -->
    <div class="accessory gear-badge" title="Working">
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="3"></circle>
        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
      </svg>
    </div>

    <div class="accessory alert-badge" title="Alert">!</div>

    <div class="accessory thinking-dots" title="Thinking">
      <span></span><span></span><span></span>
    </div>

    <div class="accessory sleep-container" title="Sleeping">
      <span class="sleep-z z1">z</span>
      <span class="sleep-z z2">z</span>
      <span class="sleep-z z3">z</span>
    </div>
  `;
}

export class PetComponent {
  public element: HTMLElement;
  public kind: string;
  public state: PetState = "idle";
  public color: string;

  constructor(kind: string, color: string, initialX: number, initialY: number) {
    this.kind = kind;
    this.color = color;

    this.element = document.createElement("div");
    this.element.className = `pet pet-container state-${this.state} interactive`;
    this.element.dataset.kind = kind;
    this.element.style.left = `${initialX}px`;
    this.element.style.top = `${initialY}px`;
    this.element.innerHTML = createPetSvg(color);
  }

  public setState(state: PetState): void {
    if (this.state === state) return;
    this.element.classList.remove(`state-${this.state}`);
    this.state = state;
    this.element.classList.add(`state-${state}`);
  }

  public setPosition(x: number, y: number): void {
    this.element.style.left = `${x}px`;
    this.element.style.top = `${y}px`;
  }

  public getPosition(): { x: number; y: number } {
    const rect = this.element.getBoundingClientRect();
    return { x: rect.left, y: rect.top };
  }
}
