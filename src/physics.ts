import { invoke } from "@tauri-apps/api/core";
import { PetComponent } from "./pet";
import { PlatformRect, PlatformSegment } from "./types";

const GRAVITY = 2600; // px/s^2: realistic drop weight
const BOUNCE = 0.3; // share of speed kept after hitting a platform
const SLIDE_FRICTION = 2400; // px/s^2 slowing him down on a surface
const MAX_SPEED = 2600; // cap on throw speed
const MIN_BOUNCE_SPEED = 220; // slower impacts just settle
const MAX_SECONDS = 4; // safety stop for physics loop
const DAZED_MS = 550; // duration of squashed landed expression
const WINDOW_TRACK_INTERVAL_MS = 80; // poll frequency when standing on a window

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/** Top edge of the pet when his feet stand on the floor (the top of the taskbar, if there is one). */
export function floorTop(petHeight: number): number {
  const taskbar = Math.max(0, window.screen.height - window.screen.availHeight);
  return Math.max(0, window.innerHeight - taskbar - petHeight);
}

/** Fetches exposed top-edge platforms of visible, non-fullscreen OS windows + screen floor. */
export async function fetchPlatforms(petHeight: number): Promise<PlatformSegment[]> {
  const dpr = window.devicePixelRatio || 1;
  const floorY = floorTop(petHeight) + petHeight;
  const floor: PlatformSegment = {
    x1: 0,
    x2: window.innerWidth,
    y: floorY,
    hwnd: 0,
    title: "Floor",
  };

  try {
    const raw = await invoke<PlatformSegment[]>("scan_window_platforms");
    const platforms = raw.map((p) => ({
      x1: p.x1 / dpr,
      x2: p.x2 / dpr,
      y: p.y / dpr,
      hwnd: p.hwnd,
      title: p.title,
    }));
    platforms.push(floor);
    return platforms;
  } catch (e) {
    console.error("scan_window_platforms failed:", e);
    return [floor];
  }
}

/**
 * Gravity and platform collision for Barnaby.
 * Barnaby falls and can land/stand on top of visible desktop windows (excluding fullscreen ones)
 * or the screen floor. While standing on a window, he rides along if the window moves.
 */
export class PetPhysics {
  private raf = 0;
  private dazedTimer = 0;
  private trackTimer = 0;
  private standingHwnd: number | null = null;
  private lastWindowPos: { x: number; y: number } | null = null;
  private currentDone?: (x: number, y: number) => void;

  constructor(
    private pet: PetComponent,
    private onUpdate: (x: number, y: number) => void,
  ) {}

  get active(): boolean {
    return this.raf !== 0;
  }

  get isStandingOnWindow(): boolean {
    return this.standingHwnd !== null && this.standingHwnd !== 0;
  }

  /** Stop all active physics and window tracking (e.g. when grabbed by the user). */
  cancel(): void {
    if (this.raf) {
      cancelAnimationFrame(this.raf);
      this.raf = 0;
    }
    if (this.trackTimer) {
      window.clearInterval(this.trackTimer);
      this.trackTimer = 0;
    }
    window.clearTimeout(this.dazedTimer);
    this.standingHwnd = null;
    this.lastWindowPos = null;
    this.pet.element.classList.remove("falling", "landed");
  }

  /**
   * Release the pet into the air with initial velocity (vx, vy).
   * Falls under gravity, checks for collision with window platforms, bounces and settles.
   */
  async release(vx: number, vy: number, done: (x: number, y: number) => void): Promise<void> {
    this.cancel();
    this.currentDone = done;

    const el = this.pet.element;
    const w = el.offsetWidth || 56;
    const h = el.offsetHeight || 72;
    let x = parseFloat(el.style.left) || 0;
    let y = parseFloat(el.style.top) || 0;

    vx = clamp(vx, -MAX_SPEED, MAX_SPEED);
    vy = clamp(vy, -MAX_SPEED, MAX_SPEED);

    // Fetch window platforms
    const platforms = await fetchPlatforms(h);

    // Check if he is already resting on a platform and wasn't thrown
    const feetX = x + w / 2;
    const feetY = y + h;
    if (Math.abs(vx) < 40 && Math.abs(vy) < 40) {
      const restingPlatform = platforms.find(
        (p) => feetX >= p.x1 - 6 && feetX <= p.x2 + 6 && Math.abs(feetY - p.y) <= 4,
      );
      if (restingPlatform) {
        y = restingPlatform.y - h;
        this.pet.setPosition(x, y);
        this.standingHwnd = restingPlatform.hwnd;
        if (restingPlatform.hwnd !== 0) {
          this.startWindowTracking(restingPlatform.hwnd, w, h);
        }
        done(x, y);
        return;
      }
    }

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
      if (this.standingHwnd && this.standingHwnd !== 0) {
        this.startWindowTracking(this.standingHwnd, w, h);
      }
      done(x, y);
    };

