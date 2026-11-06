// The sample editor, quickly: one edit per beat on the snare, heard as it changes,
// then saved as a new Variant while the Original stays as it was.
import { Check, SlidersHorizontal } from "lucide-react";
import { Easing } from "remotion";
import { SCENES, editAt, edits } from "../score";
import { theme } from "../theme";
import { editorWaveforms, ramp, useLevels, useStep } from "../timing";
import { Caption, Path, fmt, panel, sceneStyle } from "../ui";
import { Glow } from "./Title";

const { from, to } = SCENES.editor;
const PANEL = { left: 120, top: 270, width: 1680, height: 750 };
const GREEN = theme.voices[1];
const SAVE_CLICK = to - 8;
const WINDOW_SECONDS = 1.6;

/** Slider values before and after their edit. */
const sliders = [
  { section: 0, label: "Gain", unit: "dB", from: 0, to: -1.5, min: -24, max: 12, edit: 0, digits: 1 },
  { section: 0, label: "Pitch", unit: "st", from: 0, to: -4, min: -12, max: 12, edit: 1, digits: 0 },
  { section: 1, label: "Low · 120 Hz", unit: "dB", from: 0, to: 5, min: -12, max: 12, edit: 2, digits: 0 },
  { section: 1, label: "High · 8 kHz", unit: "dB", from: 0, to: -4, min: -12, max: 12, edit: 2, digits: 0 },
  { section: 2, label: "Echo amount", unit: "%", from: 0, to: 35, min: 0, max: 100, edit: 3, digits: 0 },
  { section: 2, label: "Echo time", unit: "ms", from: 120, to: 205, min: 20, max: 1000, edit: 3, digits: 0 },
  { section: 3, label: "Reverb amount", unit: "%", from: 0, to: 38, min: 0, max: 100, edit: 4, digits: 0 },
  { section: 3, label: "Reverb decay", unit: "s", from: 0.5, to: 1.5, min: 0.1, max: 6, edit: 4, digits: 1 },
];
const sections = ["Level & pitch", "Equalizer", "Echo", "Reverb"];
/** The edit that brings each section to life (Trim lights the waveform). */
const sectionEdit = [1, 2, 3, 4];

