import { invoke } from "@tauri-apps/api/core";
import { PetComponent } from "./pet";
import { updateClickRegions } from "./regions";

export class PetDragManager {
  private pet: PetComponent;
  private isDragging = false;
  private startMouseX = 0;
  private startMouseY = 0;
  private startPetX = 0;
  private startPetY = 0;
  private onMoveCallback?: (x: number, y: number) => void;
  private onClickCallback?: (e: MouseEvent) => void;

  constructor(pet: PetComponent) {
    this.pet = pet;
    this.setup();
  }

  public onMove(cb: (x: number, y: number) => void): void {
    this.onMoveCallback = cb;
  }

  public onClick(cb: (e: MouseEvent) => void): void {
    this.onClickCallback = cb;
  }

  private setup(): void {
    const el = this.pet.element;

    el.addEventListener("mousedown", (e) => {
      // Only drag on left mouse button
      if (e.button !== 0) return;

      this.isDragging = false;
      this.startMouseX = e.clientX;
      this.startMouseY = e.clientY;

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
        }

        if (this.isDragging) {
          const newX = Math.max(0, Math.min(window.innerWidth - 80, this.startPetX + dx));
          const newY = Math.max(0, Math.min(window.innerHeight - 80, this.startPetY + dy));

          this.pet.setPosition(newX, newY);
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

          const pos = this.pet.getPosition();
          invoke("save_pet_position", {
            agentKind: this.pet.kind,
            x: Math.round(pos.x),
            y: Math.round(pos.y),
          }).catch(console.error);

          updateClickRegions();
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
