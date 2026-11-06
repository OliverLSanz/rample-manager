// The kit in context: the app's real sequencer plays the four voices together, so
// you hear how the sounds fit before anything goes to the SD card. One voice is
// programmed per bar, then a Layer is swapped and the snare opened in the editor.
import { Grid2X2, ChevronDown, Settings2, SlidersHorizontal } from "lucide-react";
import { useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { continueRender, delayRender, Easing } from "remotion";
import "../../../frontend/style.css";
import { SequencerRow, TransportBar } from "../../../frontend/Sequencer";
import { idleTransport, type Voice } from "../../../frontend/types";
import { LAYER_SWAP, SCENES, files, voices, writes } from "../score";
import { theme } from "../theme";
import { ramp, useLevels, useStep, waveforms } from "../timing";
import { Caption, Path } from "../ui";
import { Glow } from "./Title";

const ZOOM = 1.4;
const allWrites = writes();
const noop = async () => {};
const { from, to } = SCENES.sequencer;
/** The pointer opens the snare in the sample editor here. */
export const EDIT_CLICK = to - 2;

const CHIPS = { left: 120, top: 266, width: 1680, gap: 22, height: 96 };
const chipX = (i: number) => CHIPS.left + i * ((CHIPS.width - 3 * CHIPS.gap) / 4 + CHIPS.gap);
const CHIP_WIDTH = (CHIPS.width - 3 * CHIPS.gap) / 4;

export function SequencerScene() {
  const step = useStep();
  const local = step - from;
  const playhead = local >= 0 && step < to ? Math.floor(local) % 16 : -1;
  const levels = useLevels();

  const enter = ramp(step, from - 2, from, Easing.out(Easing.cubic));
  const exit = ramp(step, to - 2, to, Easing.in(Easing.cubic));

  const sessionVoices: Voice[] = voices.map((voice, index) => {
    const on = Array.from({ length: 16 }, (_, s) =>
      allWrites.some((w) => w.voice === index && w.step === s && w.at <= step),
    );
    return {
      kitID: "D0",
      samplePath: voice.file,
      steps: on,
      volume: 0.8,
      muted: false,
      solo: false,
      layerMode: "manual",
      velocities: Array.from({ length: 16 }, (_, s) => voice.velocity(s)),
    };
  });

  const chipStops = [
    { at: LAYER_SWAP - 3, x: chipX(0) + 200, y: CHIPS.top + 150 },
    { at: LAYER_SWAP, x: chipX(0) + 120, y: CHIPS.top + 62, click: true },
    { at: EDIT_CLICK - 3, x: chipX(1) + 200, y: CHIPS.top + 120 },
    { at: EDIT_CLICK, x: chipX(1) + CHIP_WIDTH - 32, y: CHIPS.top + 26, click: true },
  ];

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        background: theme.bg,
        opacity: enter,
        transform: `translateY(${(1 - enter) * 50}px) scale(${1 + exit * 0.04})`,
        transformOrigin: `${chipX(1) + CHIP_WIDTH / 2}px ${CHIPS.top}px`,
        filter: exit > 0 ? `blur(${exit * 10}px)` : undefined,
      }}
    >
      <Glow />
      <Caption
        eyebrow="PREVIEW · YOUR KIT IN CONTEXT"
        step={step}
        lines={[
          { at: from, text: "Your kit. In context." },
          { at: from + 16, text: "Hear how your sounds fit together…" },
          { at: from + 32, text: "…before they ever reach the Rample." },
          { at: from + 48, text: "No more SD card back and forth." },
          { at: LAYER_SWAP, text: "Swap a Layer. Hear it instantly." },
          { at: from + 80, text: "Then fine-tune any sound." },
        ]}
      />
      <div style={{ position: "absolute", left: CHIPS.left, width: CHIPS.width, top: CHIPS.top, display: "flex", gap: CHIPS.gap }}>
        {voices.map((voice, i) => (
          <VoiceChip key={i} index={i} step={step} level={levels.voice(i)} name={voice.name} file={voice.file} />
        ))}
      </div>
      <Panel voices={sessionVoices} playhead={playhead} peak={levels.master} step={step} />
      <Path stops={chipStops} step={step} />
    </div>
  );
}

