import { useRef, useEffect, useState } from "react";
import { Modal } from "./Modal";
import * as native from "./native";
import {
  modes,
  type NativeReading,
  type PatchSnapshot,
  type SdImportChoice,
  type SdImportReview,
} from "./types";

export function SdImport({
  onClose,
  onImported,
}: {
  onClose: () => void;
  onImported: (snapshot: PatchSnapshot) => void;
}) {
  const [review, setReview] = useState<SdImportReview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [existing, setExisting] = useState("");
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  async function choose() {
    setBusy(true);
    setError(null);
    try {
      const source = await native.chooseFolder(
        "Import Rample SD or a copy of its root folder",
      );
      if (!source || !alive.current) return;
      const next = await native.prepareSdImport(source);
      if (alive.current) {
        setReview(next);
        setName(next.draft.name);
        setExisting(next.candidates[0]?.id ?? "");
      }
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      if (alive.current) setBusy(false);
    }
  }
  async function finish(choice: SdImportChoice) {
    if (!review) return;
    setBusy(true);
    setError(null);
    try {
      const snapshot = await native.finishSdImport(review.id, choice);
      if (alive.current) onImported(snapshot);
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      if (alive.current) setBusy(false);
    }
  }
  async function restore(id: string) {
    setBusy(true);
    setError(null);
    try {
      const sample = await native.restoreSample(id);
      if (alive.current) {
        setReview((current) =>
          current
            ? {
                ...current,
                audio: {
                  ...current.audio,
                  samples: current.audio.samples.map((row) =>
                    row.id === id ? sample : row,
                  ),
                  results: current.audio.results.map((row) =>
                    row.sample?.id === id ? { ...row, sample } : row,
                  ),
                },
              }
            : current,
        );
        setMessage(`Restored ${sample.name} from Trash.`);
      }
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      if (alive.current) setBusy(false);
    }
  }
  const imported = review?.draft.importedSd;
  const retired = [
    ...new Map(
      review?.audio.results.flatMap((row) =>
        row.sample?.retired ? [[row.sample.id, row.sample] as const] : [],
      ) ?? [],
    ).values(),
  ];
  const nativeFiles =
    imported?.files.filter((file) => file.kind === "native") ?? [];
  return (
    <Modal
      label="Import Rample SD"
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <h2>Import Rample SD</h2>
      <p>
        Copy Samples, Bank/Kit addresses, ordered Voice Layers and opaque native
        files into the Library. Your SD is left unchanged.
      </p>
      {error && <p role="alert">{error}</p>}
      {message && <p role="status">{message}</p>}
      {busy && <p role="status">Copying and preparing managed audio…</p>}
      <button disabled={busy} onClick={() => void choose()}>
        Choose SD folder…
      </button>
      {review && (
        <>
          <p role="status">
            {review.audio.imported} new Sample(s), {review.audio.recognized}{" "}
            recognized, {review.audio.failed} failed.
          </p>
          {!imported?.copyComplete && (
            <p role="status">
              Partial Import: some source content could not be copied. Keep the
              source to retry individual failures; usable copied Samples remain
              available.
            </p>
          )}
          {retired.length > 0 && (
            <div>
              <strong>Recognized Samples in Trash</strong>
              <p>
                These identities are reused by the imported Layers. Restore to
                make them available in Library searches.
              </p>
              <ul>
                {retired.map((sample) => (
                  <li key={sample.id}>
                    {sample.name}{" "}
                    <button
                      disabled={busy}
                      aria-label={`Restore Sample: ${sample.name}`}
                      onClick={() => void restore(sample.id)}
                    >
                      Restore Sample
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          )}
          <p>
            {review.draft.kits.length} Kits reconstructed. All usable audio is
            kept, including Samples with invalid assignments and excess Layers.
          </p>
          <NativeSummary
            reading={imported?.native ?? null}
            preserved={nativeFiles.length}
          />
          {nativeFiles.length > 0 && (
            <details>
              <summary>Preserved native files</summary>
              <ul>
                {nativeFiles.map((file) => (
                  <li key={file.path}>{file.path}</li>
                ))}
              </ul>
            </details>
          )}
          {(imported?.issues.length ?? 0) > 0 && (
            <div role="status">
              <strong>Draft issues</strong>
              <ul>
                {imported?.issues.map((issue, i) => (
                  <li key={i}>{issue}</li>
                ))}
              </ul>
            </div>
          )}
          {review.audio.failed > 0 && (
            <div>
              <strong>Individual failures</strong>
              <ul>
                {review.audio.results
                  .filter((result) => result.error)
                  .map((result, i) => (
                    <li key={i}>
                      {result.source}: {result.error}
                    </li>
                  ))}
              </ul>
            </div>
          )}
          {review.status === "recognized" ? (
            <>
              <p>
                This unchanged arrangement and native state already belong to{" "}
                {review.candidates.find((p) => p.id === review.recognizedPatch)
                  ?.name ?? "an existing Patch"}
                .
              </p>
              <button
                disabled={busy}
                onClick={() => void finish({ type: "reuse" })}
              >
                Open existing Patch
              </button>
            </>
          ) : (
            <fieldset disabled={busy}>
              <p>
                {review.status === "changed"
                  ? "This arrangement or native state differs from previous Imports. Choose where to keep this draft."
                  : "Choose a new Patch or explicitly update an existing Patch."}
              </p>
              <label>
                New Patch name
                <input value={name} onChange={(e) => setName(e.target.value)} />
              </label>
              <button
                disabled={!name.trim()}
                onClick={() => void finish({ type: "new", name })}
              >
                Create new Patch
              </button>
              {review.candidates.length > 0 && (
                <>
                  <label>
                    Patch to update
                    <select
                      value={existing}
                      onChange={(e) => setExisting(e.target.value)}
                    >
                      {review.candidates.map((patch) => (
                        <option key={patch.id} value={patch.id}>
                          {patch.name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <p>
                    Update replaces its arrangement and imported native content.
                    Its name, tempo, pattern and mixer memory are retained.
                  </p>
                  <button
                    disabled={!existing}
                    onClick={() =>
                      void finish({ type: "update", patch: existing })
                    }
                  >
                    Update existing Patch
                  </button>
                </>
              )}
            </fieldset>
          )}
          <p>
            Copied Samples remain in the Library if you close this review. Draft
            issues must be resolved before a future Build Patch.
          </p>
        </>
      )}
      <button disabled={busy} onClick={onClose}>
        Close
      </button>
    </Modal>
  );
}

function NativeSummary({
  reading,
  preserved,
}: {
  reading: NativeReading | null;
  preserved: number;
}) {
  if (!reading)
    return (
      <p>
        Native settings were not read. {preserved} native file(s) preserved
        exactly; the app never writes them.
      </p>
    );
  const count = (source: string) =>
    reading.kits.filter((kit) => kit.source === source).length;
  const fallback =
    modes.find((mode) => mode.id === reading.defaultLayerMode)?.title ??
    reading.defaultLayerMode;
  const issues = reading.kits.flatMap((kit) => kit.issues);
  return (
    <div>
      <p>
        Native settings: Layer modes and selected Layers restored for{" "}
        {count("store")} Kit(s) saved with STORE (firmware{" "}
        {reading.verifiedFirmware}). {count("appDefault")} Kit(s) never saved
        with STORE use your default Layer mode ({fallback}).
      </p>
      <p>
        {preserved} native file(s) preserved exactly. The app never writes
        native settings: Build Patch copies audio only, so set modes on the
        Rample and STORE to keep them there.
      </p>
      {issues.length > 0 && (
        <div role="status">
          <strong>Native settings shown with your default Layer mode</strong>
          <ul>
            {issues.map((issue, i) => (
              <li key={i}>{issue}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
