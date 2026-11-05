import { invoke } from "@tauri-apps/api/core";
import { PhononStatusInfo, SetupStatus } from "./types";

/**
 * The first-run guide: a card in the conversation that ticks off what Barnaby needs, live.
 * A brain (the API key) and VS Code are needed; the voice and the talk key are nice to have.
 */

const POLL_MS = 1500;

interface Deps {
  /** Both required steps are done. */
  onReady: () => void;
  /** The person chose to leave the guide (finished or skipped). */
  onLeave: () => void;
  /** Open the Settings window (for the voice engine). */
  openSettings: () => void;
}

type MessageKind = "ok" | "error" | "busy";

const row = (id: string, title: string) => `
  <div class="setup-row" data-row="${id}">
    <span class="setup-tick" aria-hidden="true"></span>
    <div class="setup-body">
      <div class="setup-title">${title}</div>
      <div class="setup-detail"></div>
      <div class="setup-action"></div>
      <div class="setup-msg"></div>
    </div>
  </div>`;

export class SetupCard {
  public element: HTMLElement;
  private timer: number | null = null;
  private deps: Deps;
  private mounted = false;
  private notifiedReady = false;
  private sawMissing = false;
  private pickingKey = false;
  private messages: Record<string, { text: string; kind: MessageKind }> = {};

  constructor(deps: Deps) {
    this.deps = deps;
    this.element = document.createElement("div");
    this.element.className = "chat-msg chat-barnaby setup-card";
    this.element.innerHTML = `
      <div class="setup-intro">Good day, I am Sir Barnaby. Two things and I am ready to look at your code:</div>
      ${row("key", "A brain")}
      ${row("vscode", "VS Code")}
      <div class="setup-extras">Nice to have</div>
      ${row("voice", "A voice")}
      ${row("talk", "A talk key")}
      <div class="setup-foot"></div>
    `;
    this.element.addEventListener("click", (e) => this.onClick(e));
    void this.refresh();
    this.timer = window.setInterval(() => void this.refresh(), POLL_MS);
  }

  public destroy(): void {
    if (this.timer !== null) clearInterval(this.timer);
    this.timer = null;
  }

  private say(id: string, text: string, kind: MessageKind = "ok"): void {
    this.messages[id] = { text, kind };
    void this.refresh();
  }

  /** Update one row. The action's HTML is only replaced when it changes, so typing is never lost. */
  private setRow(id: string, done: boolean, detail: string, actionHtml = ""): void {
    const el = this.element.querySelector(`[data-row="${id}"]`) as HTMLElement;
    el.classList.toggle("is-done", done);
    (el.querySelector(".setup-detail") as HTMLElement).textContent = detail;
    const action = el.querySelector(".setup-action") as HTMLElement;
    if (action.dataset.html !== actionHtml) {
      action.dataset.html = actionHtml;
      action.innerHTML = actionHtml;
    }
    const msg = el.querySelector(".setup-msg") as HTMLElement;
    const m = done ? undefined : this.messages[id];
    msg.textContent = m?.text ?? "";
    msg.dataset.kind = m?.kind ?? "ok";
    const busy = m?.kind === "busy";
    el.querySelectorAll<HTMLInputElement | HTMLButtonElement>("input, button").forEach((c) => (c.disabled = busy));
  }

