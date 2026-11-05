import { TourPace } from "./types";

/**
 * The tour card: one small card in the conversation that shows the plan (the stops, in order),
 * where he is in it, and Back / Next / Stop. What he says is spoken, not written out again; the
 * words are tucked behind "The words" unless his voice is off.
 */

export interface TourStepInfo {
  index: number;
  total: number;
  title: string;
  narration: string;
  path: string;
  start_line: number;
  end_line: number;
  /** VS Code is following along (so it shows where to look). */
  vscode: boolean;
}

const el = <K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text = ""): HTMLElementTagNameMap[K] => {
  const e = document.createElement(tag);
  e.className = className;
  if (text) e.textContent = text;
  return e;
};

export class TourCard {
  public element: HTMLElement;
  private count: HTMLElement;
  private items: HTMLElement[];
  private where: HTMLElement;
  private words: HTMLDetailsElement;
  private text: HTMLElement;
  private hint: HTMLElement;
  private actions: HTMLElement;
  private back: HTMLButtonElement;
  private next: HTMLButtonElement;
  private total: number;

  constructor(labels: string[], private pace: TourPace, control: (action: "back" | "next" | "stop") => void) {
    this.total = labels.length;
    this.element = el("div", "chat-msg chat-barnaby tour-card");

    const head = el("div", "tour-head");
    head.append(el("span", "tour-name", "Our walk"));
    this.count = el("span", "tour-count", `${labels.length} stops`);
    head.append(this.count);

    const list = el("ol", "tour-list");
    this.items = labels.map((label) => {
      const li = el("li", "tour-item");
      li.append(el("span", "tour-dot"), el("span", "tour-label", label));
      list.append(li);
      return li;
    });

    this.where = el("div", "tour-where");
    this.words = el("details", "tour-words");
    this.words.append(el("summary", "", "The words"));
    this.text = el("div", "tour-text");
    this.words.append(this.text);
    this.hint = el("div", "tour-hint");

    this.actions = el("div", "tour-actions");
    this.back = el("button", "tour-btn", "Back");
    this.next = el("button", "tour-btn tour-btn-main", "Next");
    const stop = el("button", "tour-btn", "Stop");
    for (const b of [this.back, this.next, stop]) b.type = "button";
    this.back.addEventListener("click", () => control("back"));
    this.next.addEventListener("click", () => control("next"));
    stop.addEventListener("click", () => control("stop"));
    this.actions.append(this.back, this.next, stop);

    this.element.append(head, list, this.where, this.words, this.hint, this.actions);
  }

  /** Show the words by default (his voice is off) or keep them tucked away. */
  public setShowWords(show: boolean): void {
    this.words.open = show;
  }

  public setStep(s: TourStepInfo): void {
    this.items.forEach((item, i) => {
      item.classList.toggle("is-done", i < s.index);
      item.classList.toggle("is-now", i === s.index);
    });
    this.count.textContent = `${s.index + 1} of ${this.total}`;
    this.text.textContent = s.narration;
    // When VS Code is not following along, the card says where to look.
    const shortPath = s.path.split(/[\\/]/).slice(-2).join("/");
    const lines = s.start_line === s.end_line ? `line ${s.start_line}` : `lines ${s.start_line} to ${s.end_line}`;
    this.where.textContent = s.vscode ? "" : `${shortPath}, ${lines}`;
    this.hint.textContent = "";
    this.back.disabled = s.index === 0;
    this.next.textContent = s.index + 1 >= this.total ? "Finish" : "Next";
    this.next.classList.remove("is-cue");
  }

  /** He has finished speaking and waits for you. */
  public setWaiting(): void {
    if (this.pace !== "wait") return;
    this.hint.textContent = "Press Next when you are ready, or say “next”.";
    this.next.classList.add("is-cue");
  }

  /** The walk is over: keep the plan as a record, drop the controls. */
  public finish(finished: boolean): void {
    this.items.forEach((item) => {
      item.classList.remove("is-now");
      item.classList.toggle("is-done", finished);
    });
    this.count.textContent = finished ? "Done" : "Stopped";
    this.where.remove();
    this.words.remove();
    this.hint.remove();
    this.actions.remove();
  }
}
