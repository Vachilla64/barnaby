import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import WebSocket from "ws";

interface BarnabyConfig {
  port: number;
  token: string;
}

interface TourStep {
  type: "tour.step";
  tour_id: string;
  index: number;
  total: number;
  path: string;
  start_line: number;
  end_line: number;
  colour: string;
}

const RECONNECT_MS = 3000;
const CONTEXT_DEBOUNCE_MS = 300;
const MAX_SELECTION_CHARS = 4000;

let socket: WebSocket | undefined;
let reconnectTimer: NodeJS.Timeout | undefined;
let contextTimer: NodeJS.Timeout | undefined;
let statusItem: vscode.StatusBarItem;
let log: vscode.LogOutputChannel;
let decoration: vscode.TextEditorDecorationType | undefined;
let stopped = false;
/** The last problem written to the log, so a retry every 3 s does not repeat itself. */
let lastProblem = "";

function problem(message: string): void {
  if (message === lastProblem) return;
  lastProblem = message;
  log.warn(message);
}

/** Where Barnaby keeps its config. APPDATA first; the default location if that is missing or unreadable. */
function configPaths(): string[] {
  const roaming = path.join(os.homedir(), "AppData", "Roaming");
  const local = path.join(os.homedir(), "AppData", "Local");
  const dirs = [process.env.APPDATA ?? roaming, roaming, path.join(local, "Barnaby")];
  return [...new Set(dirs)].map((d) => (d.endsWith("Barnaby") ? path.join(d, "config.json") : path.join(d, "Barnaby", "config.json")));
}

function readConfig(): BarnabyConfig | undefined {
  for (const file of configPaths()) {
    try {
      if (!fs.existsSync(file)) continue;
      const cfg = JSON.parse(fs.readFileSync(file, "utf8"));
      if (typeof cfg.port === "number" && typeof cfg.token === "string") {
        return { port: cfg.port, token: cfg.token };
      }
      problem(`${file} has no port and token.`);
    } catch (e) {
      problem(`Could not read ${file}: ${(e as Error).message}`);
    }
  }
  return undefined;
}

function setStatus(connected: boolean, detail?: string): void {
  statusItem.text = "$(hubot) Barnaby";
  statusItem.color = connected ? new vscode.ThemeColor("charts.green") : new vscode.ThemeColor("disabledForeground");
  statusItem.tooltip = connected ? "Barnaby is connected" : `${detail ?? "Barnaby is not running"} (click to see the log)`;
  statusItem.command = connected ? "barnaby.ask" : "barnaby.showLog";
}

function send(msg: object): void {
  if (socket && socket.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify(msg));
  }
}

function scheduleReconnect(): void {
  if (stopped || reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = undefined;
    connect();
  }, RECONNECT_MS);
}

function connect(): void {
  const cfg = readConfig();
  if (!cfg) {
    setStatus(false, "Barnaby has not been started yet (no config file)");
    scheduleReconnect();
    return;
  }
  const ws = new WebSocket(`ws://127.0.0.1:${cfg.port}/ws?token=${encodeURIComponent(cfg.token)}`);
  socket = ws;

  ws.on("open", () => {
    lastProblem = "";
    log.info(`Connected to Barnaby on port ${cfg.port}`);
    setStatus(true);
    send({
      type: "hello",
      workspace_folders: (vscode.workspace.workspaceFolders ?? []).map((f) => f.uri.fsPath),
      vscode_version: vscode.version,
    });
    sendContext();
  });
  ws.on("message", (data) => {
    try {
      void handleMessage(JSON.parse(data.toString()));
    } catch (e) {
      console.error("Barnaby: bad message", e);
    }
  });
  ws.on("close", (code) => {
    if (lastProblem === "") log.info(`Connection to Barnaby closed (${code})`);
    setStatus(false, lastProblem || undefined);
    if (socket === ws) socket = undefined;
    scheduleReconnect();
  });
  ws.on("unexpected-response", (_req, res) => {
    problem(`Barnaby refused the connection (HTTP ${res.statusCode}). Is the token in ${configPaths()[0]} current?`);
  });
  ws.on("error", (e) => {
    // 'close' follows and reconnects.
    problem(`Cannot reach Barnaby on port ${cfg.port}: ${e.message}`);
  });
}

async function handleMessage(msg: { type: string } & Record<string, unknown>): Promise<void> {
  if (msg.type === "tour.step") {
    await showStep(msg as unknown as TourStep);
  } else if (msg.type === "tour.end") {
    clearHighlight();
  }
}

function clearHighlight(): void {
  decoration?.dispose();
  decoration = undefined;
}

