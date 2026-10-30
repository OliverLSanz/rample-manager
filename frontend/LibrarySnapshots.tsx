import { useRef, useEffect, useState } from "react";
import { Modal } from "./Modal";
import * as native from "./native";
import type { LibrarySnapshotReply } from "./types";

export function LibrarySnapshots({
  managed,
  onClose,
  onRestored,
}: {
  managed: boolean;
  onClose: () => void;
  onRestored: (result: LibrarySnapshotReply) => void;
}) {
  const [savedRecoveries, setSavedRecoveries] = useState<string[]>([]);
  const [recoveryError, setRecoveryError] = useState<string | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [recovery, setRecovery] = useState<string | null>(null);
  const [operation, setOperation] = useState<"backup" | "restore">("backup");
  const [busy, setBusy] = useState(false);
  const [reply, setReply] = useState<LibrarySnapshotReply | null>(null);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    if (managed)
      void native
        .libraryRecoverySnapshots()
        .then((paths) => {
          if (alive.current) setSavedRecoveries(paths);
        })
        .catch((e) => {
          if (alive.current)
            setRecoveryError(
              `Recovery snapshots could not be listed: ${String(e)}`,
            );
        });
    return () => {
      alive.current = false;
    };
  }, [managed]);
  async function backup() {
    setBusy(true);
    setError(null);
    setReply(null);
    setOperation("backup");
    try {
      const path = await native.chooseLibraryBackup();
      if (!path) return;
      const result = await native.backupLibrary(path);
      if (alive.current) setReply(result);
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      if (alive.current) setBusy(false);
    }
  }
  async function chooseRestore(path?: string) {
    setBusy(true);
    setError(null);
    try {
      const source =
        path ??
        (await native.chooseFolder(
          "Choose a complete Library snapshot folder",
        ));
      if (source && alive.current) setPending(source);
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      if (alive.current) setBusy(false);
    }
  }
  async function restore() {
    if (!pending) return;
    const source = pending;
    setPending(null);
    setBusy(true);
    setError(null);
    setReply(null);
    setOperation("restore");
    try {
      const result = await native.restoreLibrary(source);
      if (!alive.current) return;
      setReply(result);
      if (result.recovery) setRecovery(result.recovery);
      if (result.activated) onRestored(result);
    } catch (e) {
      if (alive.current) setError(`Library was not replaced: ${String(e)}`);
    } finally {
      if (alive.current) setBusy(false);
    }
  }
  return (
    <Modal
      label="Library backup and restore"
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <h2>Library backup and restore</h2>
      <p>
        A portable snapshot includes Originals, names and tags, saved edits and
        history, Variants, Patches and musical state, native source data, Trash
        and recognition records.
      </p>
      {!managed && (
        <p role="alert">
          Move the folder session into the managed Library before backing up or
          restoring. Return to folders to complete migration first.
        </p>
      )}
      <button
        disabled={busy || !managed || pending !== null}
        onClick={() => void backup()}
      >
        Back up Library…
      </button>
      <button
        disabled={busy || !managed || pending !== null}
        onClick={() => void chooseRestore()}
      >
        Restore Library…
      </button>
      {pending && (
        <div role="group" aria-label="Confirm Library replacement">
          <p>
            Restoring replaces the entire active Library with this snapshot. It
            does not merge Libraries.
          </p>
          <p>{pending}</p>
          <p>
            A verified recovery snapshot of the current Library will be created
            before replacement. Its location will be shown so you can restore it
            here.
          </p>
          <button disabled={busy} onClick={() => void restore()}>
            Replace Library
          </button>
          <button disabled={busy} onClick={() => setPending(null)}>
            Cancel replacement
          </button>
        </div>
      )}
      {busy && <p role="status">Verifying and copying Library files…</p>}
      {error && <p role="alert">{error}</p>}
      {!pending && reply && (
        <div role="status">
          <p>
            {operation === "backup"
              ? reply.complete
                ? "Library backup complete."
                : "Library backup failed."
              : reply.activated
                ? reply.complete
                  ? "Library restored."
                  : "Library restored; playback needs attention."
                : "Library was not replaced."}
          </p>
          {reply.error && <p role="alert">{reply.error}</p>}
          {reply.location && <p>{reply.location}</p>}
        </div>
      )}
      {!pending && recovery && (
        <div>
          <p>Recovery snapshot</p>
          <p>{recovery}</p>
          <button disabled={busy} onClick={() => void chooseRestore(recovery)}>
            Restore recovery snapshot…
          </button>
        </div>
      )}
      {!pending &&
        savedRecoveries.filter((path) => path !== recovery).length > 0 && (
          <div>
            <h3>Saved recovery snapshots</h3>
            <p>Each snapshot is verified again before replacement.</p>
            <ul aria-label="Saved recovery snapshots">
              {savedRecoveries
                .filter((path) => path !== recovery)
                .map((path) => (
                  <li key={path}>
                    <p>{path}</p>
                    <button
                      disabled={busy}
                      onClick={() => void chooseRestore(path)}
                    >
                      Restore this recovery snapshot…
                    </button>
                  </li>
                ))}
            </ul>
          </div>
        )}
      {recoveryError && <p role="alert">{recoveryError}</p>}
      <button disabled={busy} onClick={onClose}>
        Done
      </button>
    </Modal>
  );
}
