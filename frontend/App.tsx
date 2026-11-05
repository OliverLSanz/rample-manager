import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AudioLines,
  ChevronDown,
  DatabaseBackup,
  Disc3,
  Grid2X2,
  HardDriveDownload,
  History,
  Keyboard,
  Layers,
  LoaderCircle,
  Settings2,
  TriangleAlert,
  Upload,
  X,
} from "lucide-react";
import * as native from "./native";
import { Modal } from "./Modal";
import { SampleEditor } from "./SampleEditor";
import { SdImport } from "./SdImport";
import { LibrarySamples } from "./LibrarySamples";
import { LibrarySnapshots } from "./LibrarySnapshots";
import { LibraryMigration } from "./LibraryMigration";
import {
  BuildResult,
  BuildReviewPanel,
  DestinationBackups,
} from "./BuildPatch";
import { KitList } from "./KitList";
import { PatchMenu } from "./PatchMenu";
import { VoiceCard } from "./VoiceCard";
import { SequencerRow, TransportBar } from "./Sequencer";
import {
  currentKit,
  currentPatch,
  kitAddress,
  nextFreeAddress,
  patchIssues,
  voiceSource,
} from "./patchView";
import {
  idleTransport,
  type Action,
  type BuildReply,
  type BuildReview,
  type EditorInfo,
  type LayerMode,
  type LibrarySample,
  type PatchAction,
  type PatchSnapshot,
  type Transport,
} from "./types";
import "./style.css";

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

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

/** Shown while the Library has no Patch: every Kit lives in a Patch. */
function FirstPatch({
  onCreate,
  onImport,
}: {
  onCreate: (name: string) => Promise<boolean>;
  onImport: () => void;
}) {
  const [name, setName] = useState("My Patch");
  return (
    <section className="empty-state" aria-label="Start a Patch">
      <Disc3 size={34} />
      <h1>Start a Patch</h1>
      <p>
        A Patch is a complete Rample SD card: its Banks, Kits and the Layers of
        each Voice. Every Patch shares the same Library of Samples.
      </p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim()) void onCreate(name);
        }}
      >
        <input
          aria-label="New Patch name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button className="primary" disabled={!name.trim()}>
          Create Patch
        </button>
      </form>
      <button className="secondary" onClick={onImport}>
        <HardDriveDownload size={14} /> Import Rample SD…
      </button>
    </section>
  );
}

