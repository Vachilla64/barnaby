import { invoke } from "@tauri-apps/api/core";
import { AppConfig, PhononStatusInfo } from "./types";

export function initSettingsView(container: HTMLElement): void {
  container.innerHTML = `
    <div class="settings-container">
      <header class="settings-header">
        <h2>Barnaby Settings</h2>
        <span class="settings-badge">v0.1.0</span>
      </header>

      <div class="settings-body">
        <!-- Brain: Meta API key -->
        <section class="settings-card">
          <label class="card-title">Meta Model API Key</label>
          <p class="card-desc">Barnaby's brain (<code>muse-spark-1.3</code>) needs a Meta Model API key to answer questions about your code. It is stored in Windows Credential Manager. Test checks the key you pasted, or the saved one if the box is empty.</p>
          <div style="display: flex; gap: 8px; align-items: center;">
            <input type="password" id="api-key-input" class="select-voice" style="flex: 1;" placeholder="Paste your Meta API key" />
            <button id="api-key-save" class="btn btn-primary btn-sm">Save</button>
            <button id="api-key-test" class="btn btn-secondary btn-sm">Test</button>
          </div>
          <span id="api-key-msg" class="status-msg"></span>
        </section>

        <!-- 2. Voice & TTS (Phonon) -->
        <section class="settings-card">
          <label class="card-title">Voice & TTS (Phonon)</label>
          <p class="card-desc">Local streaming neural speech engine listening on <code>127.0.0.1:8001</code>.</p>
          
          <div class="form-row">
            <span>Engine Status:</span>
            <span id="phonon-status" class="badge-status">Checking...</span>
          </div>
          <p id="phonon-msg" class="status-msg" style="margin: 4px 0 10px 0; display: block;"></p>
          
          <div id="phonon-actions" class="phonon-actions-row"></div>

          <div class="form-row" style="margin-top: 14px;">
            <label for="volume-slider">Volume:</label>
            <input type="range" id="volume-slider" min="0" max="1" step="0.05" value="0.9" />
          </div>

          <div class="form-row">
            <label for="mute-all-toggle">Mute all voice:</label>
            <input type="checkbox" id="mute-all-toggle" />
          </div>

          <div style="margin-top: 14px;">
            <label class="card-desc" style="font-weight: 500; color: #fff;">Voice per agent kind:</label>
            <div class="voice-agents-grid">
              <div class="voice-agent-item">
                <span>Claude Code</span>
                <select id="voice-claude" class="select-voice">
                  <option value="Sir Bramwell">Sir Bramwell (pompous English knight)</option>
                  <option value="Master Aldric">Master Aldric (old British wizard)</option>
                  <option value="Hugh">Hugh</option>
                  <option value="Teddy">Teddy</option>
                  <option value="Freya">Freya</option>
                  <option value="Toby">Toby (British)</option>
                  <option value="Easton">Easton</option>
                  <option value="Alfie">Alfie (British)</option>
                </select>
              </div>
              <div class="voice-agent-item">
                <span>Gemini CLI</span>
                <select id="voice-gemini" class="select-voice">
                  <option value="Sir Bramwell">Sir Bramwell (pompous English knight)</option>
                  <option value="Master Aldric">Master Aldric (old British wizard)</option>
                  <option value="Hugh">Hugh</option>
                  <option value="Teddy">Teddy</option>
                  <option value="Easton">Easton</option>
                  <option value="Freya">Freya</option>
                  <option value="Toby">Toby (British)</option>
                  <option value="Alfie">Alfie (British)</option>
                </select>
              </div>
              <div class="voice-agent-item">
                <span>Barnaby</span>
                <select id="voice-barnaby" class="select-voice">
                  <option value="Sir Bramwell">Sir Bramwell (pompous English knight)</option>
                  <option value="Master Aldric">Master Aldric (old British wizard)</option>
                  <option value="Hugh">Hugh</option>
                  <option value="Teddy">Teddy</option>
                  <option value="Toby">Toby (British)</option>
                  <option value="Alfie">Alfie (British)</option>
                  <option value="Freya">Freya</option>
                  <option value="Easton">Easton</option>
                </select>
              </div>
              <div class="voice-agent-item">
                <span>Other Agents</span>
                <select id="voice-other" class="select-voice">
                  <option value="Sir Bramwell">Sir Bramwell (pompous English knight)</option>
                  <option value="Master Aldric">Master Aldric (old British wizard)</option>
                  <option value="Hugh">Hugh</option>
                  <option value="Teddy">Teddy</option>
                  <option value="Toby">Toby (British)</option>
                  <option value="Alfie">Alfie (British)</option>
                  <option value="Freya">Freya</option>
                  <option value="Easton">Easton</option>
                </select>
              </div>
            </div>
          </div>
        </section>

        <!-- 3. Push to Talk -->
        <section class="settings-card">
          <label class="card-title">Push-to-Talk</label>
          <p class="card-desc">Hold this key and speak. If another program already uses it, pick a different one. The microphone beside the chat box works either way.</p>
          <div class="form-row">
            <input type="text" id="ptt-key" class="select-voice" spellcheck="false" />
            <button type="button" id="ptt-save" class="btn btn-secondary btn-sm">Use this key</button>
          </div>
          <span id="ptt-msg" class="status-msg"></span>
        </section>

        <!-- Look mode -->
        <section class="settings-card">
          <label class="card-title">Look mode</label>
          <p class="card-desc">Click Barnaby's monocle, then select code in VS Code. This is what he does once your selection settles.</p>
          <div class="form-row">
            <label for="look-after">After I select code:</label>
            <select id="look-after" class="select-voice">
              <option value="choices">Offer choices (Explain it, Why is it here, ...)</option>
              <option value="explain">Explain it right away</option>
              <option value="listen">Listen for my voice</option>
            </select>
          </div>
          <span id="look-after-msg" class="status-msg"></span>
        </section>

        <!-- Tours -->
        <section class="settings-card">
          <label class="card-title">Tours</label>
          <p class="card-desc">A tour is a short walk through a few places in your code. Barnaby tells you the plan first, then shows one stop at a time.</p>
          <div class="form-row">
            <label for="tour-pace">Pace:</label>
            <select id="tour-pace">
              <option value="wait">I wait for you at each stop (press Next or say "next")</option>
              <option value="auto">I keep going on my own</option>
            </select>
          </div>
          <span id="tour-pace-msg" class="status-msg"></span>
          <div class="form-row">
            <label for="tour-navigate">Open files and highlight lines in VS Code:</label>
            <input type="checkbox" id="tour-navigate" checked />
          </div>
          <span id="tour-navigate-msg" class="status-msg"></span>
        </section>

        <!-- 4. Connect Coding Agents -->
        <section class="settings-card">
          <label class="card-title">Connect Coding Agents</label>
          <p class="card-desc">Barnaby hooks into your terminal agents to speak status messages and guide you.</p>
          <div class="agents-grid">
            <div class="agent-connect-card">
              <span class="agent-name">Claude Code</span>
              <button id="connect-claude-btn" class="btn btn-outline">Connect</button>
            </div>
            <div class="agent-connect-card">
              <span class="agent-name">Gemini CLI</span>
              <button id="connect-gemini-btn" class="btn btn-outline">Connect</button>
            </div>
          </div>
          <div style="margin-top: 12px;">
            <button id="copy-snippet-btn" class="btn btn-secondary btn-sm">Copy instructions snippet</button>
            <span id="copy-msg" class="status-msg"></span>
          </div>
        </section>

        <!-- 5. Known Projects -->
        <section class="settings-card">
          <label class="card-title">Projects</label>
          <p class="card-desc">Indexed projects for codebase understanding.</p>
          <ul id="projects-list" class="projects-list">
            <li class="empty-hint">No projects added yet</li>
          </ul>
        </section>

        <!-- 6. Privacy Note -->
        <footer class="settings-privacy">
          <p>Questions and the code needed to answer them are sent to Meta's Model API. Nothing else leaves this computer.</p>
        </footer>
      </div>
    </div>
  `;

  // Fetch initial config from Rust
  invoke<AppConfig>("get_config")
    .then((cfg) => {
      const volSlider = document.getElementById("volume-slider") as HTMLInputElement;
      const muteToggle = document.getElementById("mute-all-toggle") as HTMLInputElement;
      const pttKey = document.getElementById("ptt-key") as HTMLInputElement;
      const lookAfter = document.getElementById("look-after") as HTMLSelectElement | null;

      if (volSlider) volSlider.value = cfg.voice.volume.toString();
      if (muteToggle) muteToggle.checked = !cfg.voice.enabled;
      if (pttKey) pttKey.value = cfg.push_to_talk || "Ctrl+Alt+Space";
      if (lookAfter) lookAfter.value = cfg.look?.after_select ?? "choices";

      // Populate voice selects
      const voices = cfg.voice.voices || {};
      const claudeSel = document.getElementById("voice-claude") as HTMLSelectElement;
      const geminiSel = document.getElementById("voice-gemini") as HTMLSelectElement;
      const barnabySel = document.getElementById("voice-barnaby") as HTMLSelectElement;
      const otherSel = document.getElementById("voice-other") as HTMLSelectElement;

      if (claudeSel && voices["claude-code"]) claudeSel.value = voices["claude-code"];
      if (geminiSel && voices["gemini-cli"]) geminiSel.value = voices["gemini-cli"];
      if (barnabySel && voices["barnaby"]) barnabySel.value = voices["barnaby"];
      if (otherSel && voices["other"]) otherSel.value = voices["other"];

      const navToggle = document.getElementById("tour-navigate") as HTMLInputElement | null;
      if (navToggle) navToggle.checked = cfg.tour?.navigate ?? true;
      const paceSel = document.getElementById("tour-pace") as HTMLSelectElement | null;
      if (paceSel) paceSel.value = cfg.tour?.pace ?? "wait";

      // Render projects
      const list = document.getElementById("projects-list");
      if (list && cfg.projects.length > 0) {
        list.innerHTML = cfg.projects.map((p) => `<li>${p}</li>`).join("");
      }
    })
    .catch(console.error);

  // Wire Voice Controls
  const volSlider = document.getElementById("volume-slider") as HTMLInputElement;
  if (volSlider) {
    volSlider.addEventListener("input", () => {
      const vol = parseFloat(volSlider.value);
      invoke("set_voice_volume", { volume: vol }).catch(console.error);
    });
  }

  const muteToggle = document.getElementById("mute-all-toggle") as HTMLInputElement;
  if (muteToggle) {
    muteToggle.addEventListener("change", () => {
      const enabled = !muteToggle.checked;
      invoke("set_voice_enabled", { enabled }).catch(console.error);
    });
  }

  const bindVoiceSelect = (id: string, agentKind: string) => {
    const el = document.getElementById(id) as HTMLSelectElement;
    if (el) {
      el.addEventListener("change", () => {
        invoke("set_agent_voice", { agentKind, voice: el.value }).catch(console.error);
      });
    }
  };
  bindVoiceSelect("voice-claude", "claude-code");
  bindVoiceSelect("voice-gemini", "gemini-cli");
  bindVoiceSelect("voice-barnaby", "barnaby");
  bindVoiceSelect("voice-other", "other");

  // Talk key: try the new key straight away; the old one stays if Windows refuses it
  const pttInput = document.getElementById("ptt-key") as HTMLInputElement | null;
  const pttMsg = document.getElementById("ptt-msg");
  document.getElementById("ptt-save")?.addEventListener("click", () => {
    if (pttMsg) pttMsg.textContent = "";
    invoke("set_push_to_talk", { shortcut: pttInput?.value ?? "" })
      .then(() => {
        if (pttMsg) pttMsg.textContent = `Hold ${pttInput?.value.trim()} to talk.`;
      })
      .catch((e) => {
        if (pttMsg) pttMsg.textContent = `${e}`;
      });
  });

  // Look mode: what he does once a selection settles
  const lookAfterSel = document.getElementById("look-after") as HTMLSelectElement | null;
  const lookAfterMsg = document.getElementById("look-after-msg");
  lookAfterSel?.addEventListener("change", () => {
    invoke("set_look_after_select", { after: lookAfterSel.value })
      .then(() => {
        if (lookAfterMsg) lookAfterMsg.textContent = "Saved.";
      })
      .catch((e) => {
        if (lookAfterMsg) lookAfterMsg.textContent = `${e}`;
      });
  });

  // Tours: who sets the pace
  const paceSel = document.getElementById("tour-pace") as HTMLSelectElement | null;
  const paceMsg = document.getElementById("tour-pace-msg");
  paceSel?.addEventListener("change", () => {
    invoke("set_tour_pace", { pace: paceSel.value })
      .then(() => {
        if (paceMsg) paceMsg.textContent = paceSel.value === "wait" ? "Barnaby will wait for you at each stop." : "Barnaby will keep going on his own.";
      })
      .catch((e) => {
        if (paceMsg) paceMsg.textContent = `${e}`;
      });
  });

  // Tours: move VS Code, or only say where to look
  const navToggle = document.getElementById("tour-navigate") as HTMLInputElement | null;
  const navMsg = document.getElementById("tour-navigate-msg");
  navToggle?.addEventListener("change", () => {
    invoke("set_tour_navigate", { navigate: navToggle.checked })
      .then(() => {
        if (navMsg) navMsg.textContent = navToggle.checked ? "Barnaby will open files and highlight lines." : "Barnaby will only tell you where to look.";
      })
      .catch((e) => {
        if (navMsg) navMsg.textContent = `${e}`;
      });
  });

  // Hear the chosen voice
  const hearBtn = document.createElement("button");
  hearBtn.className = "btn btn-secondary btn-sm";
  hearBtn.textContent = "Hear";
  hearBtn.addEventListener("click", () => {
    const sel = document.getElementById("voice-barnaby") as HTMLSelectElement | null;
    if (sel) invoke("preview_voice", { voice: sel.value }).catch((e) => alert(`${e}`));
  });
  document.getElementById("voice-barnaby")?.after(hearBtn);

  // Phonon Status Poller & Action Renderer
  const updatePhononUI = (info: PhononStatusInfo) => {
    const statusEl = document.getElementById("phonon-status");
    const msgEl = document.getElementById("phonon-msg");
    const actionsEl = document.getElementById("phonon-actions");

    if (!statusEl || !msgEl || !actionsEl) return;

    statusEl.className = `badge-status ${info.state}`;
    statusEl.textContent = info.state.charAt(0).toUpperCase() + info.state.slice(1);
    msgEl.textContent = info.message || "";

    actionsEl.innerHTML = "";

    if (info.state === "ready") {
      statusEl.textContent = "Ready (127.0.0.1:8001)";
      return;
    }

    if (info.state === "starting" || info.state === "installing") {
      return; // in progress
    }

    // Offline / Failed: Show actionable buttons
    if (!info.uv_installed) {
      const btn = document.createElement("button");
      btn.className = "btn btn-outline btn-sm";
      btn.textContent = "Install uv Package Manager";
      btn.onclick = async () => {
        if (confirm("Barnaby needs the uv package manager to run Phonon TTS. Install now?")) {
          btn.disabled = true;
          btn.textContent = "Installing uv...";
          try {
            await invoke("install_uv");
            pollPhononStatus();
          } catch (e) {
            alert(`Failed to install uv: ${e}`);
          }
        }
      };
      actionsEl.appendChild(btn);
    } else if (!info.ptts_installed) {
      const btn = document.createElement("button");
      btn.className = "btn btn-outline btn-sm";
      btn.textContent = "Install Phonon TTS (ptts)";
      btn.onclick = async () => {
        btn.disabled = true;
        btn.textContent = "Installing ptts...";
        try {
          await invoke("install_ptts");
          pollPhononStatus();
        } catch (e) {
          alert(`Failed to install ptts: ${e}`);
        }
      };
      actionsEl.appendChild(btn);
    } else {
      const btn = document.createElement("button");
      btn.className = "btn btn-primary btn-sm";
      btn.textContent = "Start Phonon TTS Server";
      btn.onclick = async () => {
        btn.disabled = true;
        btn.textContent = "Starting...";
        try {
          await invoke("start_phonon");
          pollPhononStatus();
        } catch (e) {
          alert(`Failed to start Phonon: ${e}`);
        }
      };
      actionsEl.appendChild(btn);
    }
  };

  const pollPhononStatus = () => {
    invoke<PhononStatusInfo>("get_phonon_status")
      .then(updatePhononUI)
      .catch((err) => {
        console.error("Failed to query Phonon status:", err);
      });
  };

  pollPhononStatus();
  const pollInterval = setInterval(pollPhononStatus, 2500);

  // Meta API key: save, test
  const keyInput = document.getElementById("api-key-input") as HTMLInputElement | null;
  const keyMsg = document.getElementById("api-key-msg");
  const setKeyMsg = (text: string, kind: "ok" | "error" | "busy" = "ok") => {
    if (!keyMsg) return;
    keyMsg.textContent = text;
    keyMsg.dataset.kind = kind;
  };
  invoke<boolean>("has_api_key")
    .then((has) => {
      if (has && keyInput) keyInput.placeholder = "A key is saved. Paste a new one to replace it.";
    })
    .catch(console.error);
  const saveTypedKey = async (): Promise<boolean> => {
    const key = keyInput?.value.trim() ?? "";
    if (!key) return false;
    await invoke("save_api_key", { key });
    if (keyInput) {
      keyInput.value = "";
      keyInput.placeholder = "A key is saved. Paste a new one to replace it.";
    }
    return true;
  };
  document.getElementById("api-key-save")?.addEventListener("click", async () => {
    try {
      if (await saveTypedKey()) setKeyMsg("Saved.");
      else setKeyMsg("Paste a key first.", "error");
    } catch (e) {
      setKeyMsg(`${e}`, "error");
    }
  });
  document.getElementById("api-key-test")?.addEventListener("click", async () => {
    const testBtn = document.getElementById("api-key-test") as HTMLButtonElement;
    testBtn.disabled = true;
    setKeyMsg("Testing...", "busy");
    try {
      await saveTypedKey();
      setKeyMsg(await invoke<string>("test_api_key"), "ok");
    } catch (e) {
      setKeyMsg(`${e}`, "error");
    } finally {
      testBtn.disabled = false;
    }
  });

  // Connect Claude Code: show exactly what changes, then ask (AGENTS.md rule).
  const connectBtn = document.getElementById("connect-claude-btn") as HTMLButtonElement | null;
  if (connectBtn) {
    connectBtn.addEventListener("click", async () => {
      try {
        const preview = await invoke<string>("connect_preview_claude");
        if (!confirm(preview)) return;
        connectBtn.disabled = true;
        connectBtn.textContent = "Connecting...";
        const result = await invoke<string>("connect_claude");
        connectBtn.textContent = "Connected";
        alert(result);
      } catch (e) {
        connectBtn.disabled = false;
        connectBtn.textContent = "Connect";
        alert(`${e}`);
      }
    });
  }

  const geminiBtn = document.getElementById("connect-gemini-btn") as HTMLButtonElement | null;
  if (geminiBtn) {
    geminiBtn.addEventListener("click", () => {
      alert("Gemini CLI: add this to ~/.gemini/settings.json under mcpServers (see README). The automatic button is cut from the hackathon scope.");
    });
  }

  // Copy instructions snippet button (SPEC 5.4.4)
  const copyBtn = document.getElementById("copy-snippet-btn");
  if (copyBtn) {
    copyBtn.addEventListener("click", () => {
      const snippet = `## Barnaby (desktop pet)
A desktop pet reads your progress to the user out loud. Use the \`barnaby\` MCP tools:
- barnaby_status at the start of a task, at each milestone, and when blocked. One short spoken sentence.
- barnaby_done when the task is finished: what changed, in plain words.
- barnaby_ask_user instead of stopping when you need a quick decision.
- barnaby_tour when the user asks you to walk them through code you wrote.
Never put secrets or long code in these calls.`;

      navigator.clipboard.writeText(snippet);
      const msg = document.getElementById("copy-msg");
      if (msg) {
        msg.textContent = "Copied to clipboard!";
        setTimeout(() => (msg.textContent = ""), 3000);
      }
    });
  }

  // Clear interval when view destroyed
  window.addEventListener("beforeunload", () => {
    clearInterval(pollInterval);
  });
}
