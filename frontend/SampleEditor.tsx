import { useCallback, useEffect, useRef, useState } from "react";
import {
  Play,
  Pause,
  Save,
  SlidersHorizontal,
  X,
  RotateCcw,
  LoaderCircle,
} from "lucide-react";
import { Modal } from "./Modal";
import * as native from "./native";
import {
  basename,
  type EditorInfo,
  type EditSettings,
  type Snapshot,
  type PatchSnapshot,
  type Transport,
} from "./types";
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));
export function SampleEditor({
  initial,
  transport,
  invalidated,
  onClose,
  onSaved,
  onToggle,
  onPatchSaved,
}: {
  initial: EditorInfo;
  transport: Transport;
  invalidated: boolean;
  onClose: () => void;
  onSaved: (s: Snapshot) => void;
  onToggle: () => Promise<void>;
  onPatchSaved?: (patch: PatchSnapshot) => void;
}) {
  const [info, setInfo] = useState(initial);
  const [sourceInvalidated, setSourceInvalidated] = useState(false);
  invalidated = invalidated || sourceInvalidated;
  const [variantName, setVariantName] = useState(
    `${initial.managed?.name ?? "Sample"} variant`,
  );
  const [managedSaved, setManagedSaved] = useState(false);
  const [settings, setSettings] = useState(initial.settings);
  const [enabled, setEnabled] = useState(true);
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<{
    backup: string;
    clipped: number;
  } | null>(null);
  const [duration, setDuration] = useState(
    initial.outputDuration ?? initial.duration,
  );
  const chain = useRef(Promise.resolve());
  const sequence = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const current = useRef({ info, settings, enabled });
  current.current = { info, settings, enabled };
  const mounted = useRef(true);
  const dirty = JSON.stringify(settings) !== JSON.stringify(info.settings);
  const queuePreview = useCallback((next: EditSettings, active: boolean) => {
    clearTimeout(timer.current);
    const version = ++sequence.current;
    const id = current.current.info.id;
    setPending(true);
    timer.current = setTimeout(() => {
      chain.current = chain.current
        .catch(() => {})
        .then(async () => {
          if (!mounted.current || version !== sequence.current) return;
          try {
            const result = await native.previewEditor(
              id,
              version,
              next,
              active,
            );
            if (mounted.current && version === sequence.current) {
              setDuration(result.duration);
              setPending(false);
              setError(null);
            }
          } catch (e) {
            if (mounted.current && version === sequence.current) {
              setError(message(e));
              if (
                current.current.info.managed &&
                /changed|closed|no longer|unavailable|restore the library|reopen/i.test(
                  message(e),
                )
              )
                setSourceInvalidated(true);
              setPending(false);
            }
          }
        });
    }, 50);
  }, []);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      clearTimeout(timer.current);
    };
  }, []);
  useEffect(() => {
    const id = info.id;
    return () => {
      void chain.current
        .catch(() => {})
        .then(() => native.closeEditor(id))
        .catch(() => {});
    };
  }, [info.id]);
  useEffect(() => {
    function key(event: KeyboardEvent) {
      if (
        event.code === "Space" &&
        !busy &&
        !(
          event.target instanceof Element &&
          event.target.closest("input,select,button,textarea")
        )
      ) {
        event.preventDefault();
        void onToggle();
      }
    }
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [busy, onToggle]);
  function change(key: keyof EditSettings, value: number) {
    if (!Number.isFinite(value)) return;
    const next = { ...current.current.settings, [key]: value };
    const minimum = Math.min(0.001, info.duration);
    if (key === "start")
      next.start = Math.max(0, Math.min(value, next.end - minimum));
    if (key === "end")
      next.end = Math.min(info.duration, Math.max(value, next.start + minimum));
    current.current.settings = next;
    setSettings(next);
    setSaved(null);
    setManagedSaved(false);
    queuePreview(next, enabled);
  }
  function togglePreview() {
    setEnabled(!enabled);
    queuePreview(settings, !enabled);
  }
  async function close() {
    if (busy) return;
    setBusy(true);
    clearTimeout(timer.current);
    sequence.current++;
    try {
      await chain.current;
      await native.closeEditor(info.id);
      onClose();
    } catch (e) {
      setError(message(e));
      setBusy(false);
    }
  }
  async function save(asNew = false) {
    if (busy || !dirty || invalidated) return;
    clearTimeout(timer.current);
    sequence.current++;
    setBusy(true);
    setError(null);
    try {
      await chain.current;
      if (info.managed) {
        const result = await native.saveManagedEditor(
          info.id,
          settings,
          asNew ? variantName : null,
        );
        current.current = {
          info: result.editor,
          settings: result.editor.settings,
          enabled: true,
        };
        sequence.current = 0;
        setInfo(result.editor);
        setSettings(result.editor.settings);
        setEnabled(true);
        setDuration(
          result.editor.outputDuration ??
            result.editor.settings.end - result.editor.settings.start,
        );
        setPending(false);
        setManagedSaved(true);
        onPatchSaved?.(result.patch);
        return;
      }
      const result = await native.saveEditor(info.id, settings);
      current.current = {
        info: result.editor,
        settings: result.editor.settings,
        enabled: true,
      };
      sequence.current = 0;
      setInfo(result.editor);
      setSettings(result.editor.settings);
      setEnabled(true);
      setDuration(result.rendered.duration);
      setPending(false);
      setSaved({
        backup: result.backup,
        clipped: result.rendered.clippedFrames,
      });
      onSaved(result.snapshot);
      if (result.snapshot.message) setError(result.snapshot.message);
    } catch (e) {
      setError(message(e));
      if (
        info.managed &&
        /changed|closed|no longer|unavailable|restore the library|reopen/i.test(
          message(e),
        )
      )
        setSourceInvalidated(true);
    } finally {
      setBusy(false);
    }
  }
  function reset() {
    setSettings(info.settings);
    current.current.settings = info.settings;
    setSaved(null);
    setManagedSaved(false);
    queuePreview(info.settings, enabled);
  }
  return (
    <Modal label="Sample editor" onClose={() => void close()}>
      <div className="sample-editor">
        <div className="modal-heading">
          <SlidersHorizontal size={21} />
          <h2>
            Sample editor <small>VOICE {info.channel + 1}</small>
          </h2>
          <button
            aria-label="Close sample editor"
            disabled={busy}
            onClick={() => void close()}
          >
            <X size={17} />
          </button>
        </div>
        <p className="editor-filename" title={info.path}>
          {info.managed?.name ?? basename(info.path)}
        </p>
        <div className="editor-transport">
          <button
            className="secondary"
            disabled={busy || invalidated}
            onClick={() =>
              void native
                .auditionEditor(info.id)
                .catch((e) => setError(message(e)))
            }
          >
            <Play size={13} />
            Audition sample
          </button>
          <button
            className="secondary"
            disabled={busy}
            onClick={() => void onToggle()}
          >
            {transport.running ? <Pause size={13} /> : <Play size={13} />}
            {transport.running ? "Pause sequence" : "Play sequence"}
          </button>
          <label className="editor-bypass">
            <input
              type="checkbox"
              checked={enabled}
              disabled={busy || invalidated}
              onChange={togglePreview}
            />
            Preview edits
          </label>
          <span
            className={`editor-status ${(transport.editorPeak ?? 0) > 1 ? "warning" : ""}`}
            role="status"
          >
            {(transport.editorPeak ?? 0) > 1
              ? "Clipping · lower Gain"
              : pending
                ? "Updating…"
                : enabled
                  ? "Live preview"
                  : info.managed
                    ? "Saved sound"
                    : "Original audio"}
          </span>
        </div>
        <p className="small">
          {info.managed
            ? "Draft preview applies only to this Layer occurrence. Closing restores its saved sound."
            : "Only this sample is edited. Other layers and voices keep their sound."}{" "}
          Drag the trim handles or enter times below.
        </p>
        <TrimWaveform
          peaks={info.waveform}
          duration={info.duration}
          start={settings.start}
          end={settings.end}
          disabled={busy || invalidated}
          onChange={change}
        />
        <fieldset disabled={busy || invalidated} className="editor-trim-values">
          <label>
            Start (s)
            <NumberInput
              label="Start time"
              min={0}
              max={settings.end - 0.001}
              step="0.001"
              value={settings.start}
              onChange={(value) => change("start", value)}
            />
          </label>
          <label>
            End (s)
            <NumberInput
              label="End time"
              min={settings.start + 0.001}
              max={info.duration}
              step="0.001"
              value={settings.end}
              onChange={(value) => change("end", value)}
            />
          </label>
          <span>
            Output with tails <b>{duration.toFixed(3)} s</b>
          </span>
        </fieldset>
        <fieldset disabled={busy || invalidated} className="editor-controls">
          <section>
            <h3>Level & pitch</h3>
            <Slider
              label="Gain"
              value={settings.gainDb}
              min={-36}
              max={12}
              step={0.1}
              unit="dB"
              onChange={(v) => change("gainDb", v)}
            />
            <Slider
              label="Pitch"
              value={settings.pitch}
              min={-24}
              max={24}
              step={0.1}
              unit="st"
              onChange={(v) => change("pitch", v)}
            />
            <p className="small">Pitch changes playback speed and duration.</p>
          </section>
          <section>
            <h3>Equalizer</h3>
            <Slider
              label="Low · 120 Hz"
              value={settings.lowDb}
              min={-18}
              max={18}
              step={0.1}
              unit="dB"
              onChange={(v) => change("lowDb", v)}
            />
            <Slider
              label="Mid · 1 kHz"
              value={settings.midDb}
              min={-18}
              max={18}
              step={0.1}
              unit="dB"
              onChange={(v) => change("midDb", v)}
            />
            <Slider
              label="High · 8 kHz"
              value={settings.highDb}
              min={-18}
              max={18}
              step={0.1}
              unit="dB"
              onChange={(v) => change("highDb", v)}
            />
          </section>
          <section>
            <h3>Echo</h3>
            <Slider
              label="Echo amount"
              value={settings.echoMix * 100}
              min={0}
              max={100}
              step={1}
              unit="%"
              onChange={(v) => change("echoMix", v / 100)}
            />
            <Slider
              label="Echo time"
              value={settings.echoMs}
              min={30}
              max={1000}
              step={1}
              unit="ms"
              onChange={(v) => change("echoMs", v)}
            />
            <Slider
              label="Echo feedback"
              value={settings.echoFeedback * 100}
              min={0}
              max={70}
              step={1}
              unit="%"
              onChange={(v) => change("echoFeedback", v / 100)}
            />
          </section>
          <section>
            <h3>Reverb</h3>
            <Slider
              label="Reverb amount"
              value={settings.reverbMix * 100}
              min={0}
              max={100}
              step={1}
              unit="%"
              onChange={(v) => change("reverbMix", v / 100)}
            />
            <Slider
              label="Reverb decay"
              value={settings.reverbSeconds}
              min={0.2}
              max={5}
              step={0.1}
              unit="s"
              onChange={(v) => change("reverbSeconds", v)}
            />
            <p className="small">
              Echo and reverb can be combined. Their tails are included when
              saving.
            </p>
          </section>
        </fieldset>
        <p className="small editor-save-note">
          {info.managed
            ? "Sample Edits remain reversible over the complete Original. Rample preview uses mono PCM16 at 44.1 kHz, including effect tails. Original bytes and stereo information are preserved."
            : "MONO · PCM16 WAV · 44.1 kHz. Save to sample bakes these edits into the original WAV. A recoverable backup is made first. Closing discards unsaved edits."}
        </p>
        {info.managed && (
          <section aria-label="Shared Sample references">
            <p>
              Overwrite updates all {info.managed.references.length} referring
              Layers:
            </p>
            <ul>
              {info.managed.references.map((reference, i) => (
                <li key={i}>{reference}</li>
              ))}
            </ul>
            <label>
              Variant name
              <input
                aria-label="Variant name"
                value={variantName}
                disabled={busy}
                onChange={(e) => setVariantName(e.target.value)}
              />
            </label>
            <p>
              Save as new creates an independent Sample and changes only this
              Layer.
            </p>
          </section>
        )}
        {managedSaved && (
          <p role="status">Sample Edits saved. Original preserved.</p>
        )}
        {invalidated && !error && (
          <p className="import-error" role="alert">
            The source changed on disk. Close the editor and reopen the sample
            before saving.
          </p>
        )}
        {saved && (
          <p className="editor-saved" role="status">
            Sample saved. Original backed up.
            <br />
            <small title={saved.backup}>
              {saved.clipped
                ? `${saved.clipped} output frames clipped. Restore the backup and lower Gain if needed.`
                : "Your edits are now part of the WAV, including effect tails."}
            </small>
            <button
              className="text-button"
              onClick={() =>
                void native.showEditBackups().catch((e) => setError(message(e)))
              }
            >
              Show edit backups
            </button>
          </p>
        )}
        {error && (
          <p className="import-error" role="alert">
            {error}
          </p>
        )}
        <div className="modal-buttons">
          <button
            className="secondary"
            disabled={busy || invalidated}
            onClick={reset}
          >
            <RotateCcw size={13} />
            Reset edits
          </button>
          <button
            className="secondary"
            disabled={busy}
            onClick={() => void close()}
          >
            {dirty ? "Discard edits" : "Close"}
          </button>
          {info.managed && (
            <button
              disabled={
                busy || invalidated || !dirty || !enabled || !variantName.trim()
              }
              onClick={() => void save(true)}
            >
              Save as new
            </button>
          )}
          <button
            className="primary"
            disabled={busy || invalidated || !dirty || !enabled}
            onClick={() => void save()}
          >
            {busy ? (
              <LoaderCircle size={14} className="spin" />
            ) : (
              <Save size={14} />
            )}
            {busy
              ? "Saving…"
              : info.managed
                ? "Overwrite shared Sample"
                : "Save to sample"}
          </button>
        </div>
      </div>
    </Modal>
  );
}
function Slider({
  label,
  value,
  min,
  max,
  step,
  unit,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  unit: string;
  onChange: (v: number) => void;
}) {
  return (
    <label className="edit-slider">
      <span>
        {label}
        <span>
          <NumberInput
            label={`${label} value`}
            min={min}
            max={max}
            step={step}
            value={value}
            onChange={onChange}
          />
          {unit}
        </span>
      </span>
      <input
        type="range"
        aria-label={label}
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </label>
  );
}
function NumberInput({
  label,
  value,
  min,
  max,
  step,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number | string;
  onChange: (value: number) => void;
}) {
  const [draft, setDraft] = useState(String(value));
  const [focused, setFocused] = useState(false);
  return (
    <input
      type="number"
      aria-label={label}
      min={min}
      max={max}
      step={step}
      value={focused ? draft : Number(value.toFixed(6))}
      onFocus={() => {
        setDraft(String(value));
        setFocused(true);
      }}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => {
        setFocused(false);
        const parsed = draft.trim() === "" ? NaN : Number(draft);
        if (Number.isFinite(parsed)) {
          const next = Math.max(min, Math.min(max, parsed));
          if (next !== value) onChange(next);
        }
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          event.currentTarget.blur();
        }
      }}
    />
  );
}
function TrimWaveform({
  peaks,
  duration,
  start,
  end,
  disabled,
  onChange,
}: {
  peaks: number[];
  duration: number;
  start: number;
  end: number;
  disabled: boolean;
  onChange: (key: keyof EditSettings, value: number) => void;
}) {
  const drag = useRef<"start" | "end" | null>(null);
  const x1 = (start / duration) * 1000,
    x2 = (end / duration) * 1000;
  return (
    <svg
      className="editor-waveform"
      viewBox="0 0 1000 140"
      role="img"
      aria-label="Sample waveform and trim region"
      onPointerDown={(e) => {
        if (disabled) return;
        const rect = e.currentTarget.getBoundingClientRect();
        const x = ((e.clientX - rect.left) / rect.width) * 1000;
        drag.current = Math.abs(x - x1) < Math.abs(x - x2) ? "start" : "end";
        e.currentTarget.setPointerCapture?.(e.pointerId);
        onChange(
          drag.current,
          Math.max(0, Math.min(duration, (x / 1000) * duration)),
        );
      }}
      onPointerMove={(e) => {
        if (!drag.current || disabled) return;
        const rect = e.currentTarget.getBoundingClientRect();
        onChange(
          drag.current,
          Math.max(
            0,
            Math.min(
              duration,
              ((e.clientX - rect.left) / rect.width) * duration,
            ),
          ),
        );
      }}
      onPointerUp={() => {
        drag.current = null;
      }}
      onPointerCancel={() => {
        drag.current = null;
      }}
    >
      <rect width="1000" height="140" fill="#12171a" />
      <rect
        x={x1}
        width={Math.max(0, x2 - x1)}
        height="140"
        fill="#83c9ad"
        opacity=".09"
      />
      {peaks.map((p, i) => (
        <line
          key={i}
          x1={((i + 0.5) * 1000) / peaks.length}
          x2={((i + 0.5) * 1000) / peaks.length}
          y1={70 - p * 55}
          y2={70 + p * 55}
          stroke="#83c9ad"
          strokeWidth="3"
        />
      ))}
      <rect width={x1} height="140" fill="#111316" opacity=".7" />
      <rect x={x2} width={1000 - x2} height="140" fill="#111316" opacity=".7" />
      {[x1, x2].map((x, i) => (
        <g key={i}>
          <line
            x1={x}
            x2={x}
            y1="0"
            y2="140"
            stroke="#f68061"
            strokeWidth="3"
          />
          <rect
            x={Math.max(0, Math.min(984, x - 8))}
            y="0"
            width="16"
            height="22"
            rx="3"
            fill="#f68061"
          />
        </g>
      ))}
    </svg>
  );
}
