import { invoke } from "@tauri-apps/api/core";
import { PetComponent } from "./pet";
import { PetPhysics, floorTop } from "./physics";
import { updateClickRegions } from "./regions";

/** How far back (ms) the mouse path counts when working out the throw speed. */
const THROW_WINDOW_MS = 100;

interface Sample {
  t: number;
  x: number;
  y: number;
}

export class PetDragManager {
  private pet: PetComponent;
  private physics: PetPhysics;
  private isDragging = false;
  private startMouseX = 0;
  private startMouseY = 0;
  private startPetX = 0;
  private startPetY = 0;
  private samples: Sample[] = [];
  private onMoveCallback?: (x: number, y: number) => void;
  private onClickCallback?: (e: MouseEvent) => void;

  constructor(pet: PetComponent) {
    this.pet = pet;
    this.physics = new PetPhysics(pet, (x, y) => this.onMoveCallback?.(x, y));
    this.setup();
  }

  public onMove(cb: (x: number, y: number) => void): void {
    this.onMoveCallback = cb;
  }

  public onClick(cb: (e: MouseEvent) => void): void {
    this.onClickCallback = cb;
  }

  /** Speed of the pet at the moment of release, from the last 100 ms of movement. */
  private throwVelocity(): { vx: number; vy: number } {
    const now = performance.now();
    const recent = this.samples.filter((s) => now - s.t <= THROW_WINDOW_MS);
    if (recent.length < 2) return { vx: 0, vy: 0 };
    const first = recent[0];
    const last = recent[recent.length - 1];
    const seconds = (last.t - first.t) / 1000;
    if (seconds <= 0) return { vx: 0, vy: 0 };
    return { vx: (last.x - first.x) / seconds, vy: (last.y - first.y) / seconds };
  }

  private setup(): void {
    const el = this.pet.element;

    el.addEventListener("mousedown", (e) => {
      // Only drag on left mouse button
      if (e.button !== 0) return;

      // Caught in mid-air: stop falling and hold him where he is.
      this.physics.cancel();

      this.isDragging = false;
      this.startMouseX = e.clientX;
      this.startMouseY = e.clientY;
      this.samples = [];

      const rect = el.getBoundingClientRect();
      this.startPetX = rect.left;
      this.startPetY = rect.top;

      // Ensure window stays interactive while mouse is held down on the pet
      invoke("set_drag_state", { isDragging: true }).catch(console.error);

      const onMouseMove = (moveEvent: MouseEvent) => {
        const dx = moveEvent.clientX - this.startMouseX;
        const dy = moveEvent.clientY - this.startMouseY;

        if (!this.isDragging && Math.hypot(dx, dy) > 4) {
          this.isDragging = true;
          document.body.classList.add("dragging");
          el.classList.add("grabbed");
        }

        if (this.isDragging) {
          const w = el.offsetWidth;
          const h = el.offsetHeight;
          const newX = Math.max(0, Math.min(window.innerWidth - w, this.startPetX + dx));
          const newY = Math.max(0, Math.min(floorTop(h), this.startPetY + dy));

          this.pet.setPosition(newX, newY);
          this.samples.push({ t: performance.now(), x: newX, y: newY });
          if (this.samples.length > 12) this.samples.shift();
          if (this.onMoveCallback) {
            this.onMoveCallback(newX, newY);
          }
        }
      };

      const onMouseUp = (upEvent: MouseEvent) => {
        window.removeEventListener("mousemove", onMouseMove);
        window.removeEventListener("mouseup", onMouseUp);
        document.body.classList.remove("dragging");

        // Always release drag lock on mouseup
        invoke("set_drag_state", { isDragging: false }).catch(console.error);

        if (this.isDragging) {
          this.isDragging = false;
          el.classList.remove("grabbed");

          // Let go: gravity takes him to the floor, keeping the speed of the throw.
          const { vx, vy } = this.throwVelocity();
          this.physics.release(vx, vy, (x, y) => {
            invoke("save_pet_position", {
              agentKind: this.pet.kind,
              x: Math.round(x),
              y: Math.round(y),
            }).catch(console.error);
            updateClickRegions();
          });
        } else {
          // Treated as click
          if (this.onClickCallback) {
            this.onClickCallback(upEvent);
          }
        }
      };

      window.addEventListener("mousemove", onMouseMove);
      window.addEventListener("mouseup", onMouseUp);
    });
  }
}
