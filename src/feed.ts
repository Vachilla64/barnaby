import { FeedItem } from "./types";
import { updateClickRegions } from "./regions";

export class ActivityFeedManager {
  public element: HTMLElement;
  private hoverTimer: number | null = null;
  private feed: FeedItem[] = [];

  constructor() {
    this.element = document.createElement("div");
    this.element.className = "activity-feed interactive hidden";
    this.element.innerHTML = `
      <div class="feed-header">Recent Activity</div>
      <div class="feed-list"></div>
    `;

    this.element.addEventListener("mousedown", (e) => e.stopPropagation());
    this.element.addEventListener("click", (e) => e.stopPropagation());
  }

  public addItem(text: string): void {
    const now = new Date();
    const time = `${now.getHours().toString().padStart(2, "0")}:${now
      .getMinutes()
      .toString()
      .padStart(2, "0")}:${now.getSeconds().toString().padStart(2, "0")}`;
    this.feed.unshift({ time, text });
    if (this.feed.length > 20) {
      this.feed.pop();
    }
    this.render();
  }

  public getItems(): FeedItem[] {
    return this.feed;
  }

  private render(): void {
    const list = this.element.querySelector(".feed-list") as HTMLElement;
    const last5 = this.feed.slice(0, 5);
    if (last5.length === 0) {
      list.innerHTML = `<div class="feed-empty">No activity recorded yet</div>`;
      return;
    }

    list.innerHTML = last5
      .map(
        (item) => `
        <div class="feed-item">
          <span class="feed-time">${item.time}</span>
          <span class="feed-text">${item.text}</span>
        </div>
      `
      )
      .join("");
  }

  public onPetMouseEnter(x: number, y: number): void {
    if (this.hoverTimer) clearTimeout(this.hoverTimer);
    // Show after 1000ms hover delay
    this.hoverTimer = window.setTimeout(() => {
      this.render();
      this.element.style.left = `${x}px`;
      this.element.style.top = `${y}px`;
      this.element.classList.remove("hidden");
      updateClickRegions();
    }, 1000);
  }

  public onPetMouseLeave(): void {
    if (this.hoverTimer) {
      clearTimeout(this.hoverTimer);
      this.hoverTimer = null;
    }
    this.element.classList.add("hidden");
    updateClickRegions();
  }
}
