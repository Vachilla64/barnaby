import { BubbleButton } from "./types";
import { updateClickRegions } from "./regions";

export class SpeechBubbleComponent {
  public element: HTMLElement;
  private textEl: HTMLElement;
  private buttonsEl: HTMLElement;
  private hideTimer: number | null = null;

  constructor() {
    this.element = document.createElement("div");
    this.element.className = "speech-bubble interactive hidden";
    this.element.innerHTML = `
      <div class="bubble-content">
        <div class="bubble-text"></div>
        <div class="bubble-buttons"></div>
      </div>
      <div class="bubble-arrow"></div>
    `;

    this.textEl = this.element.querySelector(".bubble-text") as HTMLElement;
    this.buttonsEl = this.element.querySelector(".bubble-buttons") as HTMLElement;

    // Prevent dragging or clicking through bubble
    this.element.addEventListener("mousedown", (e) => e.stopPropagation());
    this.element.addEventListener("click", (e) => e.stopPropagation());
  }

  public show(text: string, buttons: BubbleButton[] = [], autoHideMs: number = 8000): void {
    if (this.hideTimer) {
      clearTimeout(this.hideTimer);
      this.hideTimer = null;
    }

    this.textEl.textContent = text;
    this.buttonsEl.innerHTML = "";

    if (buttons.length > 0) {
      this.buttonsEl.style.display = "flex";
      buttons.forEach((btn) => {
        const b = document.createElement("button");
        b.className = "bubble-btn interactive";
        b.textContent = btn.label;
        b.addEventListener("click", (e) => {
          e.stopPropagation();
          btn.onClick();
        });
        this.buttonsEl.appendChild(b);
      });
    } else {
      this.buttonsEl.style.display = "none";
    }

    this.element.classList.remove("hidden");
    updateClickRegions();

    if (autoHideMs > 0) {
      this.hideTimer = window.setTimeout(() => {
        this.hide();
      }, autoHideMs);
    }
  }

  public hide(): void {
    if (this.hideTimer) {
      clearTimeout(this.hideTimer);
      this.hideTimer = null;
    }
    this.element.classList.add("hidden");
    updateClickRegions();
  }

  public isVisible(): boolean {
    return !this.element.classList.contains("hidden");
  }
}
