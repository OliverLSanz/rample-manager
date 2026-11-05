import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  Action,
  Snapshot,
  Transport,
  ReviewedFile,
  ConversionReply,
  EditSettings,
  EditorInfo,
  EditorSaveReply,
  LibrarySample,
  LibraryImportReply,
} from "./types";
export const listSamples = () => invoke<LibrarySample[]>("list_samples");
export const listTrashedSamples = () =>
  invoke<LibrarySample[]>("list_trashed_samples");
export const retireSample = (id: string) =>
  invoke<LibrarySample>("retire_sample", { id });
export const restoreSample = (id: string) =>
  invoke<LibrarySample>("restore_sample", { id });
export const querySamples = (query: string) =>
  invoke<LibrarySample[]>("query_samples", { query });
export const updateSampleMetadata = (
  id: string,
  name: string,
  tags: string[],
) => invoke<LibrarySample>("update_sample_metadata", { id, name, tags });
export const importSamples = (paths: string[]) =>
  invoke<LibraryImportReply>("import_samples", { paths });
export const chooseSampleExport = (name: string) =>
  save({
    title: "Export Sample audio",
    defaultPath: `${name}.wav`,
    filters: [{ name: "WAV", extensions: ["wav"] }],
  });
export const exportSample = (id: string, destination: string) =>
  invoke<void>("export_sample", { id, destination });
export const openSampleAudition = () => invoke<string>("open_sample_audition");
export const auditionSample = (id: string, session: string, rample = false) =>
  invoke<void>("audition_sample", { id, session, rample });
export const stopSampleAudition = (session: string, closing = false) =>
  invoke<boolean>("stop_sample_audition", { session, closing });
export async function chooseSampleSources(folder: boolean) {
  const result = await open({
    title: folder ? "Import a folder of audio Samples" : "Import audio Samples",
    directory: folder,
    multiple: !folder,
    ...(folder
      ? {}
      : {
          filters: [
            {
              name: "Audio",
              extensions: [
                "wav",
                "wave",
                "aif",
                "aiff",
                "aifc",
                "flac",
                "mp3",
                "ogg",
                "oga",
                "m4a",
                "aac",
              ],
            },
          ],
        }),
  });
  return result === null ? [] : typeof result === "string" ? [result] : result;
}
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

export const patchAction = (action: import("./types").PatchAction) =>
  invoke<import("./types").PatchSnapshot>("patch_action", { action });
export const closePatch = () => invoke<void>("close_patch");
export const buildPatch = (patch: string, destination: string) =>
  invoke<import("./types").BuildReply>("build_patch", { patch, destination });

export const openManagedEditor = (channel: number) =>
  invoke<EditorInfo>("open_managed_editor", { channel });
export const saveManagedEditor = (
  id: string,
  settings: EditSettings,
  variantName: string | null,
) =>
  invoke<{ editor: EditorInfo; patch: import("./types").PatchSnapshot }>(
    "save_managed_editor",
    { id, settings, variantName },
  );

export const prepareSdImport = (source: string) =>
  invoke<import("./types").SdImportReview>("prepare_sd_import", { source });
export const finishSdImport = (
  id: string,
  choice: import("./types").SdImportChoice,
) =>
  invoke<import("./types").PatchSnapshot>("finish_sd_import", { id, choice });

export const migrationStatus = () =>
  invoke<import("./types").MigrationStatus>("migration_status");
export const migrateLegacy = () =>
  invoke<import("./types").MigrationStatus>("migrate_legacy");

export const chooseLibraryBackup = () =>
  save({
    title: "Back up Library: choose a new snapshot folder name",
    defaultPath: `Rample Library ${new Date().toISOString().slice(0, 10)}`,
  });
export const backupLibrary = (destination: string) =>
  invoke<import("./types").LibrarySnapshotReply>("backup_library", {
    destination,
  });
export const restoreLibrary = (source: string) =>
  invoke<import("./types").LibrarySnapshotReply>("restore_library", { source });
export const libraryRecoverySnapshots = () =>
  invoke<string[]>("library_recovery_snapshots");
