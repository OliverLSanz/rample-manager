import { useEffect, useRef, useState } from "react";
import { Modal } from "./Modal";
import * as native from "./native";
import { basename, type LibrarySample, type LibraryImportReply } from "./types";

export function LibrarySamples({ onClose }: { onClose: () => void }) {
  const [samples, setSamples] = useState<LibrarySample[]>([]);
  const [report, setReport] = useState<LibraryImportReply | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(true);
  const [auditioning, setAuditioning] = useState<string | null>(null);
  const mounted = useRef(true);
  const auditionQueue = useRef(Promise.resolve());
  const owner = useRef<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    let session: string | null = null;
    mounted.current = true;
    void (async () => {
      try {
        session = await native.openSampleAudition();
        if (cancelled) {
          await native.stopSampleAudition(session, true);
          return;
        }
        owner.current = session;
        const rows = await native.listSamples();
        if (!cancelled) setSamples(rows);
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setBusy(false);
      }
    })();
    return () => {
      cancelled = true;
      mounted.current = false;
      owner.current = null;
      if (session)
        void native.stopSampleAudition(session, true).catch(() => {});
    };
  }, []);
  async function importSources(folder: boolean) {
    setBusy(true);
    setError(null);
    try {
      const paths = await native.chooseSampleSources(folder);
      if (!paths.length) return;
      const reply = await native.importSamples(paths);
      if (mounted.current) {
        setSamples(reply.samples);
        setReport(reply);
      }
    } catch (e) {
      if (mounted.current) setError(String(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  async function audition(id: string | null) {
    const task = auditionQueue.current
      .catch(() => {})
      .then(async () => {
        if (!mounted.current || !owner.current) return;
        if (id) await native.auditionSample(id, owner.current);
        else await native.stopSampleAudition(owner.current);
      });
    auditionQueue.current = task;
    try {
      await task;
      if (mounted.current) setAuditioning(id);
    } catch (e) {
      if (mounted.current) setError(String(e));
    }
  }
  return (
    <Modal
      label="Library samples"
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <h2>Library samples</h2>
      <p>
        Import mono PCM WAV files into your private Library. Originals are
        preserved exactly; completed imports no longer need the source folder.
      </p>
      <div className="modal-buttons">
        <button disabled={busy} onClick={() => void importSources(false)}>
          Import WAV files…
        </button>
        <button disabled={busy} onClick={() => void importSources(true)}>
          Import folder…
        </button>
      </div>
      {busy && <p role="status">Loading Samples…</p>}
      {report && (
        <>
          <p role="status">
            {report.imported} imported · {report.recognized} recognized ·{" "}
            {report.failed} failed{!report.complete && " — Import incomplete"}
          </p>
          <ul className="library-import-results" aria-label="Import results">
            {report.results.map((result, i) => (
              <li key={i}>
                {basename(result.source)}:{" "}
                {result.error ??
                  (result.status === "recognized"
                    ? "Already in Library"
                    : "Imported")}
              </li>
            ))}
          </ul>
        </>
      )}
      {!busy && samples.length === 0 && <p>No Samples yet.</p>}
      {samples.length > 0 && (
        <ul className="library-samples" aria-label="Library Samples">
          {samples.map((sample) => (
            <li key={sample.id}>
              <div>
                <strong>{sample.name}</strong>
                <small>
                  {sample.duration.toFixed(2)} s · {sample.rate} Hz ·{" "}
                  {sample.bits}-bit mono
                </small>
                {sample.error && <span role="alert">{sample.error}</span>}
              </div>
              <button
                disabled={busy || !sample.readable}
                aria-label={`Audition ${sample.name}`}
                onClick={() => void audition(sample.id)}
              >
                Audition
              </button>
            </li>
          ))}
        </ul>
      )}
      {error && <p role="alert">{error}</p>}
      <div className="modal-buttons">
        <button disabled={!auditioning} onClick={() => void audition(null)}>
          Stop audition
        </button>
        <button disabled={busy} onClick={onClose}>
          Close
        </button>
      </div>
    </Modal>
  );
}
