import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  Transport,
  EditSettings,
  EditorInfo,
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
/**
 * Removes `tag` from the Samples in `ids`, or from every Sample (Trash too)
 * when `ids` is null, writing `replacement` in its place. Renaming onto an
 * existing tag merges both. Returns the Samples that changed.
 */
export const replaceSampleTag = (
  tag: string,
  replacement: string | null,
  ids: string[] | null,
) => invoke<LibrarySample[]>("replace_sample_tag", { tag, replacement, ids });
/** Restores complete tag lists: Undo of a tag change. */
export const setSampleTags = (changes: { id: string; tags: string[] }[]) =>
  invoke<LibrarySample[]>("set_sample_tags", { changes });
export const importSamples = (paths: string[]) =>
  invoke<LibraryImportReply>("import_samples", { paths });
export const chooseSampleExport = (name: string) =>
  save({
    title: "Export Sample audio",
    defaultPath: `${name}.wav`,
    filters: [{ name: "WAV", extensions: ["wav"] }],
  });
/** Normalized peaks of a Sample's current Rample sound. */
export const sampleWaveform = (id: string) =>
  invoke<number[]>("sample_waveform", { id });
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
export const getTransport = () => invoke<Transport>("get_transport");
export async function chooseFolder(
  title = "Open your Rample SD or sample library",
) {
  const result = await open({ directory: true, multiple: false, title });
  return typeof result === "string" ? result : null;
}

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
export const closeEditor = (id: string) => invoke<void>("close_editor", { id });
export const auditionEditor = (id: string) =>
  invoke<void>("audition_editor", { id });

export const patchAction = (action: import("./types").PatchAction) =>
  invoke<import("./types").PatchSnapshot>("patch_action", { action });
export const reviewPatchBuild = (patchId: string, destination: string) =>
  invoke<import("./types").BuildReview>("review_patch_build", {
    patchId,
    destination,
  });
export const applyPatchBuild = (review: import("./types").BuildReview) =>
  invoke<import("./types").BuildReply>("apply_patch_build", { review });
export const patchBuildBackups = () =>
  invoke<import("./types").BuildBackup[]>("patch_build_backups");
export const restorePatchBuild = (id: string, destination: string) =>
  invoke<import("./types").RestoreReply>("restore_patch_build", {
    id,
    destination,
  });
export const showPatchBuildBackups = () =>
  invoke<void>("show_patch_build_backups");

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