function VoiceChip({ index, step, level, name, file }: { index: number; step: number; level: number; name: string; file: string }) {
  const color = theme.voices[index];
  const first = Math.min(...allWrites.filter((w) => w.voice === index).map((w) => w.at));
  const assigned = index === 0 ? 1 : ramp(step, first - 3, first - 0.5);
  // Voice 1 has two Layers; the swap rolls Layer 01 out and Layer 02 in.
  const swap = index === 0 ? ramp(step, LAYER_SWAP - 0.3, LAYER_SWAP + 1, Easing.out(Easing.back(1.4))) : 0;
  const names = index === 0 ? [name, "Kick 02 punch"] : [name];
  const peaks = waveforms[swap > 0.5 ? files.kickPunch : file] ?? [];
  const editPress = index === 1 ? Math.max(0, 1 - Math.abs(step - EDIT_CLICK) / 0.6) : 0;
  return (
    <div
      style={{
        flex: 1,
        height: CHIPS.height,
        background: theme.panel,
        border: `1px solid ${theme.line}`,
        borderTop: `3px solid ${color}`,
        borderRadius: 14,
        padding: "14px 20px",
        boxShadow: level > 0.02 ? `0 0 ${40 * level}px -6px ${color}` : undefined,
        position: "relative",
        overflow: "hidden",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 14, font: `600 13px ${theme.mono}`, letterSpacing: "0.12em", color }}>
        <span style={{ flex: 1 }}>● VOICE {String(index + 1).padStart(2, "0")}</span>
        <span style={{ color: theme.muted, opacity: assigned }}>
          {index === 0 ? `LAYER 0${swap > 0.5 ? 2 : 1} / 02` : "1 LAYER"}
        </span>
        <span
          style={{
            display: "grid",
            placeItems: "center",
            width: 26,
            height: 22,
            borderRadius: 6,
            color: editPress > 0 ? "#121519" : theme.muted,
            background: editPress > 0 ? color : "transparent",
            opacity: assigned,
          }}
        >
          <SlidersHorizontal size={15} />
        </span>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 18, marginTop: 14 }}>
        <div style={{ font: `500 20px ${theme.mono}`, color: theme.soft, width: 250, height: 28, whiteSpace: "nowrap", overflow: "hidden", position: "relative" }}>
          {assigned > 0 ? (
            names.map((n, i) => (
              <div key={n} style={{ position: "absolute", top: (i - swap) * 30, color: i === 1 ? theme.text : undefined }}>
                {i === 0 ? n.slice(0, Math.ceil(n.length * assigned)) : n}
              </div>
            ))
          ) : (
            <span style={{ color: "#4b535d" }}>Empty</span>
          )}
        </div>
        <div style={{ flex: 1, height: 34, display: "flex", alignItems: "center", gap: 1.5, opacity: assigned }}>
          {peaks.filter((_, i) => i % 2 === 0).map((p, i) => (
            <i
              key={i}
              style={{
                flex: 1,
                height: `${Math.max(4, p * 100 * assigned)}%`,
                background: color,
                opacity: 0.55 + level * 0.45,
                borderRadius: 1,
              }}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function Panel({ voices: list, playhead, peak, step }: { voices: Voice[]; playhead: number; peak: number; step: number }) {
  const grid = useRef<HTMLDivElement>(null);
  const [cells, setCells] = useState<{ x: number; y: number }[][] | null>(null);
  const [handle] = useState(() => delayRender("Measure sequencer steps"));

  // Step positions (inside the zoomed panel) for the pointer that clicks them in.
  useLayoutEffect(() => {
    const root = grid.current!;
    const offset = (el: HTMLElement) => {
      let x = el.offsetWidth / 2;
      let y = el.offsetHeight / 2;
      for (let n: HTMLElement | null = el; n && n !== root; n = n.offsetParent as HTMLElement | null) {
        x += n.offsetLeft;
        y += n.offsetTop;
      }
      return { x, y };
    };
    const rows = Array.from(root.querySelectorAll<HTMLElement>(".sequence-row"));
    setCells(rows.map((row) => Array.from(row.querySelectorAll<HTMLElement>(".step")).map(offset)));
    continueRender(handle);
  }, [handle]);

  const transport = { ...idleTransport, running: playhead >= 0, step: playhead, peak, sampleRate: 44100 };
  return (
    // `zoom` also scales the offsets, so the panel is placed by an unzoomed parent.
    <div style={{ position: "absolute", left: 120, top: 396, width: 1680 }}>
      <div ref={grid} style={{ position: "relative", zoom: ZOOM }}>
        <section className="sequencer" aria-label="16-step sequencer" style={{ boxShadow: "0 40px 90px -40px #000" }}>
          <div className="sequencer-heading">
            <Grid2X2 size={15} />
            <h2>Sequencer</h2>
            <span>16 STEPS · 4/4 · 1/16</span>
            <div className="pattern-menu">
              <Settings2 size={14} />
              <span>Pattern</span>
              <ChevronDown size={11} />
            </div>
          </div>
          <div className="step-grid">
            <div className="step-numbers">
              <span>VOICE / LEVEL</span>
              <div className="steps">
                {Array.from({ length: 16 }, (_, i) => (
                  <span className={`${i % 4 === 0 ? "beat" : ""} ${playhead === i ? "current" : ""}`} key={i}>
                    {String(i + 1).padStart(2, "0")}
                  </span>
                ))}
              </div>
            </div>
            {list.map((voice, channel) => (
              <SequencerRow key={channel} channel={channel} voice={voice} playhead={playhead} onAction={noop} onVelocity={() => {}} />
            ))}
            <div className="position-row">
              <span />
              <div className="steps">
                {Array.from({ length: 16 }, (_, i) => (
                  <i key={i} className={playhead === i ? "current" : ""} />
                ))}
              </div>
            </div>
          </div>
          <TransportBar playing={playhead >= 0} bpm={110} transport={transport} onAction={noop} />
        </section>
        {cells && <Pointer cells={cells} step={step} />}
        <WriteBursts cells={cells} step={step} />
      </div>
    </div>
  );
}

/** The mouse pointer glides to each step and clicks it in just as it arrives. */
function Pointer({ cells, step }: { cells: { x: number; y: number }[][]; step: number }) {
  const last = allWrites.length - 1;
  const k = allWrites.findLastIndex((w) => w.at <= step);
  const cell = (i: number) => cells[allWrites[i].voice][allWrites[i].step];
  let { x, y } = cell(Math.max(0, k));
  if (k >= 0 && k < last) {
    const prev = allWrites[k];
    const next = allWrites[k + 1];
    const move = Math.min(0.7, next.at - prev.at - 0.1);
    const t = Easing.inOut(Easing.cubic)(Math.min(1, Math.max(0, (step - (next.at - move)) / move)));
    const a = cell(k);
    const b = cell(k + 1);
    x = a.x + (b.x - a.x) * t;
    y = a.y + (b.y - a.y) * t;
  }
  const press = Math.max(0, ...allWrites.map((w) => 1 - Math.abs(step - w.at) / 0.25));
  const appear = ramp(step, allWrites[0].at - 3, allWrites[0].at - 1);
  const fade = ramp(step, allWrites[last].at + 2, allWrites[last].at + 5);
  return (
    <svg
      width="22"
      height="28"
      viewBox="0 0 22 28"
      style={{
        position: "absolute",
        left: x - 3,
        top: y - 2,
        transform: `scale(${1 - press * 0.15})`,
        transformOrigin: "3px 2px",
        opacity: appear * (1 - fade),
        filter: "drop-shadow(0 3px 5px #000a)",
      }}
    >
      <path d="M3 2 L3 22 L8.2 17.4 L11.6 25.2 L15 23.7 L11.7 16.1 L18.6 16 Z" fill="#fff" stroke="#111316" strokeWidth="1.4" strokeLinejoin="round" />
    </svg>
  );
}

/** A ring that expands from each step as it is clicked in. */
function WriteBursts({ cells, step }: { cells: { x: number; y: number }[][] | null; step: number }) {
  if (!cells) return null;
  return (
    <>
      {allWrites.map((w, i) => {
        const age = step - w.at;
        if (age < 0 || age > 2) return null;
        const t = age / 2;
        const c = cells[w.voice][w.step];
        return (
          <div
            key={i}
            style={
              {
                position: "absolute",
                left: c.x - 30,
                top: c.y - 30,
                width: 60,
                height: 60,
                borderRadius: 14,
                border: `2px solid ${theme.voices[w.voice]}`,
                opacity: 1 - t,
                transform: `scale(${0.5 + t * 0.9})`,
              } as CSSProperties
            }
          />
        );
      })}
    </>
  );
}