export function Editor() {
  const step = useStep();
  const levels = useLevels();
  const level = levels.voice(1);
  const enter = ramp(step, from - 2, from + 0.5, Easing.out(Easing.back(1.3)));

  const saveX = PANEL.left + PANEL.width - 377;
  const saveY = PANEL.top + PANEL.height - 44;
  const stops = [
    { at: SAVE_CLICK - 3, x: saveX - 200, y: saveY - 160 },
    { at: SAVE_CLICK, x: saveX, y: saveY, click: true },
  ];
  const saved = step >= SAVE_CLICK;

  return (
    <div style={sceneStyle(step, from, to)}>
      <Glow />
      <Caption
        eyebrow="SAMPLE EDITOR · NON-DESTRUCTIVE"
        color={GREEN}
        step={step}
        lines={[
          { at: from, text: <Words step={step} /> },
          { at: editAt(edits.length) + 4, text: "Your originals stay untouched." },
        ]}
      />
      <div
        style={{
          ...panel,
          position: "absolute",
          ...PANEL,
          overflow: "hidden",
          transform: `scale(${0.94 + 0.06 * enter})`,
          opacity: Math.min(1, enter * 1.5),
        }}
      >
        <div style={{ height: 84, display: "flex", alignItems: "center", gap: 16, padding: "0 30px", borderBottom: `1px solid ${theme.line}` }}>
          <SlidersHorizontal size={26} color={theme.accent} />
          <div style={{ font: `600 28px ${theme.display}`, color: theme.text }}>Sample editor</div>
          <div style={{ font: `600 14px ${theme.mono}`, letterSpacing: "0.12em", color: GREEN }}>VOICE 2</div>
          <div style={{ font: `500 20px ${theme.mono}`, color: theme.soft, marginLeft: 24 }}>Snare 03 crack</div>
          <div style={{ marginLeft: "auto", display: "flex", alignItems: "center", gap: 10, font: `500 17px ${theme.body}`, color: theme.text }}>
            <span style={{ width: 22, height: 22, borderRadius: 6, background: "#3b82f6", display: "grid", placeItems: "center" }}>
              <Check size={15} strokeWidth={3} color="#fff" />
            </span>
            Preview edits
            <span style={{ font: `500 15px ${theme.mono}`, color: GREEN, marginLeft: 6 }}>Live preview</span>
          </div>
        </div>
        <WaveformEditor step={step} level={level} />
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 18, padding: "0 30px" }}>
          {sections.map((title, i) => {
            const active = ramp(step, editAt(sectionEdit[i]) - 0.3, editAt(sectionEdit[i]) + 0.5) * (1 - ramp(step, editAt(sectionEdit[i]) + 3, editAt(sectionEdit[i]) + 4.5));
            return (
              <div
                key={title}
                style={{
                  height: 156,
                  borderRadius: 14,
                  padding: "16px 22px",
                  border: `1px solid ${active > 0 ? GREEN : theme.line}`,
                  boxShadow: active > 0 ? `0 0 ${36 * active}px -10px ${GREEN}` : undefined,
                  background: active > 0 ? `rgba(131, 201, 173, ${0.05 * active})` : "transparent",
                }}
              >
                <div style={{ font: `600 19px ${theme.body}`, color: theme.text, marginBottom: 10 }}>{title}</div>
                {sliders
                  .filter((s) => s.section === i)
                  .map((s) => (
                    <Slider key={s.label} step={step} {...s} />
                  ))}
              </div>
            );
          })}
        </div>
        <div style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: 88, display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 14, padding: "0 30px", borderTop: `1px solid ${theme.line}` }}>
          {saved && (
            <div style={{ marginRight: "auto", display: "flex", alignItems: "center", gap: 10, font: `500 18px ${theme.body}`, color: GREEN, opacity: ramp(step, SAVE_CLICK, SAVE_CLICK + 1.5) }}>
              <Check size={20} strokeWidth={2.6} />
              Saved as a new Variant · Original kept
            </div>
          )}
          <FooterButton label="Reset edits" />
          <FooterButton label="Save as new" press={Math.max(0, 1 - Math.abs(step - SAVE_CLICK) / 0.6)} highlight={saved} />
          <div style={{ height: 50, padding: "0 22px", borderRadius: 10, display: "flex", alignItems: "center", background: theme.accent, color: "#121519", font: `700 17px ${theme.body}` }}>
            Overwrite shared Sample
          </div>
        </div>
      </div>
      <Path stops={stops} step={step} />
    </div>
  );
}

/** "Trim. Pitch. EQ. Echo. Reverb." — each word lands with its edit. */
function Words({ step }: { step: number }) {
  return (
    <span style={{ display: "flex", gap: 26 }}>
      {edits.map((word, i) => {
        const t = ramp(step, editAt(i) - 0.4, editAt(i) + 0.8, Easing.out(Easing.back(2)));
        const now = i === Math.min(edits.length - 1, Math.floor((step - from) / 4));
        return (
          <span
            key={word}
            style={{
              opacity: Math.min(1, t),
              transform: `translateY(${(1 - t) * 26}px) scale(${0.8 + 0.2 * t})`,
              display: "inline-block",
              color: now ? GREEN : theme.text,
            }}
          >
            {word}
          </span>
        );
      })}
    </span>
  );
}