  private async refresh(): Promise<void> {
    if (this.timer === null) return;
    if (this.element.isConnected) this.mounted = true;
    else if (this.mounted) {
      // The conversation was cleared: nothing to keep up to date any more.
      this.destroy();
      return;
    }
    let status: SetupStatus;
    try {
      status = await invoke<SetupStatus>("setup_status");
    } catch (e) {
      console.error("setup_status failed:", e);
      return;
    }
    let voice: PhononStatusInfo | null = null;
    try {
      voice = await invoke<PhononStatusInfo>("get_phonon_status");
    } catch {
      // Shown below as "not running".
    }

    this.setRow(
      "key",
      status.has_key,
      status.has_key
        ? "The key is saved."
        : "Paste your Meta API key (from dev.meta.ai). It stays in Windows Credential Manager.",
      status.has_key
        ? ""
        : `<div class="setup-form"><input type="password" class="setup-input" data-input="key" placeholder="Meta API key" autocomplete="off" spellcheck="false" />
           <button type="button" class="setup-btn" data-act="save-key">Save and test</button></div>`,
    );

    // "Installed" and "talking to me" are different: VS Code only loads a new extension on a reload.
    this.setRow(
      "vscode",
      status.vscode,
      status.vscode
        ? "VS Code is linked."
        : status.extension_installed
          ? "The extension is installed, but VS Code is not talking to me yet. Open VS Code, or reload its window (Ctrl+Shift+P, then Reload Window)."
          : "I look at your code through a small extension in VS Code.",
      status.vscode
        ? ""
        : status.extension_installed
          ? `<button type="button" class="setup-btn quiet" data-act="install-ext">Install it again</button>`
          : `<button type="button" class="setup-btn" data-act="install-ext">Install it in VS Code</button>`,
    );

    const voiceReady = voice?.state === "ready";
    const warming = voice?.state === "starting" || voice?.state === "installing";
    this.setRow(
      "voice",
      voiceReady,
      voiceReady
        ? "My voice is ready."
        : warming
          ? "Warming up my voice. One moment."
          : "My voice is not running. I can still write my answers.",
      voiceReady || warming ? "" : `<button type="button" class="setup-btn quiet" data-act="settings">Open voice settings</button>`,
    );

    if (status.talk_key_ok) {
      this.setRow("talk", true, `Hold ${status.talk_key} and speak, or press the microphone beside the text box.`);
    } else if (this.pickingKey) {
      this.setRow(
        "talk",
        false,
        `${status.talk_key} belongs to another program. Choose a different key.`,
        `<div class="setup-form"><input type="text" class="setup-input" data-input="talk" placeholder="For example Ctrl+Alt+K" spellcheck="false" />
         <button type="button" class="setup-btn" data-act="save-talk">Use this key</button></div>`,
      );
    } else {
      this.setRow(
        "talk",
        false,
        `${status.talk_key} belongs to another program. The microphone beside the text box works meanwhile.`,
        `<button type="button" class="setup-btn quiet" data-act="pick-talk">Pick another key</button>`,
      );
    }

    const ready = status.has_key && status.vscode;
    // Only cheer when something was missing a moment ago; a card opened on a ready setup stays quiet.
    if (!ready) this.sawMissing = true;
    const foot = this.element.querySelector(".setup-foot") as HTMLElement;
    const footHtml = ready
      ? `<div class="setup-done">Splendid. Click my monocle, select some code in VS Code, and I shall take a look.</div>
         <button type="button" class="setup-btn" data-act="leave">Got it</button>`
      : `<button type="button" class="setup-btn quiet" data-act="leave">Skip for now</button>`;
    if (foot.dataset.html !== footHtml) {
      foot.dataset.html = footHtml;
      foot.innerHTML = footHtml;
    }
    if (ready && this.sawMissing && !this.notifiedReady) {
      this.notifiedReady = true;
      this.deps.onReady();
    }
  }

  private onClick(e: MouseEvent): void {
    const act = (e.target as HTMLElement).closest<HTMLElement>("[data-act]")?.dataset.act;
    switch (act) {
      case "save-key":
        void this.saveKey();
        break;
      case "install-ext":
        void this.installExtension();
        break;
      case "settings":
        this.deps.openSettings();
        break;
      case "pick-talk":
        this.pickingKey = true;
        void this.refresh();
        break;
      case "save-talk":
        void this.saveTalkKey();
        break;
      case "leave":
        this.destroy();
        this.deps.onLeave();
        break;
    }
  }

  private async saveKey(): Promise<void> {
    const key = this.element.querySelector<HTMLInputElement>('[data-input="key"]')?.value.trim() ?? "";
    if (!key) {
      this.say("key", "Paste the key first.", "error");
      return;
    }
    this.say("key", "Testing the key…", "busy");
    try {
      await invoke("save_api_key", { key });
      await invoke<string>("test_api_key");
      delete this.messages.key;
    } catch (e) {
      this.messages.key = { text: String(e), kind: "error" };
    }
    void this.refresh();
  }

  private async installExtension(): Promise<void> {
    this.say("vscode", "Installing…", "busy");
    try {
      this.say("vscode", await invoke<string>("install_vscode_extension"));
    } catch (e) {
      this.say("vscode", String(e), "error");
    }
  }

  private async saveTalkKey(): Promise<void> {
    const shortcut = this.element.querySelector<HTMLInputElement>('[data-input="talk"]')?.value ?? "";
    try {
      await invoke("set_push_to_talk", { shortcut });
      delete this.messages.talk;
      this.pickingKey = false;
      void this.refresh();
    } catch (e) {
      this.say("talk", String(e), "error");
    }
  }
}