function toRgba(hex: string, alpha: number): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex);
  if (!m) return `rgba(232, 135, 91, ${alpha})`;
  const n = parseInt(m[1], 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Scroll like a person reading down to the line: start above it, then step down. */
async function smoothReveal(editor: vscode.TextEditor, line: number): Promise<void> {
  const visible = editor.visibleRanges[0];
  const height = visible ? visible.end.line - visible.start.line : 30;
  const topTarget = Math.max(0, line - Math.floor(height / 3));
  let current = Math.max(0, topTarget - 10);
  editor.revealRange(new vscode.Range(current, 0, current, 0), vscode.TextEditorRevealType.AtTop);
  while (current < topTarget) {
    current = Math.min(topTarget, current + 2);
    editor.revealRange(new vscode.Range(current, 0, current, 0), vscode.TextEditorRevealType.AtTop);
    await sleep(30);
  }
}

async function showStep(step: TourStep): Promise<void> {
  try {
    const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(step.path));
    const editor = await vscode.window.showTextDocument(doc, {
      viewColumn: vscode.ViewColumn.One,
      preserveFocus: false,
      preview: true,
    });
    const last = doc.lineCount - 1;
    const start = Math.min(Math.max(step.start_line - 1, 0), last);
    const end = Math.min(Math.max(step.end_line - 1, start), last);

    await smoothReveal(editor, start);

    clearHighlight();
    decoration = vscode.window.createTextEditorDecorationType({
      backgroundColor: toRgba(step.colour, 0.22),
      isWholeLine: true,
      overviewRulerColor: step.colour,
      overviewRulerLane: vscode.OverviewRulerLane.Center,
      gutterIconPath: vscode.Uri.parse(
        "data:image/svg+xml;utf8," +
          encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><circle cx="8" cy="8" r="4" fill="${step.colour}"/></svg>`),
      ),
      gutterIconSize: "contain",
    });
    const range = new vscode.Range(start, 0, end, doc.lineAt(end).range.end.character);
    editor.setDecorations(decoration, [range]);
    editor.selection = new vscode.Selection(start, 0, start, 0);
  } catch (e) {
    console.error("Barnaby: could not show step", e);
  } finally {
    send({ type: "tour.step.shown", tour_id: step.tour_id, index: step.index });
  }
}

function sendContext(): void {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.uri.scheme !== "file") return;
  const sel = editor.selection;
  send({
    type: "editor.context",
    path: editor.document.uri.fsPath,
    selection_text: editor.document.getText(sel).slice(0, MAX_SELECTION_CHARS),
    start_line: sel.start.line + 1,
    end_line: sel.end.line + 1,
    cursor_line: sel.active.line + 1,
  });
}

function scheduleContext(): void {
  if (contextTimer) clearTimeout(contextTimer);
  contextTimer = setTimeout(sendContext, CONTEXT_DEBOUNCE_MS);
}

function control(action: "next" | "back" | "stop"): void {
  send({ type: "tour.control", action });
}

export function activate(context: vscode.ExtensionContext): void {
  stopped = false;
  log = vscode.window.createOutputChannel("Barnaby", { log: true });
  log.info(`Barnaby extension started (VS Code ${vscode.version})`);
  statusItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  setStatus(false);
  statusItem.show();

  context.subscriptions.push(
    statusItem,
    log,
    vscode.commands.registerCommand("barnaby.showLog", () => log.show(true)),
    vscode.commands.registerCommand("barnaby.ask", async () => {
      const question = await vscode.window.showInputBox({ prompt: "Ask Barnaby about your code" });
      if (question?.trim()) {
        sendContext();
        send({ type: "ask", question: question.trim() });
      }
    }),
    vscode.commands.registerCommand("barnaby.next", () => control("next")),
    vscode.commands.registerCommand("barnaby.previous", () => control("back")),
    vscode.commands.registerCommand("barnaby.stop", () => {
      control("stop");
      clearHighlight();
    }),
    vscode.workspace.onDidChangeWorkspaceFolders(() => {
      send({
        type: "hello",
        workspace_folders: (vscode.workspace.workspaceFolders ?? []).map((f) => f.uri.fsPath),
        vscode_version: vscode.version,
      });
      sendContext();
    }),
    vscode.window.onDidChangeTextEditorSelection(scheduleContext),
    vscode.window.onDidChangeActiveTextEditor(scheduleContext),
    { dispose: deactivate },
  );

  connect();
}

export function deactivate(): void {
  stopped = true;
  if (reconnectTimer) clearTimeout(reconnectTimer);
  if (contextTimer) clearTimeout(contextTimer);
  clearHighlight();
  socket?.close();
  socket = undefined;
}
