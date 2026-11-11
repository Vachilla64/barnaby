export interface ClickRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface PlatformSegment {
  x1: number;
  x2: number;
  y: number;
  hwnd: number;
  title: string;
}

export interface PlatformRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export type PetState =
  | "idle"
  | "working"
  | "talking"
  | "thinking"
  | "alert"
  | "happy"
  | "sleeping"
  | "guiding";

/** What Barnaby does once code is selected in look mode. */
export type AfterSelect = "choices" | "explain" | "listen";

/** Who sets the pace of a tour: the person (Next) or Barnaby. */
export type TourPace = "wait" | "auto";

/** What the first-run guide ticks off (Rust `setup_status`). */
export interface SetupStatus {
  has_key: boolean;
  vscode: boolean;
  extension_installed: boolean;
  folders: string[];
  onboarding_done: boolean;
  talk_key: string;
  talk_key_ok: boolean;
}

export interface FeedItem {
  time: string;
  text: string;
}

export interface PetConfig {
  colour: string;
  x: number | null;
  y: number | null;
}

export interface AppConfig {
  port: number;
  token: string;
  brain: { model: string; base_url?: string };
  tour?: { navigate: boolean; pace?: TourPace };
  look?: { after_select: AfterSelect };
  onboarding_done?: boolean;
  voice: {
    enabled: boolean;
    volume: number;
    voices: Record<string, string>;
  };
  push_to_talk: string;
  pets: Record<string, PetConfig>;
  projects: string[];
}

export interface BubbleButton {
  id: string;
  label: string;
  onClick: () => void;
}

export interface PhononStatusInfo {
  state: "ready" | "starting" | "installing" | "offline" | "failed";
  uv_installed: boolean;
  ptts_installed: boolean;
  message?: string;
}
