import { useEffect, useRef, useState } from "react";
import * as native from "./native";
import type {
  BuildBackup,
  BuildReply,
  BuildReview,
  BuildStatus,
} from "./types";

function plural(count: number, one: string, many = `${one}s`) {
  return `${count} ${count === 1 ? one : many}`;
}

function PathList({ label, paths }: { label: string; paths: string[] }) {
  if (paths.length === 0) return null;
  return (
    <details open={paths.length <= 12}>
      <summary>
        {label} ({paths.length})
      </summary>
      <ul aria-label={label}>
        {paths.map((path) => (
          <li key={path}>{path}</li>
        ))}
      </ul>
    </details>
  );
}

/** The destination difference shown before Build Patch changes anything. */
export function BuildReviewPanel({
  review,
  busy,
  onApply,
  onCancel,
}: {
  review: BuildReview;
  busy: boolean;
  onApply: () => void;
  onCancel: () => void;
}) {
  const blocked = review.conflicts.length > 0;
  return (
    <section className="patch-build" aria-label="Build Patch review">
      <h3>Review changes to {review.destination}</h3>
      <p>
        {`${review.additions.length} to add, ${review.replacements.length} to replace, ${review.removals.length} to remove, ${review.unchanged.length} already up to date.`}
      </p>
      {blocked && (
        <div className="patch-build-conflicts">
          <p>Resolve these before building onto this destination:</p>
          <ul>
            {review.conflicts.map((conflict, i) => (
              <li key={i}>{conflict}</li>
            ))}
          </ul>
        </div>
      )}
      {review.removedKits.length > 0 && (
        <p>{`Kits removed from the destination: ${review.removedKits.join(", ")}.`}</p>
      )}
      <PathList label="Files to add" paths={review.additions} />
      <PathList label="Files to replace" paths={review.replacements} />
      <PathList label="Files to remove" paths={review.removals} />
      <PathList label="Files already up to date" paths={review.unchanged} />
      <p>
        Replaced and removed files are backed up on this computer before
        anything changes. Native settings (_save), firmware and files outside
        Kit folders are left untouched; settings that no longer match the files
        are yours to update on the Rample.
      </p>
      {review.notices.length > 0 && (
        <ul>
          {review.notices.map((notice, i) => (
            <li key={i}>{notice}</li>
          ))}
        </ul>
      )}
      <div className="patch-build-actions">
        <button
          className="primary"
          disabled={busy || blocked}
          onClick={onApply}
        >
          Apply Build
        </button>
        <button disabled={busy} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </section>
  );
}

/** The outcome of an applied review, never claiming an incomplete Build. */
export function BuildResult({
  built,
  onRestore,
  onDismiss,
}: {
  built: BuildReply;
  onRestore: (backup: string) => void;
  onDismiss: () => void;
}) {
  return (
    <section className="patch-build" aria-label="Build Patch result">
      {built.error ? (
        <div role="alert">
          {built.error.split("\n").map((line, i) => (
            <p key={i}>{line}</p>
          ))}
        </div>
      ) : (
        <>
          <p>
            {`Built ${plural(built.kits.length, "Kit")} and ${plural(built.files.length, "audio file")} into ${built.destination}.`}
          </p>
          <p>
            {`Added ${built.added}, replaced ${built.replaced} and removed ${built.removed} files; ${built.unchanged} ${built.unchanged === 1 ? "was" : "were"} already up to date.`}
          </p>
          <p>
            Only Bank/Kit folders and prepared audio were written. Layer modes
            and selections are preview settings in this app: set them on the
            Rample and STORE.
          </p>
        </>
      )}
      {built.backup && (
        <p>{`Replaced and removed files are backed up in ${built.backup.folder}.`}</p>
      )}
      {built.notices.length > 0 && (
        <ul>
          {built.notices.map((notice, i) => (
            <li key={i}>{notice}</li>
          ))}
        </ul>
      )}
      <details>
        <summary>Patch files in Layer order</summary>
        <ul>
          {built.files.map((file) => (
            <li key={file.path}>
              {`${file.path} · Voice ${file.voice}, Layer ${file.layer}: ${file.sampleName}`}
            </li>
          ))}
        </ul>
      </details>
      <div className="patch-build-actions">
        {built.backup && (
          <button onClick={() => onRestore(built.backup!.id)}>
            Restore destination…
          </button>
        )}
        <button onClick={onDismiss}>Dismiss</button>
      </div>
    </section>
  );
}