    const tick = (now: number) => {
      const dt = Math.min(0.033, (now - last) / 1000);
      last = now;

      const prevY = y;
      const prevFeetY = prevY + h;

      vy += GRAVITY * dt;
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

      const curFeetX = x + w / 2;
      const newFeetY = y + h;

      // Platform collision detection (find candidate platforms where feet crossed top surface)
      let candidatePlatform: PlatformSegment | null = null;

      for (const p of platforms) {
        // Horizontal span check with small margin
        if (curFeetX >= p.x1 - 6 && curFeetX <= p.x2 + 6) {
          // Downward passage check through platform top surface
          if (prevFeetY <= p.y + 6 && newFeetY >= p.y - 6 && vy >= 0) {
            if (!candidatePlatform || p.y < candidatePlatform.y) {
              candidatePlatform = p;
            }
          }
        }
      }

      if (candidatePlatform) {
        y = candidatePlatform.y - h;
        this.standingHwnd = candidatePlatform.hwnd;

        if (vy > MIN_BOUNCE_SPEED) {
          // Hard landing: bounce and trigger landed animation
          landedOnce = true;
          el.classList.remove("falling");
          el.classList.remove("landed");
          void el.offsetWidth; // re-trigger CSS keyframe
          el.classList.add("landed");
          vy = -vy * BOUNCE;
          vx *= 0.75;
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

      const resting = candidatePlatform && vy === 0 && Math.abs(vx) < 5;
      if (resting || (now - started) / 1000 > MAX_SECONDS) {
        finish();
      } else {
        this.raf = requestAnimationFrame(tick);
      }
    };

    this.raf = requestAnimationFrame(tick);
  }

  /**
   * While standing on a window, poll its position so Barnaby moves with it.
   * If the window moves, Barnaby moves with it; if closed/minimized or walked off, he falls.
   */
  private startWindowTracking(hwnd: number, w: number, h: number): void {
    if (this.trackTimer) window.clearInterval(this.trackTimer);
    const dpr = window.devicePixelRatio || 1;

    this.trackTimer = window.setInterval(async () => {
      if (!this.standingHwnd || this.standingHwnd !== hwnd) {
        window.clearInterval(this.trackTimer);
        this.trackTimer = 0;
        return;
      }

      try {
        const bounds = await invoke<PlatformRect | null>("get_window_bounds", { hwnd });
        if (!bounds) {
          // Window closed or minimized: Barnaby drops!
          window.clearInterval(this.trackTimer);
          this.trackTimer = 0;
          this.standingHwnd = null;
          void this.release(0, 0, this.currentDone || ((_x, _y) => {}));
          return;
        }

        const winLeft = bounds.left / dpr;
        const winTop = bounds.top / dpr;
        const winRight = bounds.right / dpr;

        let curX = parseFloat(this.pet.element.style.left) || 0;
        const curFeetX = curX + w / 2;

        // Check if feet are still on the window's top edge
        if (curFeetX < winLeft - 10 || curFeetX > winRight + 10) {
          // Walked or slid off the window edge: Barnaby drops!
          window.clearInterval(this.trackTimer);
          this.trackTimer = 0;
          this.standingHwnd = null;
          void this.release(0, 0, this.currentDone || ((_x, _y) => {}));
          return;
        }

        if (this.lastWindowPos) {
          const dx = winLeft - this.lastWindowPos.x;
          const dy = winTop - this.lastWindowPos.y;

          if (Math.abs(dx) > 0.5 || Math.abs(dy) > 0.5) {
            curX = Math.max(0, Math.min(window.innerWidth - w, curX + dx));
            const curY = winTop - h;
            this.pet.setPosition(curX, curY);
            this.onUpdate(curX, curY);
          }
        }

        this.lastWindowPos = { x: winLeft, y: winTop };
      } catch {
        // Window check failed: fall back to safe gravity
        window.clearInterval(this.trackTimer);
        this.trackTimer = 0;
        this.standingHwnd = null;
        void this.release(0, 0, this.currentDone || ((_x, _y) => {}));
      }
    }, WINDOW_TRACK_INTERVAL_MS);
  }
}
