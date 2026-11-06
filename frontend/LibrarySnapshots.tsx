import { useRef, useEffect, useState } from "react";
import { DatabaseBackup, Download, LoaderCircle, Upload } from "lucide-react";
import { Modal } from "./Modal";
import * as native from "./native";
import type { LibrarySnapshotReply } from "./types";

export function LibrarySnapshots({
  onClose,
  onRestored,
}: {
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
  }, []);
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
      <div className="dialog library-snapshots">
        <div className="modal-heading">
          <DatabaseBackup size={20} />
          <h2>Library backup and restore</h2>
        </div>
        <p>
          A portable snapshot includes Originals, names and tags, saved edits
          and history, Variants, Patches and musical state, native source data,
          Trash and recognition records.
        </p>
        <div className="dialog-actions">
          <button
            className="primary"
            disabled={busy || pending !== null}
            onClick={() => void backup()}
          >
            <Download size={14} />
            Back up Library…
          </button>
          <button
            className="secondary"
            disabled={busy || pending !== null}
            onClick={() => void chooseRestore()}
          >
            <Upload size={14} />
            Restore Library…
          </button>
        </div>
        {pending && (
          <section
            className="dialog-section warn"
            role="group"
            aria-label="Confirm Library replacement"
          >
            <h3>Replace the Library?</h3>
            <p>
              Restoring replaces the entire active Library with this snapshot.
              It does not merge Libraries.
            </p>
            <p className="dialog-path">{pending}</p>
            <p>
              A verified recovery snapshot of the current Library will be
              created before replacement. Its location will be shown so you can
              restore it here.
            </p>
            <div className="dialog-actions">
              <button
                className="primary"
                disabled={busy}
                onClick={() => void restore()}
              >
                Replace Library
              </button>
              <button
                className="secondary"
                disabled={busy}
                onClick={() => setPending(null)}
              >
                Cancel replacement
              </button>
            </div>
          </section>
        )}
        {busy && (
          <p className="dialog-busy" role="status">
            <LoaderCircle size={14} className="spin" />
            Verifying and copying Library files…
          </p>
        )}
        {error && (
          <p className="import-error" role="alert">
            {error}
          </p>
        )}
        {!pending && reply && (
          <section className="dialog-section" role="status">
            <h3>
              {operation === "backup"
                ? reply.complete
                  ? "Library backup complete."
                  : "Library backup failed."
                : reply.activated
                  ? reply.complete
                    ? "Library restored."
                    : "Library restored; playback needs attention."
                  : "Library was not replaced."}
            </h3>
            {reply.error && (
              <p className="import-error" role="alert">
                {reply.error}
              </p>
            )}
            {reply.location && <p className="dialog-path">{reply.location}</p>}
          </section>
        )}
        {!pending && recovery && (
          <section className="dialog-section">
            <h3>Recovery snapshot</h3>
            <p className="dialog-path">{recovery}</p>
            <div className="dialog-actions">
              <button
                className="secondary"
                disabled={busy}
                onClick={() => void chooseRestore(recovery)}
              >
                Restore recovery snapshot…
              </button>
            </div>
          </section>
        )}
        {!pending &&
          savedRecoveries.filter((path) => path !== recovery).length > 0 && (
            <section className="dialog-section">
              <h3>Saved recovery snapshots</h3>
              <p>Each snapshot is verified again before replacement.</p>
              <ul className="dialog-rows" aria-label="Saved recovery snapshots">
                {savedRecoveries
                  .filter((path) => path !== recovery)
                  .map((path) => (
                    <li key={path}>
                      <p className="dialog-path">{path}</p>
                      <button
                        className="secondary"
                        disabled={busy}
                        onClick={() => void chooseRestore(path)}
                      >
                        Restore this recovery snapshot…
                      </button>
                    </li>
                  ))}
              </ul>
            </section>
          )}
        {recoveryError && (
          <p className="import-error" role="alert">
            {recoveryError}
          </p>
        )}
        <div className="modal-buttons">
          <button className="secondary" disabled={busy} onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </Modal>
  );
}
