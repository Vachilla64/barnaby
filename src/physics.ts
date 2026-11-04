import { PetComponent } from "./pet";

const GRAVITY = 2600; // px/s^2: a bit more than real, so he drops with some weight
const BOUNCE = 0.3; // share of the speed kept after hitting the floor
const SLIDE_FRICTION = 2400; // px/s^2 slowing him down on the floor
const MAX_SPEED = 2600; // cap on how hard he can be thrown
const MIN_BOUNCE_SPEED = 220; // slower impacts just stop
const MAX_SECONDS = 4; // safety stop for the loop
const DAZED_MS = 550;

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/** Top edge of the pet when his feet stand on the floor (the top of the taskbar, if there is one). */
export function floorTop(petHeight: number): number {
  const taskbar = Math.max(0, window.screen.height - window.screen.availHeight);
  return Math.max(0, window.innerHeight - taskbar - petHeight);
}

/**
 * Gravity for the pet. After he is let go he falls to the floor, bounces a little and slides to a
 * stop. A throw keeps the speed of the mouse. CSS classes show how he feels:
 * `falling` (scared, flailing) and `landed` (squashed and dazed).
 */
export class PetPhysics {
  private raf = 0;
  private dazedTimer = 0;

  constructor(
    private pet: PetComponent,
    private onUpdate: (x: number, y: number) => void,
  ) {}

  get active(): boolean {
    return this.raf !== 0;
  }

  /** Stop at once, for example when the user catches him in mid-air. */
  cancel(): void {
    cancelAnimationFrame(this.raf);
    this.raf = 0;
    window.clearTimeout(this.dazedTimer);
    this.pet.element.classList.remove("falling", "landed");
  }

  release(vx: number, vy: number, done: (x: number, y: number) => void): void {
    const el = this.pet.element;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    let x = parseFloat(el.style.left) || 0;
    let y = parseFloat(el.style.top) || 0;
    vx = clamp(vx, -MAX_SPEED, MAX_SPEED);
    vy = clamp(vy, -MAX_SPEED, MAX_SPEED);

    // Already standing and not thrown: nothing to do.
    if (y >= floorTop(h) - 1 && Math.abs(vx) < 40 && vy > -40) {
      y = floorTop(h);
      this.pet.setPosition(x, y);
      done(x, y);
      return;
    }

    this.cancel();
    el.classList.add("falling");
    let last = performance.now();
    const started = last;
    let landedOnce = false;

    const finish = () => {
      this.raf = 0;
      el.classList.remove("falling");
      if (landedOnce) {
        window.clearTimeout(this.dazedTimer);
        this.dazedTimer = window.setTimeout(() => el.classList.remove("landed"), DAZED_MS);
      }
      done(x, y);
    };

    const tick = (now: number) => {
      const dt = Math.min(0.033, (now - last) / 1000);
      last = now;
      const floor = floorTop(h);
      const standing = y >= floor && vy === 0;

      if (!standing) vy += GRAVITY * dt;
      x += vx * dt;
      y += vy * dt;

      const maxX = Math.max(0, window.innerWidth - w);
      if (x < 0) {
        x = 0;
        vx = Math.abs(vx) * 0.5;
      } else if (x > maxX) {
        x = maxX;
        vx = -Math.abs(vx) * 0.5;
      }
      if (y < 0) {
        y = 0;
        vy = Math.abs(vy) * 0.3;
      }

      if (y >= floor) {
        y = floor;
        if (vy > MIN_BOUNCE_SPEED) {
          // Hard landing: squash, daze and bounce.
          landedOnce = true;
          el.classList.remove("falling");
          el.classList.remove("landed");
          void el.offsetWidth; // restart the squash animation on every bounce
          el.classList.add("landed");
          vy = -vy * BOUNCE;
          vx *= 0.8;
        } else {
          vy = 0;
        }
        if (vy === 0 && vx !== 0) {
          const sign = Math.sign(vx);
          vx -= sign * SLIDE_FRICTION * dt;
          if (Math.sign(vx) !== sign) vx = 0;
        }
      }

      this.pet.setPosition(x, y);
      this.onUpdate(x, y);

      const resting = y >= floor && vy === 0 && vx === 0;
      if (resting || (now - started) / 1000 > MAX_SECONDS) {
        finish();
      } else {
        this.raf = requestAnimationFrame(tick);
      }
    };

    this.raf = requestAnimationFrame(tick);
  }
}
