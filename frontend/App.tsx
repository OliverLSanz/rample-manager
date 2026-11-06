import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AudioLines,
  ChevronDown,
  DatabaseBackup,
  Disc3,
  Grid2X2,
  HardDriveDownload,
  History,
  Info,
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
import { SamplesScreen } from "./SamplesScreen";
import { LibrarySnapshots } from "./LibrarySnapshots";
import {
  BuildProgressNotice,
  BuildResult,
  BuildReviewPanel,
  DestinationBackups,
} from "./BuildPatch";
import { KitList } from "./KitList";
import { SamplePanel } from "./SamplePanel";
import { PatchMenu } from "./PatchMenu";
import { VoiceCard } from "./VoiceCard";
import { SequencerRow, TransportBar } from "./Sequencer";
import {
  currentKit,
  currentPatch,
  kitAddress,
  buildBlockers,
  nextFreeAddress,
  patchIssues,
  sortedKits,
  voiceSource,
} from "./patchView";
import {
  idleTransport,
  type Action,
  type BuildReply,
  type BuildProgress,
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

const mod = /Mac/i.test(navigator.platform) ? "⌘" : "Ctrl+";
const shortcutGroups: [string, [string, string][]][] = [
  [
    "Playback",
    [
      ["Space", "Play or stop the sequencer"],
      ["1 – 4", "Audition a voice"],
      ["Right-click a step", "Edit its velocity"],
    ],
  ],
  [
    "Kits",
    [
      ["[ and ]", "Previous or next Kit"],
      ["↑ ↓ in the Kit list", "Select a Kit"],
      ["Delete in the Kit list", "Delete the focused Kit"],
      [`${mod}Z`, "Undo a deletion while it is offered"],
    ],
  ],
  [
    "Views",
    [
      [`${mod}1`, "Kit"],
      [`${mod}2`, "Samples"],
      [`${mod}F`, "Search the current view"],
      ["?", "These shortcuts"],
    ],
  ],
  [
    "Samples and Add Samples",
    [
      ["↑ ↓", "Browse, hearing each Sample"],
      ["Enter", "Add as a Layer"],
      ["Space", "Hear the selected Sample"],
      [`Shift or ${mod}click`, "Select several Samples"],
    ],
  ],
];

function ShortcutsSheet({ onClose }: { onClose: () => void }) {
  return (
    <Modal label="Keyboard shortcuts" onClose={onClose}>
      <div className="modal-heading">
        <Keyboard size={20} />
        <h2>Keyboard shortcuts</h2>
        <button aria-label="Close keyboard shortcuts" onClick={onClose}>
          <X size={17} />
        </button>
      </div>
      <div className="shortcut-groups">
        {shortcutGroups.map(([title, rows]) => (
          <section key={title} aria-label={title}>
            <h3>{title}</h3>
            <dl>
              {rows.map(([keys, action]) => (
                <div key={keys}>
                  <dt>
                    <kbd>{keys}</kbd>
                  </dt>
                  <dd>{action}</dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
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
  const [transport, setTransport] = useState<Transport>(idleTransport);
  const [error, setError] = useState<string | null>(null);
  const [bootError, setBootError] = useState<string | null>(null);
  const [pending, setPending] = useState(0);
  const [sdImport, setSdImport] = useState(false);
  // The main window shows the Kit or the Library's Samples.
  const [screen, setScreen] = useState<"kit" | "samples">("kit");
  // The Voice that the Add Samples panel adds to; null while it is closed.
  const [adding, setAdding] = useState<number | null>(null);
  const [snapshots, setSnapshots] = useState(false);
  const [editor, setEditor] = useState<EditorInfo | null>(null);
  const [velocity, setVelocity] = useState<{
    channel: number;
    step: number;
  } | null>(null);
  const [building, setBuilding] = useState<string | null>(null);
  const [review, setReview] = useState<BuildReview | null>(null);
  const [buildProgress, setBuildProgress] = useState<BuildProgress | null>(
    null,
  );
  const [built, setBuilt] = useState<BuildReply | null>(null);
  const [backups, setBackups] = useState<{ focus: string | null } | null>(null);
  // Counts deletions so each one restarts the Undo offer; 0 hides it.
  const [undoOffer, setUndoOffer] = useState(0);
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
  /** Deletes a Kit, Patch or Layer, then offers Undo for a few seconds. */
  const remove = useCallback(
    async (action: PatchAction) => {
      if (await act(action)) setUndoOffer((n) => n + 1);
    },
    [act],
  );
  const onPatch = useCallback(
    (action: PatchAction) =>
      void (action.type === "removeLayer" ? remove(action) : act(action)),
    [act, remove],
  );
  const undo = useCallback(() => {
    setUndoOffer(0);
    void act({ type: "undoDelete" });
  }, [act]);
  useEffect(() => {
    if (!undoOffer) return;
    const timer = setTimeout(() => setUndoOffer(0), 10000);
    return () => clearTimeout(timer);
  }, [undoOffer]);
  const onMode = useCallback(
    (channel: number, mode: LayerMode) =>
      void musical({ type: "setMode", channel, mode }),
    [musical],
  );
  const onAudition = useCallback(
    (channel: number) => void musical({ type: "audition", channel }),
    [musical],
  );
  /** Adds Samples as new Layers in order, stopping at the first refusal. */
  const addLayers = useCallback(
    async (channel: number, samples: string[]) => {
      for (const sample of samples)
        if (!(await act({ type: "addLayer", channel, sample }))) return false;
      return samples.length > 0;
    },
    [act],
  );
  const onDropSample = useCallback(
    (channel: number, samples: string[]) => void addLayers(channel, samples),
    [addLayers],
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

  const [shortcuts, setShortcuts] = useState(false);
  const overlay = sdImport || snapshots || shortcuts || !!editor || !!velocity;
  useEffect(() => {
    function key(e: KeyboardEvent) {
      if (overlay || !view) return;
      if (
        undoOffer &&
        (e.metaKey || e.ctrlKey) &&
        !e.shiftKey &&
        e.key.toLowerCase() === "z" &&
        !(
          e.target instanceof Element &&
          e.target.closest('input,textarea,[contenteditable="true"]')
        )
      ) {
        e.preventDefault();
        undo();
        return;
      }
      const command = (e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey;
      if (command && (e.key === "1" || e.key === "2")) {
        e.preventDefault();
        if (e.key === "2") setAdding(null);
        setScreen(e.key === "1" ? "kit" : "samples");
        return;
      }
      if (command && e.key.toLowerCase() === "f") {
        e.preventDefault();
        const search =
          screen === "samples"
            ? 'input[aria-label="Search Samples by name or tag"]'
            : adding !== null
              ? '.sample-panel input[type="search"]'
              : 'input[aria-label="Search kits or samples"]';
        document.querySelector<HTMLInputElement>(search)?.focus();
        return;
      }
      const typing =
        e.target instanceof Element &&
        !!e.target.closest(
          'input,textarea,select,[role="listbox"],[contenteditable="true"]',
        );
      // Option is allowed: some layouts, such as Spanish, type [ ] with it.
      if (!typing && !e.metaKey && !e.ctrlKey) {
        if (e.key === "?") {
          e.preventDefault();
          setShortcuts(true);
          return;
        }
        if (e.key === "[" || e.key === "]") {
          e.preventDefault();
          const open = currentPatch(view);
          const kits = sortedKits(open);
          const at = kits.findIndex((k) => k.id === open?.selectedKit);
          const next = kits[at + (e.key === "]" ? 1 : -1)];
          if (next) {
            setScreen("kit");
            void act({ type: "selectKit", kit: next.id });
          }
          return;
        }
      }
      if (
        e.target instanceof Element &&
        e.target.closest(
          'button,a[href],input,textarea,select,[role="button"],[role="listbox"],[contenteditable="true"]',
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
  }, [overlay, view, musical, undoOffer, undo, screen, adding, act]);

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
  const blockers = buildBlockers(issues);
  const notes = issues.filter((issue) => !blockers.includes(issue));
  const blockersRef = useRef<HTMLDetailsElement>(null);
  const busy = pending > 0;

  async function build() {
    if (!patch || busy || building) return;
    if (blockers.length) {
      // Choosing a card would only end in a refusal: show what to fix first.
      setScreen("kit");
      requestAnimationFrame(() => {
        const details = blockersRef.current;
        if (!details) return;
        details.open = true;
        details.scrollIntoView?.({ behavior: "smooth", block: "center" });
        details.querySelector("summary")?.focus();
      });
      return;
    }
    const destination = await native.chooseFolder(
      "Choose a folder or SD card for Build Patch",
    );
    if (!destination || !alive.current) return;
    setBuilding("Reviewing the destination…");
    setBuildProgress(null);
    setBuilt(null);
    setReview(null);
    setError(null);
    try {
      await queue.current.catch(() => {});
      const reviewed = await native.reviewPatchBuild(
        patch.id,
        destination,
        (step) => alive.current && setBuildProgress(step),
      );
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
    setBuildProgress(null);
    setError(null);
    try {
      await queue.current.catch(() => {});
      const reply = await native.applyPatchBuild(
        review,
        (step) => alive.current && setBuildProgress(step),
      );
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
        <h1>Rample Manager</h1>
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
    <div
      className={`app-shell ${adding !== null && kit && screen === "kit" ? "with-panel" : ""}`}
    >
      <aside className="sidebar">
        <div className="sidebar-drag" data-tauri-drag-region />
        <div className="brand">
          <AudioLines size={31} />
          <div>
            <strong>RAMPLE</strong>
            <span>M A N A G E R</span>
          </div>
          <small>{__APP_VERSION__.split(".").slice(0, 2).join(".")}</small>
        </div>
        <KitList
          key={patch?.id ?? ""}
          patch={patch}
          issues={issues}
          samples={samples}
          onPatch={(action) => {
            if (action.type === "selectKit") setScreen("kit");
            return act(action);
          }}
          onDelete={(kit) => void remove({ type: "deleteKit", kit })}
        />
        <div className="sidebar-bottom">
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
            onDelete={(patch) => void remove({ type: "deletePatch", patch })}
          />
          <div className="view-switch" role="tablist" aria-label="View">
            <button
              role="tab"
              aria-selected={screen === "kit"}
              onClick={() => setScreen("kit")}
            >
              <Grid2X2 size={13} /> Kit
            </button>
            <button
              role="tab"
              aria-selected={screen === "samples"}
              onClick={() => {
                setAdding(null);
                setScreen("samples");
              }}
            >
              <AudioLines size={13} /> Samples
            </button>
          </div>
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
              title={
                blockers.length
                  ? "Fix the listed issues before building this Patch"
                  : "Write this Patch's Kit audio to a folder or SD card"
              }
              onClick={() => void build()}
            >
              <Upload size={14} />
              Build to SD…
              {blockers.length > 0 && (
                <span
                  className="build-blockers"
                  aria-label={`${blockers.length} ${blockers.length === 1 ? "issue blocks" : "issues block"} Build`}
                >
                  {blockers.length}
                </span>
              )}
            </button>
          </div>
        </header>
        <div
          className={`workspace ${screen === "samples" ? "samples-mode" : ""}`}
        >
          {building && (
            <BuildProgressNotice title={building} step={buildProgress} />
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
          {screen === "samples" ? (
            <SamplesScreen
              view={view}
              onPatch={act}
              onChanged={() => void act({ type: "open" })}
              onOpen={(usage) => {
                void (async () => {
                  if (usage.patch !== view.graph.selectedPatch)
                    if (
                      !(await act({ type: "selectPatch", patch: usage.patch }))
                    )
                      return;
                  if (await act({ type: "selectKit", kit: usage.kit }))
                    setScreen("kit");
                })();
              }}
            />
          ) : !patch ? (
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
                          onAdd={setAdding}
                          onEdit={onEdit}
                          onDropSample={onDropSample}
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
              {blockers.length > 0 && (
                <details className="compatibility" ref={blockersRef}>
                  <summary>
                    <TriangleAlert size={13} />
                    {blockers.length}{" "}
                    {blockers.length === 1 ? "issue" : "issues"} to fix before
                    Build
                    <ChevronDown size={12} />
                  </summary>
                  <ul>
                    {blockers.map((issue, i) => {
                      // Kit issues start with the address: link to that Kit.
                      const [, address, rest] =
                        /^([A-Z]\d+): (.*)$/s.exec(issue) ?? [];
                      const target = patch.kits.find(
                        (k) => kitAddress(k) === address,
                      );
                      return (
                        <li key={i}>
                          {target ? (
                            <>
                              <button
                                className="issue-kit"
                                aria-label={`Show Kit ${address}`}
                                onClick={() =>
                                  void act({
                                    type: "selectKit",
                                    kit: target.id,
                                  })
                                }
                              >
                                {address}
                              </button>{" "}
                              {rest}
                            </>
                          ) : (
                            issue
                          )}
                        </li>
                      );
                    })}
                  </ul>
                </details>
              )}
              {notes.length > 0 && (
                <details className="compatibility notes">
                  <summary>
                    <Info size={13} />
                    {notes.length} {notes.length === 1 ? "note" : "notes"} on
                    this Patch
                    <ChevronDown size={12} />
                  </summary>
                  <ul>
                    {notes.map((note, i) => (
                      <li key={i}>{note}</li>
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
          {screen === "kit" && (
            <footer>
              <button
                className="shortcuts-button"
                onClick={() => setShortcuts(true)}
              >
                <Keyboard size={13} />
                <span>Space: play / stop</span>
                <b>·</b>
                <span>1–4: audition voice</span>
                <b>·</b>
                <span>? all shortcuts</span>
              </button>
              <span title="Build to SD writes Bank/Kit folders and prepared audio. It never writes native Rample settings (_save).">
                Drafts are saved on this computer · Build never writes _save
              </span>
            </footer>
          )}
        </div>
      </main>
      {adding !== null && kit && screen === "kit" && (
        <SamplePanel
          samples={view.samples}
          channel={adding}
          kit={kitAddress(kit)}
          onChannel={setAdding}
          onAdd={(samples, channel) =>
            addLayers(
              channel,
              samples.map((s) => s.id),
            )
          }
          onClose={() => setAdding(null)}
        />
      )}
      <div className="toasts">
        {undoOffer > 0 && view.undo && (
          <div className="toast undo-toast" role="status">
            <span>Deleted {view.undo}</span>
            <button className="undo-button" onClick={undo}>
              Undo{" "}
              <kbd>{/Mac/i.test(navigator.platform) ? "⌘Z" : "Ctrl+Z"}</kbd>
            </button>
            <button aria-label="Dismiss" onClick={() => setUndoOffer(0)}>
              <X size={15} />
            </button>
          </div>
        )}
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
      </div>
      {sdImport && (
        <SdImport
          onClose={() => setSdImport(false)}
          onImported={(snapshot) => {
            setView(snapshot);
            setSdImport(false);
          }}
        />
      )}
      {snapshots && (
        <LibrarySnapshots
          onClose={() => setSnapshots(false)}
          onRestored={() => {
            requestVersion.current++;
            setEditor(null);
            setAdding(null);
            setSdImport(false);
            setVelocity(null);
            setReview(null);
            setBuilt(null);
            void act({ type: "open" });
          }}
        />
      )}
      {editor && (
        <SampleEditor
          initial={editor}
          transport={transport}
          onClose={() => {
            setEditor(null);
            void act({ type: "open" });
          }}
          onPatchSaved={setView}
          onToggle={() => musical({ type: "togglePlayback" })}
        />
      )}
      {shortcuts && <ShortcutsSheet onClose={() => setShortcuts(false)} />}
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
