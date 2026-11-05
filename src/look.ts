import { invoke } from "@tauri-apps/api/core";
import { PetComponent } from "./pet";

/**
 * Look mode, the pet side: clicking his monocle makes him wait for a selection in VS Code. While he
 * waits, a monocle ring follows the mouse. The ring is drawn here, not by Windows, so it never
 * gets in the way of a click.
 */

/** How far outside the lens a click still counts as being on it. */
const HIT_PADDING = 12;
const RING = 44;

export class LookController {
  private on = false;
  private ring: HTMLElement;
  private tip: HTMLElement;
  private pet: PetComponent;

  constructor(pet: PetComponent, parent: HTMLElement) {
    this.pet = pet;
    this.ring = document.createElement("div");
    this.ring.className = "look-ring hidden";
    this.ring.innerHTML = `<div class="look-lens"></div><div class="look-glint"></div>`;
    parent.appendChild(this.ring);

    this.tip = document.createElement("div");
    this.tip.className = "look-tip hidden";
    this.tip.textContent = "Click to have me look at code";
    parent.appendChild(this.tip);

    // Warm up the monocle when the mouse is near it.
    pet.element.addEventListener("mousemove", (e) => this.hover(e));
    pet.element.addEventListener("mouseleave", () => this.hover(null));
  }

  public isOn(): boolean {
    return this.on;
  }

  private monocleRect(): DOMRect | null {
    const ring = this.pet.element.querySelector<SVGGraphicsElement>(".monocle-ring");
    return ring ? ring.getBoundingClientRect() : null;
  }

  /** True when a click at these page coordinates landed on his monocle. */
  public isOnMonocle(x: number, y: number): boolean {
    const r = this.monocleRect();
    if (!r || r.width === 0) return false;
    return (
      x >= r.left - HIT_PADDING &&
      x <= r.right + HIT_PADDING &&
      y >= r.top - HIT_PADDING &&
      y <= r.bottom + HIT_PADDING
    );
  }

  private hover(e: MouseEvent | null): void {
    const near = e !== null && this.isOnMonocle(e.clientX, e.clientY);
    this.pet.element.classList.toggle("monocle-hot", near);
    const r = this.monocleRect();
    if (near && r && !this.on && !document.body.classList.contains("dragging")) {
      this.tip.style.left = `${r.left + r.width / 2}px`;
      this.tip.style.top = `${r.top - 10}px`;
      this.tip.classList.remove("hidden");
    } else {
      this.tip.classList.add("hidden");
    }
  }

  /** Ask the backend to start or stop looking. Throws "vscode" when VS Code is not linked. */
  public async toggle(): Promise<void> {
    await invoke("set_look_mode", { on: !this.on });
  }

  /** The backend says look mode switched (the click, a timeout, Alt+Escape, a selection). */
  public setOn(on: boolean, reason?: string): void {
    this.on = on;
    this.pet.element.classList.toggle("looking", on);
    this.tip.classList.add("hidden");
    if (on) {
      this.ring.classList.remove("hidden", "look-snap");
    } else if (reason === "selected") {
      // The lens snaps onto what he saw, then goes.
      this.ring.classList.add("look-snap");
      window.setTimeout(() => this.ring.classList.add("hidden"), 320);
    } else {
      this.ring.classList.add("hidden");
    }
  }

  /** Cursor position in physical pixels of the overlay window. */
  public moveRing(physicalX: number, physicalY: number): void {
    const dpr = window.devicePixelRatio || 1;
    this.ring.style.transform = `translate(${physicalX / dpr - RING / 2}px, ${physicalY / dpr - RING / 2}px)`;
  }
}
