import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { Action, Snapshot, Transport } from "./types";
export const getSnapshot = () => invoke<Snapshot>("get_snapshot");
export const act = (action: Action) => invoke<Snapshot>("act", { action });
export const getTransport = () => invoke<Transport>("get_transport");
export const onLibraryChange = (callback: (s: Snapshot) => void) =>
  listen<Snapshot>("library-changed", (e) => callback(e.payload));
export async function chooseFolder(
  title = "Open your Rample SD or sample library",
) {
  const result = await open({ directory: true, multiple: false, title });
  return typeof result === "string" ? result : null;
}
export const exportKit = (name: string, parent: string) =>
  invoke<string>("export_kit", { name, parent });
export const reveal = (channel?: number) =>
  invoke("reveal", { channel: channel ?? null });
