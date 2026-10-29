import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  Action,
  Snapshot,
  Transport,
  ReviewedFile,
  ConversionReply,
  EditSettings,
  EditorInfo,
  EditorSaveReply,
} from "./types";
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

export const convertImports = (files: ReviewedFile[]) =>
  invoke<ConversionReply>("convert_imports", { files });
export const showBackups = () => invoke<void>("show_backups");

export const openEditor = (channel: number) =>
  invoke<EditorInfo>("open_editor", { channel });
export const previewEditor = (
  id: string,
  sequence: number,
  settings: EditSettings,
  enabled: boolean,
) =>
  invoke<{ sequence: number; duration: number }>("preview_editor", {
    id,
    sequence,
    settings,
    enabled,
  });
export const saveEditor = (id: string, settings: EditSettings) =>
  invoke<EditorSaveReply>("save_editor", { id, settings });
export const closeEditor = (id: string) => invoke<void>("close_editor", { id });
export const auditionEditor = (id: string) =>
  invoke<void>("audition_editor", { id });
export const showEditBackups = () => invoke<void>("show_edit_backups");
