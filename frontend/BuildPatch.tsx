import type { BuildReply, BuildReview } from "./types";

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
  onDismiss,
}: {
  built: BuildReply;
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
      <button onClick={onDismiss}>Dismiss</button>
    </section>
  );
}
