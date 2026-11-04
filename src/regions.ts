import { invoke } from "@tauri-apps/api/core";
import { ClickRegion } from "./types";

let syncPending = false;

export function updateClickRegions(): void {
  if (syncPending) return;
  syncPending = true;

  requestAnimationFrame(() => {
    syncPending = false;
    const elements = document.querySelectorAll<HTMLElement>(".interactive");
    const dpr = window.devicePixelRatio || 1;
    const regions: ClickRegion[] = [];

    elements.forEach((el) => {
      // Skip elements that are explicitly marked as hidden
      if (el.classList.contains("hidden")) return;

      const rect = el.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return;

      const style = window.getComputedStyle(el);
      if (style.display === "none" || style.visibility === "hidden" || style.opacity === "0") {
        return;
      }

      // Add a 4px margin to ensure comfortable hovering and clicking
      const margin = 4;
      const x = Math.round((rect.left - margin) * dpr);
      const y = Math.round((rect.top - margin) * dpr);
      const width = Math.round((rect.width + margin * 2) * dpr);
      const height = Math.round((rect.height + margin * 2) * dpr);

      regions.push({ x, y, width, height });
    });

    invoke("set_click_regions", { regions }).catch((err) => {
      console.error("Failed to set click regions:", err);
    });
  });
}

export function initRegionWatcher(): void {
  const observer = new MutationObserver(() => {
    updateClickRegions();
  });

  observer.observe(document.body, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["style", "class", "hidden"],
  });

  window.addEventListener("resize", () => updateClickRegions());
  window.addEventListener("scroll", () => updateClickRegions(), true);
  updateClickRegions();
}