function WaveformEditor({ step, level }: { step: number; level: number }) {
  const stage = Math.min(edits.length, Math.max(0, Math.floor((step - from) / 4) + 1));
  const blend = stage === 0 ? 1 : ramp(step, editAt(stage - 1) - 0.2, editAt(stage - 1) + 1);
  const prev = editorWaveforms[Math.max(0, stage - 1)];
  const next = editorWaveforms[stage];
  const original = editorWaveforms[0];
  const peaks = next.map((p, i) => prev[i] + (p - prev[i]) * blend);
  // Trim handle in output time: the trimmed snare, slowed down by the pitch edit.
  const trimmed = 0.24 * (stage >= 2 ? 2 ** (4 / 12) : 1);
  const endSeconds = stage === 0 ? 0.51 : 0.51 + (trimmed - 0.51) * ramp(step, editAt(0) - 0.2, editAt(0) + 1.2);
  const handle = endSeconds / WINDOW_SECONDS;
  const trimActive = ramp(step, editAt(0) - 0.3, editAt(0) + 0.5) * (1 - ramp(step, editAt(0) + 3, editAt(0) + 4.5));
  return (
    <div style={{ padding: "22px 30px 20px" }}>
      <div
        style={{
          position: "relative",
          height: 190,
          borderRadius: 12,
          background: "#141a1a",
          border: `1px solid ${trimActive > 0 ? GREEN : theme.line}`,
          boxShadow: trimActive > 0 ? `0 0 ${36 * trimActive}px -10px ${GREEN}` : undefined,
          overflow: "hidden",
        }}
      >
        <div style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: `${handle * 100}%`, background: "#83c9ad14" }} />
        <div style={{ position: "absolute", inset: "0 10px", display: "flex", alignItems: "center", gap: 2 }}>
          {peaks.map((p, i) => {
            const inside = i / peaks.length < handle;
            const ghost = stage >= 1 && !inside ? original[i] : 0;
            const h = Math.max(inside ? p : Math.max(p, ghost), 0.004);
            return (
              <i
                key={i}
                style={{
                  flex: 1,
                  height: `${Math.max(1.5, Math.sqrt(h) * 92)}%`,
                  borderRadius: 2,
                  background: GREEN,
                  opacity: inside ? 0.75 + level * 0.25 : p > ghost ? 0.5 : 0.16,
                }}
              />
            );
          })}
        </div>
        {[0, handle].map((x, i) => (
          <div key={i} style={{ position: "absolute", top: 0, bottom: 0, left: `calc(${x * 100}% + ${i ? 0 : 2}px)`, width: 3, background: theme.accent }}>
            <div style={{ position: "absolute", top: 0, left: -7, width: 17, height: 24, borderRadius: 4, background: theme.accent }} />
          </div>
        ))}
        <div style={{ position: "absolute", right: 16, bottom: 12, font: `500 15px ${theme.mono}`, color: theme.muted }}>
          Output with tails {fmt(outputSeconds(stage), 2)} s
        </div>
      </div>
    </div>
  );
}

function outputSeconds(stage: number) {
  const base = stage === 0 ? 0.51 : 0.24 * (stage >= 2 ? 2 ** (4 / 12) : 1);
  return base + (stage >= 4 ? 1.02 : 0) + (stage >= 5 ? 1.8 : 0);
}

function Slider({ step, label, unit, from: a, to: b, min, max, edit, digits }: (typeof sliders)[number] & { step: number }) {
  const t = ramp(step, editAt(edit) - 0.3, editAt(edit) + 1.4, Easing.inOut(Easing.cubic));
  const value = a + (b - a) * t;
  const x = (value - min) / (max - min);
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 18, height: 46 }}>
      <div style={{ width: 170, font: `500 16px ${theme.body}`, color: theme.soft }}>{label}</div>
      <div style={{ flex: 1, position: "relative", height: 6, borderRadius: 3, background: "#2b3139" }}>
        <div style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: `${x * 100}%`, borderRadius: 3, background: t > 0 ? `${GREEN}aa` : "#3a414b" }} />
        <div style={{ position: "absolute", top: -7, left: `calc(${x * 100}% - 10px)`, width: 20, height: 20, borderRadius: 10, background: "#e8ebee", boxShadow: "0 2px 6px #0008" }} />
      </div>
      <div style={{ width: 96, textAlign: "right", font: `500 17px ${theme.mono}`, color: t > 0 ? theme.text : theme.muted }}>
        {fmt(value, digits)} <span style={{ color: theme.muted, fontSize: 14 }}>{unit}</span>
      </div>
    </div>
  );
}

function FooterButton({ label, press = 0, highlight }: { label: string; press?: number; highlight?: boolean }) {
  return (
    <div
      style={{
        height: 50,
        padding: "0 20px",
        borderRadius: 10,
        display: "flex",
        alignItems: "center",
        background: highlight ? `${GREEN}30` : theme.raised,
        color: highlight ? GREEN : theme.text,
        font: `600 17px ${theme.body}`,
        transform: `scale(${1 - press * 0.06})`,
      }}
    >
      {label}
    </div>
  );
}