function status(backup: BuildBackup) {
  const status: BuildStatus = backup.status;
  switch (status) {
    case "complete":
      return "Complete";
    case "incomplete":
      return `Incomplete: ${backup.error ?? "the destination is only partly updated."}`;
    case "applying":
      return "Interrupted: the destination may be only partly updated.";
  }
}

/** Destination backups of applied Builds, with a confirmed restore route. */
export function DestinationBackups({
  focus,
  onClose,
}: {
  /** Backup whose restore confirmation opens first. */
  focus: string | null;
  onClose: () => void;
}) {
  const [backups, setBackups] = useState<BuildBackup[] | null>(null);
  const [confirming, setConfirming] = useState<string | null>(focus);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string[]>([]);
  const alive = useRef(true);
  async function load() {
    try {
      const list = await native.patchBuildBackups();
      if (alive.current) setBackups(list);
    } catch (e) {
      if (alive.current) setError(String(e));
    }
  }
  useEffect(() => {
    alive.current = true;
    void load();
    return () => {
      alive.current = false;
    };
  }, []);
  async function restore(backup: BuildBackup, destination: string | null) {
    if (!destination || working) return;
    setWorking(true);
    setError(null);
    setMessage([]);
    try {
      const reply = await native.restorePatchBuild(backup.id, destination);
      if (!alive.current) return;
      setMessage([
        `Restored ${reply.destination}: ${plural(reply.restored, "file")} put back and ${reply.removed} removed.`,
        ...reply.notices,
      ]);
      setConfirming(null);
      await load();
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      if (alive.current) setWorking(false);
    }
  }
  return (
    <section className="patch-build" aria-label="Destination backups">
      <h3>Destination backups</h3>
      <p>
        Each Build that changed a folder or card keeps the files it replaced or
        removed on this computer. Restoring removes the files that Build added
        and puts back what it replaced or removed.
      </p>
      {error && (
        <div role="alert">
          {error.split("\n").map((line, i) => (
            <p key={i}>{line}</p>
          ))}
        </div>
      )}
      {message.map((line, i) => (
        <p key={i} role="status">
          {line}
        </p>
      ))}
      {backups === null ? (
        <p>Loading destination backups…</p>
      ) : backups.length === 0 ? (
        <p>No Build has changed a destination yet.</p>
      ) : (
        <ul className="patch-build-backups">
          {backups.map((backup) => (
            <li
              key={backup.id}
              aria-label={`${backup.patchName} → ${backup.destination}`}
            >
              <p>
                <strong>{`${backup.patchName} → ${backup.destination}`}</strong>{" "}
                · {new Date(backup.created).toLocaleString()}
              </p>
              <p>{status(backup)}</p>
              <p>
                {`Added ${backup.added.length}, replaced ${backup.replaced.length} and removed ${backup.removed.length} files.`}
              </p>
              {backup.restored !== null && (
                <p>{`Restored ${new Date(backup.restored).toLocaleString()}.`}</p>
              )}
              {confirming === backup.id ? (
                <div>
                  <p>
                    {`Restore ${backup.destination} to how it was before this Build? Files this Build added are removed, and the files it replaced or removed are put back. Files changed since this Build are never overwritten, and _save and other files are not touched.`}
                  </p>
                  <div className="patch-build-actions">
                    <button
                      className="primary"
                      disabled={working}
                      onClick={() => void restore(backup, backup.destination)}
                    >
                      Restore destination
                    </button>
                    <button
                      disabled={working}
                      onClick={() =>
                        void native
                          .chooseFolder(
                            `Choose the folder or card to restore from the ${backup.patchName} Build`,
                          )
                          .then((folder) => restore(backup, folder))
                      }
                    >
                      Restore another folder…
                    </button>
                    <button
                      disabled={working}
                      onClick={() => setConfirming(null)}
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              ) : (
                <button
                  disabled={working}
                  onClick={() => {
                    setConfirming(backup.id);
                    setMessage([]);
                  }}
                >
                  Restore…
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      <div className="patch-build-actions">
        <button onClick={() => void native.showPatchBuildBackups()}>
          Show backup folder
        </button>
        <button disabled={working} onClick={onClose}>
          Close
        </button>
      </div>
    </section>
  );
}
