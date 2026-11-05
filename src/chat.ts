import { updateClickRegions } from "./regions";

/**
 * The conversation: a list of messages that stands beside Barnaby and runs up toward the top of the
 * screen. Questions (typed, spoken or from a button) and his answers all land here, with buttons
 * under his latest message for the likely next question.
 */

export interface ChatChip {
  label: string;
  onClick: () => void;
}

export interface ChatCallbacks {
  onSend?: (text: string) => void;
  onMic?: () => void;
  onNewChat?: () => void;
  onSetup?: () => void;
  onStop?: () => void;
}

const PANEL_W = 360;
/** Space between him and the list. */
const GAP = 14;
const EDGE = 8;
const MAX_MESSAGES = 60;
/** The list tucks itself away after this long without anything happening. */
const IDLE_CLOSE_MS = 45000;

const ICONS = {
  newChat: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`,
  help: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M9.5 9.5a2.6 2.6 0 1 1 3.7 2.4c-.8.4-1.2.9-1.2 1.8"/><circle cx="12" cy="17" r=".6"/></svg>`,
  close: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg>`,
  mic: `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/></svg>`,
  send: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"/></svg>`,
  look: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="6"/><path d="m16 16 4 4"/></svg>`,
};

export class ChatPanel {
  public element: HTMLElement;
  private list: HTMLElement;
  private input: HTMLInputElement;
  private micBtn: HTMLButtonElement;
  private thinkingEl: HTMLElement | null = null;
  private chipsEl: HTMLElement | null = null;
  private idleTimer: number | null = null;
  private hovered = false;
  private listening = false;
  /** True while something the person is working through (the setup card) is on the list. */
  private pinned = false;
  private callbacks: ChatCallbacks = {};

  constructor() {
    this.element = document.createElement("div");
    this.element.className = "chat-panel interactive hidden";
    this.element.innerHTML = `
      <div class="chat-head">
        <span class="chat-title">Sir Barnaby</span>
        <button type="button" class="chat-stop hidden" data-act="stop" title="Stop talking" aria-label="Stop talking">Stop talking</button>
        <button type="button" class="chat-icon-btn" data-act="new" title="Start a new conversation" aria-label="Start a new conversation">${ICONS.newChat}</button>
        <button type="button" class="chat-icon-btn" data-act="setup" title="Setup and tips" aria-label="Setup and tips">${ICONS.help}</button>
        <button type="button" class="chat-icon-btn" data-act="close" title="Tuck the conversation away" aria-label="Tuck the conversation away">${ICONS.close}</button>
      </div>
      <div class="chat-list"></div>
      <div class="chat-compose">
        <input type="text" class="chat-input" placeholder="Ask about your code…" spellcheck="false" />
        <button type="button" class="chat-mic" title="Talk to me" aria-label="Talk to me">${ICONS.mic}</button>
        <button type="button" class="chat-send" title="Send" aria-label="Send">${ICONS.send}</button>
      </div>
    `;
    this.list = this.element.querySelector(".chat-list") as HTMLElement;
    this.input = this.element.querySelector(".chat-input") as HTMLInputElement;
    this.micBtn = this.element.querySelector(".chat-mic") as HTMLButtonElement;
    this.setupEvents();
  }

  public setCallbacks(callbacks: ChatCallbacks): void {
    this.callbacks = callbacks;
  }

  private setupEvents(): void {
    // Clicking in the list must never start dragging the pet or close the menu.
    this.element.addEventListener("mousedown", (e) => e.stopPropagation());
    this.element.addEventListener("click", (e) => e.stopPropagation());
    this.element.addEventListener("mouseenter", () => {
      this.hovered = true;
    });
    this.element.addEventListener("mouseleave", () => {
      this.hovered = false;
      this.touch();
    });

    this.element.querySelector(".chat-head")!.addEventListener("click", (e) => {
      const act = (e.target as HTMLElement).closest<HTMLElement>("[data-act]")?.dataset.act;
      if (act === "new") this.callbacks.onNewChat?.();
      else if (act === "setup") this.callbacks.onSetup?.();
      else if (act === "stop") this.callbacks.onStop?.();
      else if (act === "close") this.close();
    });

    const send = () => {
      const text = this.input.value.trim();
      if (!text) return;
      this.input.value = "";
      this.callbacks.onSend?.(text);
    };
    this.element.querySelector(".chat-send")!.addEventListener("click", send);
    this.micBtn.addEventListener("click", () => this.callbacks.onMic?.());
    this.input.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Enter") send();
      else if (e.key === "Escape") this.input.blur();
      this.touch();
    });
  }

  // ----- open and close -----

  public isOpen(): boolean {
    return !this.element.classList.contains("hidden");
  }

  public open(focusInput = false): void {
    if (!this.isOpen()) {
      this.element.classList.remove("hidden");
      this.element.classList.remove("chat-opening");
      void this.element.offsetWidth;
      this.element.classList.add("chat-opening");
      this.scrollToEnd();
    }
    this.touch();
    updateClickRegions();
    if (focusInput) setTimeout(() => this.input.focus(), 60);
  }

  public close(): void {
    this.element.classList.add("hidden");
    this.input.blur();
    this.clearIdleTimer();
    updateClickRegions();
  }

  public toggle(): void {
    if (this.isOpen()) this.close();
    else this.open(true);
  }

  /** Anything happening in the conversation resets the countdown to tucking itself away. */
  public touch(): void {
    this.clearIdleTimer();
    this.idleTimer = window.setTimeout(() => {
      const busy =
        this.hovered || this.pinned || this.thinkingEl || this.listening || this.input.value || document.activeElement === this.input;
      if (busy) this.touch();
      else this.close();
    }, IDLE_CLOSE_MS);
  }

  private clearIdleTimer(): void {
    if (this.idleTimer !== null) {
      clearTimeout(this.idleTimer);
      this.idleTimer = null;
    }
  }

  // ----- placement -----

  /** Stand beside him, on the side that has more room, with the bottom on his feet. */
  public reposition(petX: number, petY: number, petW: number, petH: number): void {
    const w = Math.min(PANEL_W, window.innerWidth - 2 * EDGE);
    const centre = petX + petW / 2;
    const onLeft = centre > window.innerWidth / 2;
    const wanted = onLeft ? petX - GAP - w : petX + petW + GAP;
    const left = Math.max(EDGE, Math.min(wanted, window.innerWidth - w - EDGE));
    const feet = petY + petH;
    this.element.style.width = `${w}px`;
    this.element.style.left = `${left}px`;
    this.element.style.bottom = `${Math.max(EDGE, window.innerHeight - feet)}px`;
    this.element.style.maxHeight = `${Math.max(180, feet - EDGE)}px`;
    this.element.dataset.side = onLeft ? "left" : "right";
  }

  // ----- messages -----

  private append(el: HTMLElement): void {
    this.clearChips();
    this.list.appendChild(el);
    // He is working on something: keep that line last.
    if (this.thinkingEl) this.list.appendChild(this.thinkingEl);
    while (this.list.children.length > MAX_MESSAGES) this.list.firstElementChild?.remove();
    this.scrollToEnd();
    this.touch();
  }

  private scrollToEnd(): void {
    requestAnimationFrame(() => {
      this.list.scrollTop = this.list.scrollHeight;
      updateClickRegions();
    });
  }

  private clearChips(): void {
    this.chipsEl?.remove();
    this.chipsEl = null;
  }

  private message(kind: string, text: string): HTMLElement {
    const el = document.createElement("div");
    el.className = `chat-msg chat-${kind} chat-in`;
    el.textContent = text;
    return el;
  }

  public addUser(text: string): void {
    this.append(this.message("user", text));
  }

  /** His words, and when he has a walk to offer, the map of its stops under them. */
  public addBarnaby(text: string, chips: ChatChip[] = [], agenda: string[] = []): void {
    const msg = this.message("barnaby", text);
    if (agenda.length > 0) {
      const label = document.createElement("div");
      label.className = "chat-agenda-label";
      label.textContent = "I will show you";
      const list = document.createElement("ol");
      list.className = "chat-agenda";
      for (const stop of agenda) {
        const li = document.createElement("li");
        li.textContent = stop;
        list.appendChild(li);
      }
      msg.append(label, list);
    }
    this.append(msg);
    if (chips.length > 0) this.setChips(chips);
  }

  /** During a walk the list shrinks to the tour card, so it covers as little of the code as it can. */
  public setFocus(on: boolean): void {
    this.element.classList.toggle("is-touring", on);
    this.scrollToEnd();
  }

  public addError(text: string): void {
    this.append(this.message("error", text));
  }

  /** A small caption between messages, for example which lines he is looking at. */
  public addNote(text: string): void {
    const note = this.message("note", "");
    note.innerHTML = `<span class="chat-note-icon">${ICONS.look}</span>`;
    note.append(document.createTextNode(text));
    this.append(note);
  }

  /** Something richer than text, such as the setup card. */
  public addElement(el: HTMLElement, pinned = false): void {
    el.classList.add("chat-in");
    this.append(el);
    if (pinned) this.pinned = true;
  }

  public unpin(): void {
    this.pinned = false;
  }

  public setChips(chips: ChatChip[]): void {
    this.clearChips();
    const row = document.createElement("div");
    row.className = "chat-chips chat-in";
    for (const chip of chips) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "chat-chip";
      b.textContent = chip.label;
      b.addEventListener("click", () => {
        this.clearChips();
        chip.onClick();
      });
      row.appendChild(b);
    }
    this.chipsEl = row;
    if (this.thinkingEl) this.list.insertBefore(row, this.thinkingEl);
    else this.list.appendChild(row);
    this.scrollToEnd();
  }

  /** Three dots at the end of the list while he works on an answer. */
  public setThinking(on: boolean): void {
    if (on && !this.thinkingEl) {
      this.clearChips();
      const el = document.createElement("div");
      el.className = "chat-msg chat-barnaby chat-thinking chat-in";
      el.innerHTML = "<span></span><span></span><span></span>";
      this.thinkingEl = el;
      this.list.appendChild(el);
      this.scrollToEnd();
    } else if (!on && this.thinkingEl) {
      this.thinkingEl.remove();
      this.thinkingEl = null;
    }
    this.touch();
  }

  /** The microphone is open: say so on the button and in the input. */
  public setListening(on: boolean): void {
    this.listening = on;
    this.micBtn.classList.toggle("is-listening", on);
    this.input.placeholder = on ? "Listening…" : "Ask about your code…";
    this.touch();
  }

  /** While he is speaking an answer that is on the list, offer a way to cut him off. */
  public setSpeaking(on: boolean): void {
    this.element.querySelector(".chat-stop")!.classList.toggle("hidden", !on);
    if (on) this.touch();
  }

  public clear(): void {
    this.list.innerHTML = "";
    this.thinkingEl = null;
    this.chipsEl = null;
    this.pinned = false;
    this.element.classList.remove("is-touring");
    updateClickRegions();
  }

  public focusInput(): void {
    this.input.focus();
  }
}
