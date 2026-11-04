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
      // Only include elements that are displayed and visible
      if (el.offsetParent === null && el.offsetWidth === 0 && el.offsetHeight === 0) {
        return;
      }
      const style = window.getComputedStyle(el);
      if (style.display === "none" || style.visibility === "hidden" || style.opacity === "0") {
        return;
      }

      const rect = el.getBoundingClientRect();
      if (rect.width > 0 && rect.height > 0) {
        // Add a slight 2px margin to ensure easy hovering/clicking
        const margin = 2;
        const x = Math.max(0, Math.round((rect.left - margin) * dpr));
        const y = Math.max(0, Math.round((rect.top - margin) * dpr));
        const width = Math.round((rect.width + margin * 2) * dpr);
        const height = Math.round((rect.height + margin * 2) * dpr);

        regions.push({ x, y, width, height });
      }
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
