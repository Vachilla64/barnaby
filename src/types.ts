export interface ClickRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type PetState =
  | "idle"
  | "working"
  | "talking"
  | "thinking"
  | "alert"
  | "happy"
  | "sleeping";

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
  brain: { model: string };
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
