import { invoke } from "@tauri-apps/api/core";
import { AppConfig } from "./types";

export function initSettingsView(container: HTMLElement): void {
  container.innerHTML = `
    <div class="settings-container">
      <header class="settings-header">
        <h2>Devy Settings</h2>
        <span class="settings-badge">v0.1.0</span>
      </header>

      <div class="settings-body">
        <!-- 1. Meta API Key -->
        <section class="settings-card">
          <label class="card-title">Meta Model API Key</label>
          <p class="card-desc">Used for code comprehension and codebase tours via <code>muse-spark-1.3</code>.</p>
          <div class="input-row">
            <input type="password" id="meta-key-input" placeholder="Enter your Meta API key..." />
            <button id="test-key-btn" class="btn btn-secondary">Test</button>
          </div>
          <span id="key-status-msg" class="status-msg"></span>
        </section>

        <!-- 2. Voice -->
        <section class="settings-card">
          <label class="card-title">Voice & TTS (Phonon)</label>
          <p class="card-desc">Local streaming neural speech engine listening on 127.0.0.1:8001.</p>
          <div class="form-row">
            <span>Status:</span>
            <span id="phonon-status" class="badge-status">Checking...</span>
          </div>
          <div class="form-row">
            <label for="volume-slider">Volume:</label>
            <input type="range" id="volume-slider" min="0" max="1" step="0.05" value="0.9" />
          </div>
          <div class="form-row">
            <label for="mute-all-toggle">Mute all voice:</label>
            <input type="checkbox" id="mute-all-toggle" />
          </div>
        </section>

        <!-- 3. Push to Talk -->
        <section class="settings-card">
          <label class="card-title">Push-to-Talk</label>
          <p class="card-desc">Hold to ask questions about your active codebase.</p>
          <div class="shortcut-box">
            <kbd id="ptt-key">Ctrl + Alt + Space</kbd>
          </div>
        </section>

        <!-- 4. Connect Coding Agents -->
        <section class="settings-card">
          <label class="card-title">Connect Coding Agents</label>
          <p class="card-desc">Devy hooks into your terminal agents to speak status messages and guide you.</p>
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
      const pttKey = document.getElementById("ptt-key") as HTMLElement;

      if (volSlider) volSlider.value = cfg.voice.volume.toString();
      if (muteToggle) muteToggle.checked = !cfg.voice.enabled;
      if (pttKey) pttKey.textContent = cfg.push_to_talk || "Ctrl + Alt + Space";

      // Render projects
      const list = document.getElementById("projects-list");
      if (list && cfg.projects.length > 0) {
        list.innerHTML = cfg.projects.map((p) => `<li>${p}</li>`).join("");
      }
    })
    .catch(console.error);

  // Check phonon health
  fetch("http://127.0.0.1:8001/health")
    .then((res) => {
      const statusEl = document.getElementById("phonon-status");
      if (statusEl) {
        statusEl.textContent = res.ok ? "Ready" : "Offline";
        statusEl.className = res.ok ? "badge-status ready" : "badge-status offline";
      }
    })
    .catch(() => {
      const statusEl = document.getElementById("phonon-status");
      if (statusEl) {
        statusEl.textContent = "Offline (not started)";
        statusEl.className = "badge-status offline";
      }
    });

  // Copy instructions snippet button
  const copyBtn = document.getElementById("copy-snippet-btn");
  if (copyBtn) {
    copyBtn.addEventListener("click", () => {
      const snippet = `You have a companion desktop pet called Devy. Keep status updates concise. Call devy_status when starting a major step.`;
      navigator.clipboard.writeText(snippet);
      const msg = document.getElementById("copy-msg");
      if (msg) {
        msg.textContent = "Copied to clipboard!";
        setTimeout(() => (msg.textContent = ""), 3000);
      }
    });
  }
}
