import { SdImport } from "./SdImport";
import { useEffect, useRef, useState } from "react";
import { SampleEditor } from "./SampleEditor";
import { Modal } from "./Modal";
import { LibrarySamples } from "./LibrarySamples";
import { SequencerRow, TransportBar } from "./Sequencer";
import * as native from "./native";
import {
  modes,
  type Action,
  type EditorInfo,
  type PatchAction,
  type PatchSnapshot,
  type Transport,
} from "./types";

export function PatchEditor({
  transport,
  onClose,
  managed = false,
}: {
  transport: Transport;
  onClose: () => void;
  managed?: boolean;
}) {
  const [sdImport, setSdImport] = useState(false);
  const [library, setLibrary] = useState(false);
  const [view, setView] = useState<PatchSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(true);
  const [newName, setNewName] = useState("");
  const [name, setName] = useState("");
  const [bank, setBank] = useState("A");
  const [number, setNumber] = useState(0);
  const [editor, setEditor] = useState<EditorInfo | null>(null);
  const [picker, setPicker] = useState<number | null>(null);
  const [velocity, setVelocity] = useState<{
    channel: number;
    step: number;
  } | null>(null);
  const alive = useRef(true);
  const queue = useRef(Promise.resolve());
  const patch = view?.graph.patches.find(
    (p) => p.id === view.graph.selectedPatch,
  );
  const kit = patch?.kits.find((k) => k.id === patch.selectedKit);
  useEffect(() => {
    setName(patch?.name ?? "");
  }, [patch?.id, patch?.name]);
  useEffect(() => {
    if (kit) {
      setBank(kit.bank);
      setNumber(kit.number);
    }
  }, [kit?.id, kit?.bank, kit?.number]);
  useEffect(() => {
    alive.current = true;
    void native
      .patchAction({ type: "open" })
      .then((result) => {
        if (alive.current) setView(result);
      })
      .catch((e) => {
        if (alive.current) setError(String(e));
      })
      .finally(() => {
        if (alive.current) setBusy(false);
      });
    return () => {
      alive.current = false;
    };
  }, []);
  async function act(action: PatchAction): Promise<boolean> {
    setBusy(true);
    setError(null);
    let success = false;
    const task = queue.current
      .catch(() => {})
      .then(async () => {
        const result = await native.patchAction(action);
        if (alive.current) {
          setView(result);
          success = true;
        }
      });
    queue.current = task;
    try {
      await task;
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      if (alive.current) setBusy(false);
    }
    return success;
  }
  async function musical(action: Action) {
    await act({ type: "musical", action });
  }
  async function close() {
    if (busy || managed) return;
    setBusy(true);
    try {
      await queue.current.catch(() => {});
      await native.closePatch();
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }
  useEffect(() => {
    function key(event: KeyboardEvent) {
      if (busy || sdImport || picker !== null || velocity || editor) return;
      if (
        event.target instanceof Element &&
        event.target.closest('input,textarea,select,[contenteditable="true"]')
      )
        return;
      if (event.repeat || event.metaKey || event.ctrlKey || event.altKey)
        return;
      if (event.code === "Space") {
        event.preventDefault();
        void musical({ type: "togglePlayback" });
      } else if (/^[1-4]$/.test(event.key)) {
        event.preventDefault();
        void musical({ type: "audition", channel: Number(event.key) - 1 });
      }
    }
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });
  if (editor)
    return (
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
        onToggle={async () => {
          await musical({ type: "togglePlayback" });
        }}
      />
    );
  if (sdImport)
    return (
      <SdImport
        onClose={() => setSdImport(false)}
        onImported={(snapshot) => {
          setView(snapshot);
          setSdImport(false);
        }}
      />
    );
  if (library)
    return (
      <LibrarySamples
        onClose={() => {
          setLibrary(false);
          void act({ type: "open" });
        }}
      />
    );
  if (picker !== null)
    return (
      <LibrarySamples
        onClose={() => {
          setPicker(null);
          void act({ type: "open" });
        }}
        onSelect={async (id) => {
          await act({ type: "addLayer", channel: picker, sample: id });
          setPicker(null);
        }}
      />
    );
  if (velocity && view?.session)
    return (
      <Modal label="Step velocity" onClose={() => setVelocity(null)}>
        <h2>
          Voice {velocity.channel + 1}, step {velocity.step + 1}
        </h2>
        <label>
          Velocity
          <input
            type="number"
            min={1}
            max={127}
            value={
              view.session.voices[velocity.channel].velocities[velocity.step]
            }
            onChange={(e) =>
              void musical({
                type: "setVelocity",
                ...velocity,
                value: Number(e.target.value),
              })
            }
          />
        </label>
        <button onClick={() => setVelocity(null)}>Done</button>
      </Modal>
    );
  return (
    <Modal label="Patches" onClose={() => void close()}>
      <div className="patch-workspace">
        <header className="patch-heading">
          <div>
            <h2>Patches</h2>
            <p>Arrange shared Library Samples into ordered Rample Kits.</p>
          </div>
          <div className="patch-heading-actions">
            <button disabled={busy} onClick={() => setSdImport(true)}>
              Import Rample SD…
            </button>
            <button disabled={busy} onClick={() => void act({ type: "open" })}>
              Refresh preview
            </button>
            <button disabled={busy} onClick={() => setLibrary(true)}>
              Library samples…
            </button>
            {!managed && (
              <button disabled={busy} onClick={() => void close()}>
                Back to folders
              </button>
            )}
          </div>
        </header>
        {error && <p role="alert">{error}</p>}
        {busy && <p role="status">Saving and preparing preview…</p>}
        <fieldset disabled={busy} className="patch-settings">
          <label>
            Selected Patch
            <select
              value={patch?.id ?? ""}
              onChange={(e) =>
                void act({ type: "selectPatch", patch: e.target.value })
              }
            >
              <option value="" disabled>
                Select a Patch
              </option>
              {view?.graph.patches.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void act({ type: "createPatch", name: newName }).then((ok) => {
                if (ok) setNewName("");
              });
            }}
          >
            <label>
              New Patch name
              <input
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                required
              />
            </label>
            <button disabled={!newName.trim()}>Create Patch</button>
          </form>
          {patch && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void act({ type: "renamePatch", name });
              }}
            >
              <label>
                Patch name
                <input
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  required
                />
              </label>
              <button disabled={!name.trim()}>Rename Patch</button>
            </form>
          )}
        </fieldset>
        {patch && (
          <fieldset disabled={busy} className="patch-settings">
            <label>
              Selected Kit
              <select
                value={kit?.id ?? ""}
                onChange={(e) =>
                  void act({ type: "selectKit", kit: e.target.value })
                }
              >
                <option value="" disabled>
                  Select a Kit
                </option>
                {[...patch.kits]
                  .sort(
                    (a, b) =>
                      a.bank.localeCompare(b.bank) || a.number - b.number,
                  )
                  .map((k) => (
                    <option key={k.id} value={k.id}>
                      {k.bank}
                      {k.number}
                    </option>
                  ))}
              </select>
            </label>
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void act({ type: "createKit", bank, number });
              }}
            >
              <label>
                Bank
                <select value={bank} onChange={(e) => setBank(e.target.value)}>
                  {Array.from({ length: 26 }, (_, i) =>
                    String.fromCharCode(65 + i),
                  ).map((b) => (
                    <option key={b}>{b}</option>
                  ))}
                </select>
              </label>
              <label>
                Kit number
                <input
                  type="number"
                  min={0}
                  max={99}
                  value={number}
                  onChange={(e) => setNumber(Number(e.target.value))}
                  required
                />
              </label>
              <button>Create Kit</button>
              {kit && (
                <button
                  type="button"
                  onClick={() => void act({ type: "addressKit", bank, number })}
                >
                  Move Kit here
                </button>
              )}
            </form>
          </fieldset>
        )}
        {patch?.importedSd && (
          <details className="patch-validation">
            <summary>SD Import report</summary>
            <p>
              Historical source issues are kept for reference. Current draft
              validation below updates as you repair the Patch.
            </p>
            <ul>
              {patch.importedSd.issues.map((issue, index) => (
                <li key={index}>{issue}</li>
              ))}
            </ul>
            {patch.importedSd.unassigned.length > 0 && (
              <>
                <strong>Unassigned source occurrences</strong>
                <p>Use Add Sample in the intended Voice for each occurrence.</p>
                <ul>
                  {patch.importedSd.unassigned.map((file, index) => (
                    <li key={index}>{file.path}</li>
                  ))}
                </ul>
              </>
            )}
          </details>
        )}
        {view && view.validation.length > 0 && (
          <div className="patch-validation" role="status">
            <strong>Draft validation</strong>
            <ul>
              {view.validation.map((message, i) => (
                <li key={i}>{message}</li>
              ))}
            </ul>
          </div>
        )}
        {!patch && !busy && (
          <p>Create a named Patch to begin. All Patches share your Library.</p>
        )}
        {patch && !kit && (
          <p>Create a Kit at a Bank/Kit address to add Layers.</p>
        )}
        {kit && view?.session && (
          <>
            <fieldset disabled={busy} className="patch-voices">
              {kit.voices.map((_, channel) => {
                const source =
                  patch?.kits.find(
                    (k) => k.id === (patch.voiceKits?.[channel] ?? kit.id),
                  ) ?? kit;
                const voice = source.voices[channel];
                return (
                  <section
                    key={channel}
                    aria-label={`Patch Voice ${channel + 1}`}
                  >
                    <h3>Voice {channel + 1}</h3>
                    <p>
                      Source Kit {source.bank}
                      {source.number}
                    </p>
                    <label>
                      Layer mode
                      <select
                        aria-label={`Layer mode for Voice ${channel + 1}`}
                        value={voice.mode}
                        onChange={(e) =>
                          void musical({
                            type: "setMode",
                            channel,
                            mode: e.target.value as typeof voice.mode,
                          })
                        }
                      >
                        {modes.map((mode) => (
                          <option key={mode.id} value={mode.id}>
                            {mode.title}
                          </option>
                        ))}
                      </select>
                    </label>
                    <ol>
                      {voice.layers.map((layer, index) => {
                        const sample = view.samples.find(
                          (s) => s.id === layer.sampleId,
                        );
                        return (
                          <li
                            key={layer.id}
                            className={
                              voice.selectedLayer === layer.id ? "selected" : ""
                            }
                          >
                            <button
                              aria-label={`Select Layer ${index + 1}: ${sample?.name ?? "Unavailable Sample"}`}
                              aria-pressed={voice.selectedLayer === layer.id}
                              onClick={() =>
                                void act({
                                  type: "selectLayer",
                                  channel,
                                  layer: layer.id,
                                })
                              }
                            >
                              {index + 1}.{" "}
                              {sample?.name ?? "Unavailable Sample"}
                            </button>
                            <div className="patch-layer-actions">
                              <button
                                aria-label={`Move Layer ${index + 1} up`}
                                disabled={index === 0}
                                onClick={() =>
                                  void act({
                                    type: "moveLayer",
                                    channel,
                                    layer: layer.id,
                                    to: index - 1,
                                  })
                                }
                              >
                                ↑
                              </button>
                              <button
                                aria-label={`Move Layer ${index + 1} down`}
                                disabled={index === voice.layers.length - 1}
                                onClick={() =>
                                  void act({
                                    type: "moveLayer",
                                    channel,
                                    layer: layer.id,
                                    to: index + 1,
                                  })
                                }
                              >
                                ↓
                              </button>
                              <button
                                aria-label={`Repeat Layer ${index + 1}`}
                                onClick={() =>
                                  void act({
                                    type: "addLayer",
                                    channel,
                                    sample: layer.sampleId,
                                  })
                                }
                              >
                                Repeat
                              </button>
                              <button
                                aria-label={`Remove Layer ${index + 1}`}
                                onClick={() =>
                                  void act({
                                    type: "removeLayer",
                                    channel,
                                    layer: layer.id,
                                  })
                                }
                              >
                                Remove
                              </button>
                            </div>
                          </li>
                        );
                      })}
                    </ol>
                    <button
                      aria-label={`Add Sample to Voice ${channel + 1}`}
                      onClick={() => setPicker(channel)}
                    >
                      Add Sample…
                    </button>
                    <button
                      aria-label={`Edit selected Sample in Voice ${channel + 1}`}
                      disabled={!voice.layers.length}
                      onClick={() => {
                        setBusy(true);
                        setError(null);
                        void native
                          .openManagedEditor(channel)
                          .then(setEditor)
                          .catch((e) => setError(String(e)))
                          .finally(() => setBusy(false));
                      }}
                    >
                      Edit Sample…
                    </button>
                    <button
                      aria-label={`Audition Voice ${channel + 1}`}
                      disabled={!voice.layers.length}
                      onClick={() =>
                        void musical({ type: "audition", channel })
                      }
                    >
                      Audition
                    </button>
                  </section>
                );
              })}
            </fieldset>
            <section className="sequencer" aria-label="Patch 16-step sequencer">
              <h3>Sequencer · 16 steps</h3>
              <div className="patch-pattern">
                <button
                  disabled={busy}
                  onClick={() => void musical({ type: "basicPattern" })}
                >
                  Load example beat
                </button>
                <button
                  disabled={busy}
                  onClick={() => void musical({ type: "clearPattern" })}
                >
                  Clear pattern
                </button>
              </div>
              <div className="step-grid">
                {view.session.voices.map((voice, channel) => (
                  <SequencerRow
                    key={channel}
                    channel={channel}
                    voice={voice}
                    playhead={transport.running ? transport.step : -1}
                    onAction={musical}
                    onVelocity={setVelocity}
                  />
                ))}
              </div>
              <TransportBar
                playing={view.playing}
                bpm={view.session.bpm}
                transport={transport}
                onAction={musical}
              />
            </section>
          </>
        )}
        <p className="patch-native-note">
          Drafts are saved on this computer. Native SD settings and Build Patch
          are pending device verification.
        </p>
      </div>
    </Modal>
  );
}