export default function App() {
  const [view, setView] = useState<PatchSnapshot | null>(null);
  // The folder session from before the managed Library: whether it has been
  // moved into the Library, and whether it is only the bundled demo.
  const [legacy, setLegacy] = useState({ managed: true, isDemo: false });
  const [transport, setTransport] = useState<Transport>(idleTransport);
  const [error, setError] = useState<string | null>(null);
  const [bootError, setBootError] = useState<string | null>(null);
  const [pending, setPending] = useState(0);
  const [sdImport, setSdImport] = useState(false);
  const [library, setLibrary] = useState(false);
  const [picker, setPicker] = useState<number | null>(null);
  const [snapshots, setSnapshots] = useState(false);
  const [editor, setEditor] = useState<EditorInfo | null>(null);
  const [velocity, setVelocity] = useState<{
    channel: number;
    step: number;
  } | null>(null);
  const [building, setBuilding] = useState<string | null>(null);
  const [review, setReview] = useState<BuildReview | null>(null);
  const [built, setBuilt] = useState<BuildReply | null>(null);
  const [backups, setBackups] = useState<{ focus: string | null } | null>(null);
  const [migrationHidden, setMigrationHidden] = useState(false);
  const alive = useRef(true);
  const queue = useRef(Promise.resolve());
  // Bumped when a Library restore replaces every Patch: older replies are stale.
  const requestVersion = useRef(0);

  /** Serializes Patch changes; resolves true when the backend accepted it. */
  const act = useCallback(async (action: PatchAction): Promise<boolean> => {
    const version = requestVersion.current;
    let success = false;
    setPending((n) => n + 1);
    const task = queue.current
      .catch(() => {})
      .then(async () => {
        if (version !== requestVersion.current) return;
        const result = await native.patchAction(action);
        if (alive.current && version === requestVersion.current) {
          setView(result);
          success = true;
        }
      });
    queue.current = task;
    try {
      await task;
    } catch (e) {
      if (alive.current && version === requestVersion.current)
        setError(errorText(e));
    } finally {
      if (alive.current) setPending((n) => n - 1);
    }
    return success;
  }, []);
  const musical = useCallback(
    async (action: Action) => {
      await act({ type: "musical", action });
    },
    [act],
  );
  const onPatch = useCallback((action: PatchAction) => void act(action), [act]);
  const onMode = useCallback(
    (channel: number, mode: LayerMode) =>
      void musical({ type: "setMode", channel, mode }),
    [musical],
  );
  const onAudition = useCallback(
    (channel: number) => void musical({ type: "audition", channel }),
    [musical],
  );
  const onEdit = useCallback((channel: number) => {
    setError(null);
    setPending((n) => n + 1);
    void queue.current
      .catch(() => {})
      .then(() => native.openManagedEditor(channel))
      .then((info) => {
        if (alive.current) setEditor(info);
      })
      .catch((e) => {
        if (alive.current) setError(errorText(e));
      })
      .finally(() => {
        if (alive.current) setPending((n) => n - 1);
      });
  }, []);

  useEffect(() => {
    alive.current = true;
    let cancelled = false;
    native
      .getSnapshot()
      .then((s) => {
        if (!cancelled) setLegacy({ managed: !!s.managed, isDemo: s.isDemo });
      })
      .catch(() => {});
    native
      .patchAction({ type: "open" })
      .then((result) => {
        if (!cancelled) setView(result);
      })
      .catch((e) => {
        if (!cancelled) setBootError(errorText(e));
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
    };
  }, []);

  const overlay =
    sdImport ||
    library ||
    picker !== null ||
    snapshots ||
    !!editor ||
    !!velocity;
  useEffect(() => {
    function key(e: KeyboardEvent) {
      if (overlay || !view) return;
      if (
        e.target instanceof Element &&
        e.target.closest(
          'button,a[href],input,textarea,select,[role="button"],[contenteditable="true"]',
        )
      )
        return;
      if (e.repeat || e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.code === "Space") {
        e.preventDefault();
        void musical({ type: "togglePlayback" });
      } else if (/^[1-4]$/.test(e.key)) {
        e.preventDefault();
        void musical({ type: "audition", channel: Number(e.key) - 1 });
      }
    }
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [overlay, view, musical]);

  const samples = useMemo(
    () => new Map<string, LibrarySample>(view?.samples.map((s) => [s.id, s])),
    [view?.samples],
  );
  const patch = currentPatch(view);
  const kit = currentKit(patch);
  const issues = useMemo(
    () => (view && patch ? patchIssues(view, patch) : []),
    [view, patch],
  );
  const busy = pending > 0;

  async function build() {
    if (!patch || busy || building) return;
    const destination = await native.chooseFolder(
      "Choose a folder or SD card for Build Patch",
    );
    if (!destination || !alive.current) return;
    setBuilding("Reviewing the destination…");
    setBuilt(null);
    setReview(null);
    setError(null);
    try {
      await queue.current.catch(() => {});
      const reviewed = await native.reviewPatchBuild(patch.id, destination);
      if (alive.current) setReview(reviewed);
    } catch (e) {
      if (alive.current) setError(errorText(e));
    } finally {
      if (alive.current) setBuilding(null);
    }
  }
  async function applyBuild() {
    if (!review || building) return;
    setBuilding("Building Patch…");
    setError(null);
    try {
      await queue.current.catch(() => {});
      const reply = await native.applyPatchBuild(review);
      if (alive.current) setBuilt(reply);
    } catch (e) {
      if (alive.current) setError(errorText(e));
    } finally {
      if (alive.current) {
        setReview(null);
        setBuilding(null);
      }
    }
  }

  if (!view)
    return (
      <div className="boot">
        <AudioLines size={48} />
        <h1>Rample Preview</h1>
        {bootError ? (
          <>
            <p role="alert">{bootError}</p>
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

  const session = view.session;
  const imported = patch?.importedSd;
  const report = [
    ...(imported?.issues ?? []),
    ...(imported?.unassigned.map(
      (file) => `Unassigned source occurrence: ${file.path}`,
    ) ?? []),
  ];
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
        <KitList
          patch={patch}
          issues={issues}
          samples={samples}
          onPatch={act}
        />
        <div className="sidebar-bottom">
          <button className="folder-button" onClick={() => setLibrary(true)}>
            <AudioLines size={15} /> Library samples…
          </button>
          <button
            className="folder-button"
            onClick={() => {
              requestVersion.current++;
              setSnapshots(true);
            }}
          >
            <DatabaseBackup size={15} /> Library backup…
          </button>
          <small>LOCAL · OFFLINE</small>
        </div>
      </aside>
      <main>
        <header className="topbar" data-tauri-drag-region>
          <PatchMenu
            view={view}
            patch={patch}
            onPatch={act}
            onImport={() => setSdImport(true)}
          />
          <div className="header-actions">
            <span className="live-state">
              <i />
              {busy ? "Saving…" : "Saved on this computer"}
            </span>
            <button
              className="icon-button"
              aria-label="Destination backups"
              title="Destination backups: undo a previous Build"
              onClick={() => setBackups({ focus: null })}
            >
              <History size={15} />
            </button>
            <span className="divider" />
            <button
              className="secondary export-button"
              disabled={!patch || !!building}
              title="Write this Patch's Kit audio to a folder or SD card"
              onClick={() => void build()}
            >
              <Upload size={14} />
              Build to SD…
            </button>
          </div>
        </header>
        <div className="workspace">
          {!legacy.managed && !legacy.isDemo && !migrationHidden && (
            <div className="migration">
              <LibraryMigration
                onComplete={() => {
                  setLegacy((l) => ({ ...l, managed: true }));
                  void act({ type: "open" });
                }}
              />
              <button
                className="icon-button"
                aria-label="Hide folder migration"
                title="Not now"
                onClick={() => setMigrationHidden(true)}
              >
                <X size={14} />
              </button>
            </div>
          )}
          {building && (
            <div className="notice" role="status">
              <LoaderCircle size={17} className="spin" />
              <span>{building}</span>
            </div>
          )}
          {review && (
            <BuildReviewPanel
              review={review}
              busy={!!building}
              onApply={() => void applyBuild()}
              onCancel={() => setReview(null)}
            />
          )}
          {built && (
            <BuildResult
              built={built}
              onRestore={(backup) => setBackups({ focus: backup })}
              onDismiss={() => setBuilt(null)}
            />
          )}
          {backups && (
            <DestinationBackups
              key={backups.focus ?? ""}
              focus={backups.focus}
              onClose={() => setBackups(null)}
            />
          )}
          {!patch ? (
            <FirstPatch
              onCreate={(name) => act({ type: "createPatch", name })}
              onImport={() => setSdImport(true)}
            />
          ) : (
            <>
              <section className="kit-heading">
                <div className="kit-code">{kit ? kitAddress(kit) : "—"}</div>
                <div>
                  <h1>Your kit. In context.</h1>
                  <p>
                    {kit
                      ? "Choose the Layers of each voice and hear how your samples fit together."
                      : `${patch.name} has no Kits yet.`}
                  </p>
                </div>
                {!kit && (
                  <button
                    className="primary"
                    onClick={() => {
                      const free = nextFreeAddress(patch);
                      void act({ type: "createKit", ...free });
                    }}
                  >
                    Create Kit {kitAddress(nextFreeAddress(patch))}
                  </button>
                )}
                {busy && (
                  <LoaderCircle
                    size={17}
                    className="spin loading-indicator"
                    aria-label="Saving"
                  />
                )}
              </section>
              {kit && session && (
                <>
                  <section className="sample-cards" aria-label="Sample voices">
                    {kit.voices.map((_, channel) => {
                      const source = voiceSource(patch, kit, channel);
                      return (
                        <VoiceCard
                          key={channel}
                          channel={channel}
                          kit={kit}
                          source={source.kit}
                          voice={source.voice}
                          samples={samples}
                          waveforms={view.waveforms}
                          activeIndex={transport.activeLayers[channel] ?? -1}
                          onPatch={onPatch}
                          onMode={onMode}
                          onAudition={onAudition}
                          onAdd={setPicker}
                          onEdit={onEdit}
                        />
                      );
                    })}
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
                              void musical({
                                type: e.target.value as
                                  "basicPattern" | "clearPattern",
                              });
                          }}
                        >
                          <option value="" disabled>
                            Pattern
                          </option>
                          <option value="basicPattern">
                            Load example beat
                          </option>
                          <option value="clearPattern">
                            Clear all 64 steps
                          </option>
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
                      {session.voices.map((voice, channel) => (
                        <SequencerRow
                          key={channel}
                          channel={channel}
                          voice={voice}
                          playhead={transport.running ? transport.step : -1}
                          onAction={musical}
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
                      playing={view.playing}
                      bpm={session.bpm}
                      transport={transport}
                      onAction={musical}
                    />
                  </section>
                </>
              )}
              {transport.error && (
                <div className="notice" role="alert">
                  <TriangleAlert size={17} />
                  <span>{transport.error}</span>
                </div>
              )}
              {issues.length > 0 && (
                <details className="compatibility">
                  <summary>
                    <TriangleAlert size={13} />
                    {issues.length} {issues.length === 1 ? "issue" : "issues"}{" "}
                    to fix before Build
                    <ChevronDown size={12} />
                  </summary>
                  <ul>
                    {issues.map((issue, i) => (
                      <li key={i}>{issue}</li>
                    ))}
                  </ul>
                </details>
              )}
              {report.length > 0 && (
                <details className="compatibility">
                  <summary>
                    <HardDriveDownload size={13} />
                    SD Import report
                    <ChevronDown size={12} />
                  </summary>
                  <p>
                    Kept for reference from the original card. Place each
                    unassigned occurrence with Add Sample in its Voice.
                  </p>
                  <ul>
                    {report.map((line, i) => (
                      <li key={i}>{line}</li>
                    ))}
                  </ul>
                </details>
              )}
            </>
          )}
          <footer>
            <div>
              <Keyboard size={13} />
              <span>Space: play / stop</span>
              <b>·</b>
              <span>1–4: audition voice</span>
            </div>
            <span title="Build to SD writes Bank/Kit folders and prepared audio. It never writes native Rample settings (_save).">
              Drafts are saved on this computer · Build never writes _save
            </span>
          </footer>
        </div>
      </main>
      {error && (
        <div className="toast" role="alert">
          <TriangleAlert size={17} />
          <span>
            {error.split("\n").map((line, i) => (
              <span key={i}>
                {i > 0 && <br />}
                {line}
              </span>
            ))}
          </span>
          <button aria-label="Dismiss error" onClick={() => setError(null)}>
            <X size={15} />
          </button>
        </div>
      )}
      {sdImport && (
        <SdImport
          onClose={() => setSdImport(false)}
          onImported={(snapshot) => {
            setView(snapshot);
            setSdImport(false);
          }}
        />
      )}
      {library && (
        <LibrarySamples
          onClose={() => {
            setLibrary(false);
            void act({ type: "open" });
          }}
        />
      )}
      {picker !== null && (
        <LibrarySamples
          onClose={() => {
            setPicker(null);
            void act({ type: "open" });
          }}
          onSelect={async (id) => {
            if (await act({ type: "addLayer", channel: picker, sample: id }))
              setPicker(null);
          }}
        />
      )}
      {snapshots && (
        <LibrarySnapshots
          managed={legacy.managed}
          onClose={() => setSnapshots(false)}
          onRestored={(result) => {
            requestVersion.current++;
            setEditor(null);
            setPicker(null);
            setLibrary(false);
            setSdImport(false);
            setVelocity(null);
            setReview(null);
            setBuilt(null);
            if (result.snapshot?.managed)
              setLegacy((l) => ({ ...l, managed: true }));
            void act({ type: "open" });
          }}
        />
      )}
      {editor && (
        <SampleEditor
          initial={editor}
          transport={transport}
          invalidated={false}
          onClose={() => {
            setEditor(null);
            void act({ type: "open" });
          }}
          onSaved={() => {}}
          onPatchSaved={setView}
          onToggle={() => musical({ type: "togglePlayback" })}
        />
      )}
      {velocity && session && (
        <VelocityDialog
          channel={velocity.channel}
          step={velocity.step}
          initial={session.voices[velocity.channel].velocities[velocity.step]}
          onClose={() => setVelocity(null)}
          onChange={(value) =>
            void musical({ type: "setVelocity", ...velocity, value })
          }
        />
      )}
    </div>
  );
}
