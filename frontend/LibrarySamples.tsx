import { useEffect, useRef, useState } from "react";
import { Modal } from "./Modal";
import * as native from "./native";
import { basename, type LibrarySample, type LibraryImportReply } from "./types";

/** Optional picker returns a Sample ID; display names are never identities. */
export function LibrarySamples({
  onClose,
  onSelect,
}: {
  onClose: () => void;
  onSelect?: (sampleId: string) => void;
}) {
  const [samples, setSamples] = useState<LibrarySample[]>([]);
  const [query, setQuery] = useState("");
  const [matches, setMatches] = useState<LibrarySample[]>([]);
  const [searching, setSearching] = useState(false);
  const [revision, setRevision] = useState(0);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
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
  useEffect(() => {
    if (!query.trim()) {
      setSearching(false);
      return;
    }
    let cancelled = false;
    setSearching(true);
    setMatches([]);
    void native
      .querySamples(query)
      .then((rows) => {
        if (!cancelled) setMatches(rows);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setSearching(false);
      });
    return () => {
      cancelled = true;
    };
  }, [query, revision]);
  const visibleSamples = query.trim() ? matches : samples;
  const selected = samples.find((sample) => sample.id === selectedId);
  async function saveMetadata(name: string, tags: string[]) {
    if (!selectedId) return;
    setBusy(true);
    setSaved(false);
    setError(null);
    try {
      const sample = await native.updateSampleMetadata(selectedId, name, tags);
      if (mounted.current) {
        setSamples((rows) =>
          rows.map((row) => (row.id === sample.id ? sample : row)),
        );
        setRevision((value) => value + 1);
        setSaved(true);
      }
    } catch (e) {
      if (mounted.current) setError(String(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
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
        setRevision((value) => value + 1);
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
      <label className="library-search">
        Search Samples by name or tag
        <input
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
      </label>
      {searching && <p role="status">Searching Samples…</p>}
      {!busy && !searching && samples.length === 0 && <p>No Samples yet.</p>}
      {!busy &&
        !searching &&
        samples.length > 0 &&
        visibleSamples.length === 0 && <p>No matching Samples.</p>}
      {visibleSamples.length > 0 && (
        <ul className="library-samples" aria-label="Library Samples">
          {visibleSamples.map((sample) => (
            <li key={sample.id}>
              <div>
                <button
                  disabled={busy}
                  aria-label={`Select ${sample.name}`}
                  aria-pressed={selectedId === sample.id}
                  onClick={() => {
                    setSelectedId(sample.id);
                    setSaved(false);
                    setError(null);
                  }}
                >
                  {sample.name}
                </button>
                {sample.tags.length > 0 && (
                  <small>{sample.tags.join(" · ")}</small>
                )}
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
      {selected && (
        <SampleMetadata
          key={selected.id}
          sample={selected}
          busy={busy}
          onSave={saveMetadata}
        />
      )}
      {saved && <p role="status">Metadata saved.</p>}
      {error && <p role="alert">{error}</p>}
      <div className="modal-buttons">
        <button disabled={!auditioning} onClick={() => void audition(null)}>
          Stop audition
        </button>
        {onSelect && (
          <button
            disabled={busy || !selected?.readable}
            onClick={() => {
              if (selectedId) onSelect(selectedId);
            }}
          >
            Use Sample
          </button>
        )}
        <button disabled={busy} onClick={onClose}>
          Close
        </button>
      </div>
    </Modal>
  );
}

function SampleMetadata({
  sample,
  busy,
  onSave,
}: {
  sample: LibrarySample;
  busy: boolean;
  onSave: (name: string, tags: string[]) => Promise<void>;
}) {
  const [name, setName] = useState(sample.name);
  const [tags, setTags] = useState(sample.tags.join("\n"));
  return (
    <form
      className="library-metadata"
      aria-label="Sample metadata"
      onSubmit={(event) => {
        event.preventDefault();
        void onSave(
          name,
          tags.split(/\r?\n/).filter((tag) => tag.length > 0),
        );
      }}
    >
      <label>
        Sample name
        <input
          value={name}
          disabled={busy}
          onChange={(event) => setName(event.target.value)}
          required
        />
      </label>
      <label>
        Tags (one per line)
        <textarea
          value={tags}
          disabled={busy}
          onChange={(event) => setTags(event.target.value)}
        />
      </label>
      <button disabled={busy || !name.trim()} type="submit">
        Save metadata
      </button>
    </form>
  );
}
