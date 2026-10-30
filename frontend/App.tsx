import {
  memo,
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import {
  Activity,
  ArrowDownUp,
  AudioLines,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Folder,
  FolderOpen,
  FolderPlus,
  Grid2X2,
  HardDrive,
  Keyboard,
  Layers,
  LoaderCircle,
  Pause,
  Play,
  RefreshCw,
  Search,
  Settings2,
  SlidersHorizontal,
  TriangleAlert,
  Upload,
  X,
} from "lucide-react";
import * as native from "./native";
import { Modal } from "./Modal";
import { SampleEditor } from "./SampleEditor";
import { LibrarySamples } from "./LibrarySamples";
import { PatchEditor } from "./PatchEditor";
import { SequencerRow, TransportBar } from "./Sequencer";
import {
  basename,
  compatibilityWarnings,
  displaySample,
  idleTransport,
  modes,
  sourceSamples,
  type Action,
  type Kit,
  type Sample,
  type Snapshot,
  type Transport,
  type Voice,
  type ReviewedFile,
  type ImportCandidate,
  type ConversionReply,
  type EditorInfo,
} from "./types";
import "./style.css";
const colors = ["#f68061", "#83c9ad", "#afa0df", "#dfc275"];
const isMac = /Mac/i.test(navigator.platform);
const modifier = isMac ? "⌘" : "Ctrl+";
function errorText(error: unknown) {
  return typeof error === "string"
    ? error
    : error instanceof Error
      ? error.message
      : String(error);
}

export default function App() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [transport, setTransport] = useState<Transport>(idleTransport);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [search, setSearch] = useState("");
  const [bank, setBank] = useState("All");
  const [exporting, setExporting] = useState(false);
  const [reviewing, setReviewing] = useState(false);
  const [librarySamples, setLibrarySamples] = useState(false);
  const [patchEditor, setPatchEditor] = useState(false);
  const [editor, setEditor] = useState<EditorInfo | null>(null);
  const [velocity, setVelocity] = useState<{
    channel: number;
    step: number;
  } | null>(null);
  const serial = useRef(Promise.resolve());
  const alive = useRef(true);
  const pending = useRef(0);
  const latestLibrary = useRef<Snapshot | null>(null);
  const action = useCallback((a: Action) => {
    pending.current++;
    if (
      [
        "root",
        "demo",
        "selectKit",
        "selectSource",
        "selectSample",
        "setMode",
        "refresh",
      ].includes(a.type)
    )
      setBusy(true);
    const task = serial.current
      .catch(() => {})
      .then(() => native.act(a))
      .then((result) => {
        if (alive.current)
          setSnapshot((previous) =>
            !previous || result.revision >= previous.revision
              ? result
              : previous,
          );
      })
      .catch((e) => {
        if (alive.current) setError(errorText(e));
      })
      .finally(() => {
        pending.current--;
        if (pending.current === 0) {
          setBusy(false);
          if (latestLibrary.current) {
            const latest = latestLibrary.current;
            setSnapshot((previous) =>
              !previous || latest.revision >= previous.revision
                ? latest
                : previous,
            );
            latestLibrary.current = null;
          }
        }
      });
    serial.current = task;
    return task;
  }, []);
  useEffect(() => {
    alive.current = true;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    native
      .onLibraryChange((s) => {
        if (pending.current) {
          latestLibrary.current = s;
        } else
          setSnapshot((previous) =>
            !previous || s.revision >= previous.revision ? s : previous,
          );
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch((e) => setError(errorText(e)));
    native
      .getSnapshot()
      .then((s) => {
        if (!cancelled)
          setSnapshot((previous) =>
            !previous || s.revision >= previous.revision ? s : previous,
          );
      })
      .catch((e) => {
        if (!cancelled) setError(errorText(e));
      });
    let polling = false;
    const timer = setInterval(() => {
      if (polling) return;
      polling = true;
      native
        .getTransport()
        .then((t) => {
          if (!cancelled) setTransport(t);
        })
        .catch(() => {})
        .finally(() => {
          polling = false;
        });
    }, 40);
    return () => {
      cancelled = true;
      alive.current = false;
      clearInterval(timer);
      unlisten?.();
    };
  }, []);
  useEffect(() => {
    if (
      snapshot?.imports.length &&
      !exporting &&
      !velocity &&
      !reviewing &&
      !librarySamples &&
      !patchEditor &&
      !editor
    )
      setReviewing(true);
  }, [
    snapshot?.imports.length,
    exporting,
    velocity,
    reviewing,
    editor,
    librarySamples,
    patchEditor,
  ]);
  const convertImports = useCallback(async (files: ReviewedFile[]) => {
    await serial.current;
    const result = await native.convertImports(files);
    if (alive.current)
      setSnapshot((previous) =>
        !previous || result.snapshot.revision >= previous.revision
          ? result.snapshot
          : previous,
      );
    return result;
  }, []);
  const dismissImports = useCallback(
    async (files: ReviewedFile[]) => {
      await action({ type: "dismissImports", files });
      setReviewing(false);
    },
    [action],
  );
  const openFolder = useCallback(async () => {
    try {
      const path = await native.chooseFolder();
      if (path) {
        setSearch("");
        setBank("All");
        await action({ type: "root", path });
      }
    } catch (e) {
      setError(errorText(e));
    }
  }, [action]);
  useEffect(() => {
    function keydown(e: KeyboardEvent) {
      if (
        exporting ||
        velocity ||
        reviewing ||
        editor ||
        librarySamples ||
        patchEditor
      )
        return;
      const target = e.target;
      if ((e.metaKey || e.ctrlKey) && !e.altKey) {
        if (e.key.toLowerCase() === "o") {
          e.preventDefault();
          void openFolder();
        } else if (e.key.toLowerCase() === "r") {
          e.preventDefault();
          void action({ type: "refresh" });
        } else if (e.key.toLowerCase() === "e" && e.shiftKey) {
          e.preventDefault();
          setExporting(true);
        }
        return;
      }
      if (
        target instanceof Element &&
        target.closest('input,textarea,select,[contenteditable="true"]')
      )
        return;
      if (e.repeat || e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.code === "Space") {
        e.preventDefault();
        void action({ type: "togglePlayback" });
      } else if (/^[1-4]$/.test(e.key)) {
        e.preventDefault();
        void action({ type: "audition", channel: Number(e.key) - 1 });
      }
    }
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  }, [
    action,
    openFolder,
    exporting,
    velocity,
    reviewing,
    editor,
    librarySamples,
    patchEditor,
  ]);
  const reveal = useCallback((channel?: number) => {
    native.reveal(channel).catch((e) => setError(errorText(e)));
  }, []);
  const openEditor = useCallback(async (channel: number) => {
    setBusy(true);
    try {
      await serial.current;
      setEditor(await native.openEditor(channel));
      setSnapshot((previous) =>
        previous ? { ...previous, editorInvalidated: false } : previous,
      );
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }, []);
  if (!snapshot)
    return (
      <div className="boot">
        <AudioLines size={48} />
        <h1>Rample Preview</h1>
        {error ? (
          <>
            <p role="alert">{error}</p>
            <button onClick={() => location.reload()}>Retry</button>
          </>
        ) : (
          <>
            <LoaderCircle className="spin" />
            <p>Opening your sample library…</p>
          </>
        )}
      </div>
    );
  const { session, catalog } = snapshot;
  const banks = [...new Set(catalog.kits.map((k) => k.bank))];
  const filtered = catalog.kits.filter(
    (k) =>
      (bank === "All" || k.bank === bank) &&
      (!search ||
        k.id.toLowerCase().includes(search.toLowerCase()) ||
        k.samples.some((s) =>
          s.name.toLowerCase().includes(search.toLowerCase()),
        )),
  );
  const warnings = compatibilityWarnings(snapshot);
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="sidebar-drag" data-tauri-drag-region />
        <div className="brand">
          <AudioLines size={31} />
          <div>
            <strong>RAMPLE</strong>
            <span>P R E V I E W</span>
          </div>
          <small>2.3</small>
        </div>
        <div className="library-heading">
          <span>LIBRARY</span>
          <span>{catalog.kits.length}</span>
        </div>
        <label className="search">
          <Search size={14} />
          <input
            aria-label="Search kits or samples"
            placeholder="Search kits or samples"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          {search && (
            <button aria-label="Clear search" onClick={() => setSearch("")}>
              <X size={12} />
            </button>
          )}
        </label>
        <label className="bank-filter">
          <Layers size={14} />
          <select
            aria-label="Filter banks"
            value={banks.includes(bank) ? bank : "All"}
            onChange={(e) => setBank(e.target.value)}
          >
            <option value="All">All banks</option>
            {banks.map((b) => (
              <option key={b} value={b}>
                Bank {b}
              </option>
            ))}
          </select>
          <ChevronDown size={11} />
        </label>
        <nav aria-label="Sample kits" className="kit-list">
          {filtered.map((k) => (
            <button
              key={k.id}
              className={`kit ${session.selectedKit === k.id ? "selected" : ""}`}
              aria-label={`Load kit ${k.id}`}
              aria-current={session.selectedKit === k.id ? "true" : undefined}
              onClick={() => void action({ type: "selectKit", kit: k.id })}
            >
              <b>{k.id}</b>
              <span>
                <strong>Kit {k.id}</strong>
                <small>{k.samples.length} samples</small>
              </span>
              {session.selectedKit === k.id ? (
                <i />
              ) : k.warnings.length > 0 ? (
                <TriangleAlert size={11} />
              ) : null}
            </button>
          ))}
          {!filtered.length && (
            <p className="no-matches">
              {search ? "No matches" : "No kits in this bank"}
            </p>
          )}
        </nav>
        <div className="sidebar-bottom">
          <button
            className="folder-button"
            onClick={() => setPatchEditor(true)}
          >
            <Layers size={15} /> Patches…
          </button>
          <button
            className="folder-button"
            onClick={() => setLibrarySamples(true)}
          >
            <AudioLines size={15} /> Library samples…
          </button>
          {snapshot.isDemo && (
            <div className="demo-note">
              <span>
                <Activity size={13} /> Demo library
              </span>
              <p>
                Four sounds to get started.
                <br />
                Open your folder to hear your own samples.
              </p>
            </div>
          )}
          <button className="folder-button" onClick={() => void openFolder()}>
            <FolderPlus size={15} />
            Open folder…
          </button>
          <small>LOCAL · OFFLINE</small>
        </div>
      </aside>
      <main>
        <header className="topbar" data-tauri-drag-region>
          <div className="folder-path">
            <HardDrive size={16} />
            <button title={session.rootPath} onClick={() => reveal()}>
              {basename(session.rootPath)}
            </button>
            {snapshot.isDemo && <span className="badge">DEMO</span>}
          </div>
          <div className="header-actions">
            <span className={`live-state ${catalog.error ? "warning" : ""}`}>
              <i />
              {busy
                ? "Loading…"
                : catalog.error
                  ? "Library offline"
                  : snapshot.watching
                    ? "Live updates"
                    : "Auto-refresh · 4 s"}
            </span>
            <button
              className={`icon-button ${busy ? "refreshing" : ""}`}
              title={`Refresh files (${modifier}R)`}
              aria-label="Refresh files"
              onClick={() => void action({ type: "refresh" })}
            >
              <RefreshCw size={15} />
            </button>
            <button
              className="format-button"
              aria-label="Check sample formats"
              title="Review sample formats"
              onClick={() => void action({ type: "reviewFormats" })}
            >
              <TriangleAlert size={16} />
              <span>Check formats</span>
            </button>
            <span className="divider" />
            <button
              className="secondary export-button"
              onClick={() => setExporting(true)}
            >
              <Upload size={14} />
              Export kit
            </button>
          </div>
        </header>
        <div className="workspace">
          <section className="kit-heading">
            <div className="kit-code">{session.selectedKit || "—"}</div>
            <div>
              <h1>Your kit. In context.</h1>
              <p>
                Choose a layer for each voice and hear how your samples fit
                together.
              </p>
            </div>
            {busy && (
              <LoaderCircle
                size={17}
                className="spin loading-indicator"
                aria-label="Loading samples"
              />
            )}
          </section>
          {catalog.error && (
            <div className="notice" role="alert">
              <TriangleAlert size={17} />
              <span>{catalog.error}</span>
            </div>
          )}
          {!catalog.kits.length && !catalog.error && (
            <div className="notice">
              <Folder size={17} />
              <span>
                No kits found. Choose the root folder containing A0, A1… Z99.
                WAV filenames inside each kit must start with 1, 2, 3 or 4.
              </span>
            </div>
          )}
          <section className="sample-cards" aria-label="Sample voices">
            {session.voices.map((voice, i) => (
              <SampleCard
                key={i}
                channel={i}
                voice={voice}
                kits={catalog.kits}
                sample={displaySample(snapshot, i, transport.activeLayers[i])}
                samples={sourceSamples(snapshot, i)}
                peaks={
                  snapshot.waveforms[
                    displaySample(snapshot, i, transport.activeLayers[i])?.id ??
                      ""
                  ]
                }
                layerCount={snapshot.banks[i]?.length ?? 0}
                activeIndex={transport.activeLayers[i]}
                onAction={action}
                onReveal={reveal}
                onEdit={openEditor}
              />
            ))}
          </section>
          <section className="sequencer" aria-label="16-step sequencer">
            <div className="sequencer-heading">
              <Grid2X2 size={15} />
              <h2>Sequencer</h2>
              <span>16 STEPS · 4/4 · 1/16</span>
              <div className="pattern-menu">
                <Settings2 size={14} />
                <select
                  aria-label="Pattern actions"
                  value=""
                  onChange={(e) => {
                    if (e.target.value)
                      void action({
                        type: e.target.value as "basicPattern" | "clearPattern",
                      });
                  }}
                >
                  <option value="" disabled>
                    Pattern
                  </option>
                  <option value="basicPattern">Load example beat</option>
                  <option value="clearPattern">Clear all 64 steps</option>
                </select>
                <ChevronDown size={11} />
              </div>
            </div>
            <div className="step-grid">
              <div className="step-numbers">
                <span>VOICE / LEVEL</span>
                <div className="steps">
                  {Array.from({ length: 16 }, (_, i) => (
                    <span
                      className={`${i % 4 === 0 ? "beat" : ""} ${transport.step === i ? "current" : ""}`}
                      key={i}
                    >
                      {String(i + 1).padStart(2, "0")}
                    </span>
                  ))}
                </div>
              </div>
              {session.voices.map((v, i) => (
                <SequencerRow
                  key={i}
                  channel={i}
                  voice={v}
                  playhead={transport.running ? transport.step : -1}
                  onAction={action}
                  onVelocity={setVelocity}
                />
              ))}
              <div className="position-row">
                <span />
                <div className="steps">
                  {Array.from({ length: 16 }, (_, i) => (
                    <i
                      key={i}
                      className={
                        transport.running && transport.step === i
                          ? "current"
                          : ""
                      }
                    />
                  ))}
                </div>
              </div>
            </div>
            <TransportBar
              playing={snapshot.playing}
              bpm={session.bpm}
              transport={transport}
              onAction={action}
            />
          </section>
          {transport.error && (
            <div className="notice" role="alert">
              <TriangleAlert size={17} />
              <span>{transport.error}</span>
            </div>
          )}
          {warnings.length > 0 && (
            <details className="compatibility">
              <summary>
                <TriangleAlert size={13} />
                {warnings.length} Rample compatibility{" "}
                {warnings.length === 1 ? "warning" : "warnings"}
                <ChevronDown size={12} />
              </summary>
              <ul>
                {warnings.map((w) => (
                  <li key={w}>{w}</li>
                ))}
              </ul>
            </details>
          )}
          <footer>
            <div>
              <Keyboard size={13} />
              <span>Space: play / stop</span>
              <b>·</b>
              <span>1–4: audition voice</span>
              <b>·</b>
              <span>{modifier}R: refresh</span>
            </div>
            <span title="Layer modes are stored on this computer. Native Rample .rpl settings are not changed.">
              {snapshot.message ||
                "Modes saved on this computer · native SD sync pending"}
            </span>
          </footer>
        </div>
      </main>
      {error && (
        <div className="toast" role="alert">
          <TriangleAlert size={17} />
          <span>{error}</span>
          <button aria-label="Dismiss error" onClick={() => setError(null)}>
            <X size={15} />
          </button>
        </div>
      )}
      {patchEditor && (
        <PatchEditor
          transport={transport}
          onClose={() => {
            setPatchEditor(false);
            void native
              .getSnapshot()
              .then(setSnapshot)
              .catch((e) => setError(errorText(e)));
          }}
        />
      )}
      {librarySamples && (
        <LibrarySamples onClose={() => setLibrarySamples(false)} />
      )}
      {editor && (
        <SampleEditor
          initial={editor}
          transport={transport}
          invalidated={!!snapshot.editorInvalidated}
          onClose={() => setEditor(null)}
          onSaved={(result) =>
            setSnapshot((previous) =>
              !previous || result.revision >= previous.revision
                ? result
                : previous,
            )
          }
          onToggle={() => action({ type: "togglePlayback" })}
        />
      )}
      {reviewing && !exporting && !velocity && (
        <ImportDialog
          items={snapshot.imports}
          backupPath={`${snapshot.supportPath}/Conversion backups`}
          onConvert={convertImports}
          onDismiss={dismissImports}
        />
      )}
      {exporting && (
        <ExportDialog
          onClose={() => setExporting(false)}
          onError={(e) => setError(errorText(e))}
          onExported={() => void action({ type: "refresh" })}
        />
      )}
      {velocity && (
        <VelocityDialog
          channel={velocity.channel}
          step={velocity.step}
          initial={session.voices[velocity.channel].velocities[velocity.step]}
          onClose={() => setVelocity(null)}
          onChange={(value) =>
            void action({ type: "setVelocity", ...velocity, value })
          }
        />
      )}
    </div>
  );
}

const Waveform = memo(function Waveform({
  peaks,
  color,
}: {
  peaks?: number[];
  color: string;
}) {
  const path =
    peaks
      ?.map((p, i) => {
        const y = Math.max(1, p * 55);
        return `M${i * 3 + 1},${(60 - y) / 2}v${y}`;
      })
      .join(" ") ?? "";
  return (
    <svg
      className="waveform"
      viewBox="0 0 300 60"
      preserveAspectRatio="none"
      role="img"
      aria-label="Waveform"
    >
      <path d="M0 30H300" stroke={color} strokeOpacity=".13" />
      <path
        d={path}
        stroke={color}
        strokeWidth="1.7"
        strokeLinecap="round"
        opacity=".75"
      />
    </svg>
  );
});
const SampleCard = memo(function SampleCard({
  channel,
  voice,
  kits,
  sample,
  samples,
  peaks,
  layerCount,
  activeIndex,
  onAction,
  onReveal,
  onEdit,
}: {
  channel: number;
  voice: Voice;
  kits: Kit[];
  sample?: Sample;
  samples: Sample[];
  peaks?: number[];
  layerCount: number;
  activeIndex: number;
  onAction: (a: Action) => Promise<void>;
  onReveal: (i: number) => void;
  onEdit: (i: number) => Promise<void>;
}) {
  const selectedExists = samples.some((s) => s.id === voice.samplePath);
  const missing = !!voice.samplePath && !selectedExists;
  const mode = modes.find((m) => m.id === voice.layerMode)!;
  return (
    <article
      className="sample-card"
      style={{ "--voice": colors[channel] } as CSSProperties}
    >
      <div className="voice-heading">
        <h2>
          <i />
          VOICE {String(channel + 1).padStart(2, "0")}
        </h2>
        <label className="source-select">
          <select
            aria-label={`Source kit for voice ${channel + 1}`}
            value={kits.some((k) => k.id === voice.kitID) ? voice.kitID : ""}
            onChange={(e) =>
              void onAction({
                type: "selectSource",
                channel,
                kit: e.target.value,
              })
            }
          >
            <option value="" disabled>
              Kit
            </option>
            {kits.map((k) => (
              <option key={k.id} value={k.id}>
                {k.id}
              </option>
            ))}
          </select>
          <ChevronDown size={10} />
        </label>
      </div>
      <label
        className={`sample-select ${missing ? "missing" : ""}`}
        title={voice.samplePath || "Select a sample"}
      >
        <ArrowDownUp size={14} />
        <select
          aria-label={`Sample for voice ${channel + 1}`}
          value={voice.samplePath}
          onChange={(e) =>
            void onAction({
              type: "selectSample",
              channel,
              path: e.target.value,
            })
          }
        >
          <option value="">No sample</option>
          {missing && (
            <option value={voice.samplePath} disabled>
              Sample deleted
            </option>
          )}
          {samples.map((s) => (
            <option key={s.id} value={s.id} disabled={!s.readable}>
              {s.name}
              {s.warnings.length ? " ⚠" : ""}
            </option>
          ))}
        </select>
      </label>
      <div className="mode-row">
        <label title={mode.description}>
          <Layers size={11} />
          <select
            aria-label={`Layer mode for voice ${channel + 1}`}
            value={voice.layerMode}
            onChange={(e) =>
              void onAction({
                type: "setMode",
                channel,
                mode: e.target.value as Voice["layerMode"],
              })
            }
          >
            {modes.map((m) => (
              <option key={m.id} value={m.id}>
                {m.title}
              </option>
            ))}
          </select>
          <ChevronDown size={9} />
        </label>
        {voice.layerMode !== "manual" && (
          <span>
            {activeIndex >= 0 ? activeIndex + 1 : "–"} / {layerCount}
          </span>
        )}
      </div>
      <Waveform peaks={peaks} color={colors[channel]} />
      <div
        className="sample-detail"
        title={
          sample
            ? `${sample.name} · ${(sample.rate / 1000).toFixed(1)} kHz · ${sample.bits} bit`
            : ""
        }
      >
        {sample ? (
          <>
            <span>{sample.duration.toFixed(2)} s</span>
            <small>{sample.channels === 1 ? "MONO" : "STEREO"}</small>
            {sample.warnings.length > 0 && (
              <TriangleAlert size={12} className="warning" />
            )}
          </>
        ) : (
          <span>
            {missing
              ? "Choose another file or restore it"
              : "No layers in this voice"}
          </span>
        )}
      </div>
      <div className="sample-actions">
        <button
          disabled={layerCount === 0}
          onClick={() => void onAction({ type: "audition", channel })}
          aria-label={`Audition voice ${channel + 1}`}
        >
          <Play size={10} fill="currentColor" />
          Audition<kbd>{channel + 1}</kbd>
        </button>
        <button
          className="icon-button"
          disabled={!selectedExists}
          title="Edit selected sample with live sequence preview"
          aria-label={`Edit sample for voice ${channel + 1}`}
          onClick={() => void onEdit(channel)}
        >
          <SlidersHorizontal size={14} />
        </button>
        <button
          className="icon-button"
          disabled={!sample}
          title="Show selected WAV in file manager"
          aria-label={`Show voice ${channel + 1} sample in file manager`}
          onClick={() => onReveal(channel)}
        >
          <Folder size={14} />
        </button>
      </div>
    </article>
  );
});
function VelocityDialog({
  channel,
  step,
  initial,
  onClose,
  onChange,
}: {
  channel: number;
  step: number;
  initial: number;
  onClose: () => void;
  onChange: (v: number) => void;
}) {
  const [value, setValue] = useState(initial);
  function change(v: number) {
    setValue(v);
    onChange(v);
  }
  return (
    <Modal label="Edit step velocity" onClose={onClose}>
      <div className="modal-heading">
        <Layers size={20} />
        <h2>
          Voice {channel + 1} · Step {step + 1}
        </h2>
        <button aria-label="Close velocity editor" onClick={onClose}>
          <X size={17} />
        </button>
      </div>
      <div className="velocity-value">
        <span>Velocity</span>
        <output>{value}</output>
      </div>
      <input
        type="range"
        aria-label="Step velocity"
        min="1"
        max="127"
        step="1"
        value={value}
        onChange={(e) => change(Number(e.target.value))}
      />
      <div className="presets">
        {[32, 64, 96, 127].map((n) => (
          <button key={n} onClick={() => change(n)}>
            {n}
          </button>
        ))}
      </div>
      <p>
        In Velocity mode, lower values select earlier layers. Velocity selects a
        layer; it does not change sample volume.
      </p>
      <div className="modal-buttons">
        <button className="primary" onClick={onClose}>
          Done
        </button>
      </div>
    </Modal>
  );
}
function ExportDialog({
  onClose,
  onError,
  onExported,
}: {
  onClose: () => void;
  onError: (e: unknown) => void;
  onExported: () => void;
}) {
  const [name, setName] = useState("B0");
  const [busy, setBusy] = useState(false);
  const valid = /^[A-Z][0-9]{1,2}$/.test(name);
  async function submit() {
    try {
      const parent = await native.chooseFolder(
        `Export ${name}: choose the destination SD root`,
      );
      if (!parent) return;
      setBusy(true);
      await native.exportKit(name, parent);
      onExported();
      onClose();
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal label="Export kit" onClose={onClose}>
      <div className="modal-heading">
        <Upload size={21} />
        <h2>Export selection</h2>
        <button aria-label="Close export dialog" onClick={onClose}>
          <X size={17} />
        </button>
      </div>
      <p>
        Create a kit with a copy of your selected samples. Choose your SD root
        as the destination to use it on Rample.
      </p>
      <label className="export-name">
        Kit name
        <input
          aria-label="Kit name"
          value={name}
          onChange={(e) => setName(e.target.value.toUpperCase())}
        />
        <small>A0–Z99</small>
      </label>
      <div className="export-note">
        <Check size={14} />
        <span>
          Compatible mono WAV · voice 1 required · existing kits stay untouched.
        </span>
      </div>
      <p className="small">
        This exports one selected sample per voice. Layer modes stay on this
        computer until native SD settings have been verified.
      </p>
      <div className="modal-buttons">
        <button className="secondary" onClick={onClose}>
          Cancel
        </button>
        <button
          className="primary"
          disabled={!valid || busy}
          onClick={() => void submit()}
        >
          {busy ? (
            <LoaderCircle className="spin" size={14} />
          ) : (
            <FolderOpen size={14} />
          )}
          Choose destination…
        </button>
      </div>
    </Modal>
  );
}

function ImportDialog({
  items,
  backupPath,
  onConvert,
  onDismiss,
}: {
  items: ImportCandidate[];
  backupPath: string;
  onConvert: (files: ReviewedFile[]) => Promise<ConversionReply>;
  onDismiss: (files: ReviewedFile[]) => Promise<void>;
}) {
  const [excluded, setExcluded] = useState<Set<string>>(() => new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reply, setReply] = useState<ConversionReply | null>(null);
  const reviewed = useRef<ReviewedFile[]>([]);
  const selected = items.filter(
    (item) => item.convertible && !excluded.has(item.id),
  );
  async function close() {
    if (busy) return;
    setBusy(true);
    try {
      await onDismiss(
        reply
          ? reviewed.current
          : items.map(({ id, fingerprint }) => ({ id, fingerprint })),
      );
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  async function convert() {
    if (!selected.length || busy) return;
    reviewed.current = items.map(({ id, fingerprint }) => ({
      id,
      fingerprint,
    }));
    setBusy(true);
    setError(null);
    try {
      setReply(
        await onConvert(
          selected.map(({ id, fingerprint }) => ({ id, fingerprint })),
        ),
      );
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal label="New samples detected" onClose={() => void close()}>
      <div className="modal-heading">
        <TriangleAlert size={21} />
        <h2>{reply ? "Conversion complete" : "New samples detected"}</h2>
        <button
          aria-label="Close sample review"
          disabled={busy}
          onClick={() => void close()}
        >
          <X size={17} />
        </button>
      </div>
      {reply ? (
        <>
          <p>
            {reply.results.filter((r) => !r.error).length} converted ·{" "}
            {reply.results.filter((r) => r.error).length} kept unchanged
          </p>
          <div className="import-list">
            {reply.results.map((r, i) => (
              <div className="import-result" key={i}>
                <strong>{r.name}</strong>
                <small className={r.error ? "import-error" : "import-success"}>
                  {r.error || `Saved as ${basename(r.target || "")}`}
                </small>
              </div>
            ))}
          </div>
          <p className="small">
            Originals and restore information are saved in {reply.backupPath}.
          </p>
          {reply.snapshot.message && (
            <p className="import-error" role="alert">
              {reply.snapshot.message}
            </p>
          )}
          <div className="modal-buttons">
            <button
              className="secondary"
              onClick={() =>
                void native.showBackups().catch((e) => setError(errorText(e)))
              }
            >
              Show backups
            </button>
            {reply.results.some((r) => r.error) && (
              <button
                className="secondary"
                onClick={() => {
                  setReply(null);
                  setExcluded(new Set());
                }}
              >
                Review remaining
              </button>
            )}
            <button
              className="primary"
              disabled={busy}
              onClick={() => void close()}
            >
              Done
            </button>
          </div>
        </>
      ) : (
        <>
          <p>
            These samples need conversion for a mono Rample kit. Convert the
            selected files and replace the originals?
          </p>
          <div className="conversion-target">
            MONO · WAV PCM · 16-bit · 44.1 kHz
          </div>
          <div className="import-list">
            {items.map((item) => (
              <label className="import-item" key={item.id}>
                <input
                  type="checkbox"
                  aria-label={`Convert ${item.kit}/${item.name}`}
                  disabled={busy || !item.convertible}
                  checked={item.convertible && !excluded.has(item.id)}
                  onChange={(e) =>
                    setExcluded((previous) => {
                      const next = new Set(previous);
                      if (e.target.checked) next.delete(item.id);
                      else next.add(item.id);
                      return next;
                    })
                  }
                />
                <div>
                  <strong title={item.id}>
                    {item.kit}/{item.name}
                  </strong>
                  {item.issues.map((issue, i) => (
                    <small key={i}>{issue}</small>
                  ))}
                  <small
                    className={
                      item.convertible ? "import-success" : "import-error"
                    }
                  >
                    {item.convertible
                      ? `Replace with ${item.target}`
                      : "Manual fix required · original will be kept"}
                  </small>
                </div>
              </label>
            ))}
          </div>
          <p className="small">
            All audio is converted to mono. Stereo channels are averaged (L + R)
            / 2; additional channels are averaged too. Short samples are padded
            with silence to reach 50 ms. Names and kit structure require manual
            changes.
          </p>
          <p className="small">
            Original files are backed up before replacement in {backupPath}.
          </p>
          <div className="modal-buttons">
            <button
              className="secondary"
              disabled={busy}
              onClick={() => void close()}
            >
              Keep originals
            </button>
            <button
              className="primary"
              disabled={busy || !selected.length}
              onClick={() => void convert()}
            >
              {busy && <LoaderCircle className="spin" size={14} />}
              {busy
                ? "Converting…"
                : `Convert and replace (${selected.length})`}
            </button>
          </div>
        </>
      )}
      {error && (
        <p className="import-error" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
