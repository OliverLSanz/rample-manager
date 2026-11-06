// The Library: a folder is imported, a search finds the kicks and the arrow keys
// audition them one per beat. The last one is added to Voice 1.
import { ArrowDown, FolderInput, Plus, Search, Volume2 } from "lucide-react";
import { Easing } from "remotion";
import { SCENES, auditionAt, auditions, type LibraryRow } from "../score";
import { theme } from "../theme";
import { ramp, useLevels, useStep, waveforms } from "../timing";
import { Caption, Path, fmt, panel, sceneStyle } from "../ui";
import { Glow } from "./Title";

const { from, to } = SCENES.library;
const PANEL = { left: 120, top: 270, width: 1680, height: 750 };
const HEADER = 84;
const TAGS = 300;
const DETAIL = 470;
const ROW = 60;
const IMPORT_CLICK = from + 1;
const SEARCH_CLICK = from + 19;
const TYPED = "kick";
const typeAt = (i: number) => from + 20 + i * 0.75;
const FILTER = { from: from + 23, to: from + 27 };
const ADD_CLICK = to - 3;

const others: LibraryRow[] = [
  { name: "Pad 01 fifth C3", tags: "#pad #ambient", seconds: 6.96 },
  { name: "Hat 07 closed", tags: "#hats #breakdance", seconds: 0.42 },
  { name: "Drone 06 reed C2", tags: "#drone #ambient", seconds: 13.95 },
  { name: "Bass 01 finger C2", tags: "#bass #breakdance", seconds: 0.8 },
  { name: "Snare 03 crack", tags: "#snare #breakdance", seconds: 0.51 },
  { name: "Particle 01 bell C5", tags: "#tonal #ambient", seconds: 4.0 },
  { name: "Tick 01 noise", tags: "#hats #minimal", seconds: 0.05 },
  { name: "Break 01 classic 2bar", tags: "#breaks #loop", seconds: 4.36 },
];
// Kicks interleaved with other sounds, as an unfiltered Library would list them.
const order = ["k", "o", "o", "k", "o", "k", "o", "o", "k", "k", "o", "k", "o", "k", "o", "k"];
const rows = (() => {
  let k = 0;
  let o = 0;
  return order.map((kind, i) =>
    kind === "k" ? { row: auditions[k], before: i, after: k++ as number | null } : { row: others[o++], before: i, after: null },
  );
})();
const tags = [
  ["#kick", 48],
  ["#industrial", 96],
  ["#breakdance", 48],
  ["#ambient", 64],
  ["#minimal", 84],
  ["#hats", 44],
  ["#snare", 16],
  ["#bass", 12],
  ["#pad", 32],
  ["#loop", 20],
] as const;

export function Library() {
  const step = useStep();
  const levels = useLevels();
  const imported = Math.round(292 * ramp(step, from + 1.5, from + 12, Easing.out(Easing.quad)));
  const filtered = ramp(step, FILTER.from, FILTER.to, Easing.inOut(Easing.cubic));
  const typed = TYPED.slice(0, [0, 1, 2, 3].filter((i) => step >= typeAt(i)).length);
  const selected = auditions.findLastIndex((_, i) => step >= auditionAt(i) - 0.2);
  const current = auditions[Math.max(0, selected)];
  const level = levels.voice(4);

  const detailX = PANEL.left + PANEL.width - DETAIL;
  const stops = [
    { at: IMPORT_CLICK - 2, x: PANEL.left + PANEL.width - 300, y: PANEL.top + 160 },
    { at: IMPORT_CLICK, x: PANEL.left + PANEL.width - 110, y: PANEL.top + HEADER / 2, click: true },
    { at: SEARCH_CLICK, x: PANEL.left + 760, y: PANEL.top + HEADER / 2, click: true },
    { at: SEARCH_CLICK + 4, x: PANEL.left + 900, y: PANEL.top + 260 },
    { at: ADD_CLICK - 3, x: detailX + 300, y: PANEL.top + 560 },
    { at: ADD_CLICK, x: detailX + 250, y: PANEL.top + 498, click: true },
  ];

  return (
    <div style={sceneStyle(step, from, to)}>
      <Glow />
      <Caption
        eyebrow="LIBRARY"
        step={step}
        lines={[
          { at: from, text: "Import every sound once." },
          { at: from + 16, text: "Name it, tag it, find it." },
          { at: from + 32, text: "Hear each one as you browse." },
        ]}
      />
      <div style={{ ...panel, position: "absolute", ...PANEL, overflow: "hidden" }}>
        {/* Header */}
        <div style={{ height: HEADER, display: "flex", alignItems: "center", gap: 22, padding: "0 26px", borderBottom: `1px solid ${theme.line}` }}>
          <div style={{ width: TAGS - 48 }}>
            <div style={{ font: `600 26px ${theme.display}`, color: theme.text }}>Samples</div>
            <div style={{ font: `500 14px ${theme.body}`, color: theme.muted }}>{imported} in the Library</div>
          </div>
          <div
            style={{
              marginLeft: 120,
              width: 420,
              height: 46,
              borderRadius: 10,
              background: theme.raised,
              display: "flex",
              alignItems: "center",
              gap: 12,
              padding: "0 16px",
              font: `500 19px ${theme.body}`,
              color: typed ? theme.text : theme.muted,
              outline: step >= SEARCH_CLICK && step < to ? `1.5px solid ${theme.accent}90` : undefined,
            }}
          >
            <Search size={18} color={theme.muted} />
            {typed || "Search name or tag"}
            {step >= SEARCH_CLICK && step < FILTER.to && Math.floor(step * 2) % 2 === 0 ? (
              <span style={{ width: 2, height: 22, background: theme.accent, marginLeft: -8 }} />
            ) : null}
          </div>
          <div style={{ marginLeft: "auto", display: "flex", gap: 12 }}>
            <Button label="Import audio files…" />
            <Button label="Import folder…" pressed={Math.max(0, 1 - Math.abs(step - IMPORT_CLICK) / 0.6)} icon />
          </div>
        </div>
        {/* Tags */}
        <div style={{ position: "absolute", left: 0, top: HEADER, width: TAGS, bottom: 0, padding: "18px 22px", borderRight: `1px solid ${theme.line}` }}>
          <TagRow label="All Samples" count={imported} active={filtered < 0.5} />
          <div style={{ font: `600 12px ${theme.mono}`, letterSpacing: "0.14em", color: theme.muted, margin: "22px 12px 10px" }}>TAGS</div>
          {tags.map(([label, count], i) => (
            <div key={label} style={{ opacity: ramp(step, from + 6 + i * 0.4, from + 8 + i * 0.4) }}>
              <TagRow label={label} count={Math.round(count * Math.min(1, imported / 292))} active={label === "#kick" && filtered > 0.5} />
            </div>
          ))}
        </div>
        {/* List */}
        <div style={{ position: "absolute", left: TAGS, top: HEADER, width: PANEL.width - TAGS - DETAIL, bottom: 0, padding: "14px 18px", overflow: "hidden" }}>
          {rows.map(({ row, before, after }, i) => {
            const appear = ramp(step, from + 1.5 + i * 0.55, from + 3.5 + i * 0.55);
            const y = after === null ? before * ROW : (before + (after - before) * filtered) * ROW;
            const gone = after === null ? filtered : 0;
            const isSelected = after !== null && after === selected && step >= auditionAt(0) - 0.2;
            return (
              <div
                key={i}
                style={{
                  position: "absolute",
                  left: 18,
                  right: 18,
                  top: 14 + y,
                  height: ROW - 6,
                  borderRadius: 10,
                  padding: "0 18px",
                  display: "flex",
                  alignItems: "center",
                  background: isSelected ? "#3a2a24" : "#ffffff05",
                  boxShadow: isSelected ? `inset 0 0 0 1.5px ${theme.accent}80` : undefined,
                  opacity: appear * (1 - gone),
                  transform: `translateX(${(1 - appear) * 60}px) scale(${1 - gone * 0.04})`,
                }}
              >
                <div style={{ flex: 1 }}>
                  <div style={{ font: `500 19px ${theme.body}`, color: theme.text }}>{row.name}</div>
                  <div style={{ font: `500 13px ${theme.mono}`, color: theme.muted }}>{row.tags}</div>
                </div>
                <div style={{ font: `500 15px ${theme.mono}`, color: theme.soft, width: 120, textAlign: "right" }}>
                  {fmt(row.seconds, 2)} s
                </div>
                <div style={{ width: 70, display: "flex", justifyContent: "flex-end" }}>
                  {isSelected && <Volume2 size={20} color={theme.accent} style={{ opacity: 0.5 + level * 0.5 }} />}
                </div>
              </div>
            );
          })}
        </div>
        {/* Detail */}
        <div style={{ position: "absolute", right: 0, top: HEADER, width: DETAIL, bottom: 0, padding: "26px 28px", borderLeft: `1px solid ${theme.line}`, opacity: ramp(step, FILTER.to - 1, FILTER.to + 1) }}>
          <div style={{ font: `600 28px ${theme.display}`, color: theme.text }}>{current.name}</div>
          <div style={{ font: `500 14px ${theme.mono}`, color: theme.muted, marginTop: 6 }}>WAV · {fmt(current.seconds, 2)} s · 44100 Hz · mono</div>
          <Waveform peaks={waveforms[current.file!] ?? []} level={level} />
          <div style={{ display: "flex", gap: 10, marginTop: 18 }}>
            <Chip label="Rample mono" />
            <Chip label="Original" />
          </div>
          <div style={{ marginTop: 26, padding: 18, borderRadius: 12, border: `1px solid ${theme.line}` }}>
            <div style={{ font: `600 12px ${theme.mono}`, letterSpacing: "0.14em", color: theme.muted }}>ADD TO</div>
            <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 12 }}>
              <div style={{ flex: 1, font: `500 17px ${theme.body}`, color: theme.text, background: theme.raised, borderRadius: 8, padding: "9px 14px" }}>Kit D0</div>
              {[1, 2, 3, 4].map((v) => (
                <div
                  key={v}
                  style={{
                    width: 38,
                    height: 38,
                    borderRadius: 8,
                    display: "grid",
                    placeItems: "center",
                    font: `600 16px ${theme.body}`,
                    background: v === 1 ? theme.accent : "transparent",
                    color: v === 1 ? "#121519" : theme.soft,
                  }}
                >
                  {v}
                </div>
              ))}
            </div>
          </div>
          <AddButton press={Math.max(0, 1 - Math.abs(step - ADD_CLICK) / 0.6)} done={step >= ADD_CLICK} />
        </div>
        {/* Arrow key hint while browsing */}
        <div
          style={{
            position: "absolute",
            left: TAGS + 18,
            bottom: 22,
            display: "flex",
            alignItems: "center",
            gap: 12,
            font: `500 16px ${theme.body}`,
            color: theme.soft,
            opacity: ramp(step, auditionAt(0) - 2, auditionAt(0)) * (1 - ramp(step, ADD_CLICK - 2, ADD_CLICK)),
          }}
        >
          <div
            style={{
              width: 40,
              height: 40,
              borderRadius: 8,
              display: "grid",
              placeItems: "center",
              background: theme.raised,
              border: "1px solid #ffffff1f",
              borderBottomWidth: 4,
              transform: `translateY(${Math.max(0, 1 - Math.abs(step - auditionAt(Math.max(0, selected))) / 0.5) * 3}px)`,
            }}
          >
            <ArrowDown size={18} />
          </div>
          Hear while browsing
        </div>
      </div>
      <Path stops={stops} step={step} />
    </div>
  );
}

function Button({ label, pressed = 0, icon }: { label: string; pressed?: number; icon?: boolean }) {
  return (
    <div
      style={{
        height: 46,
        padding: "0 18px",
        borderRadius: 10,
        display: "flex",
        alignItems: "center",
        gap: 10,
        background: pressed > 0 ? "#3a3f47" : theme.raised,
        font: `600 16px ${theme.body}`,
        color: theme.text,
        transform: `scale(${1 - pressed * 0.05})`,
      }}
    >
      {icon && <FolderInput size={18} />}
      {label}
    </div>
  );
}

function TagRow({ label, count, active }: { label: string; count: number; active: boolean }) {
  return (
    <div
      style={{
        display: "flex",
        justifyContent: "space-between",
        padding: "8px 12px",
        borderRadius: 8,
        font: `500 17px ${theme.body}`,
        color: active ? theme.accent : theme.soft,
        background: active ? "#3a2a24" : "transparent",
      }}
    >
      <span>{label}</span>
      <span style={{ font: `500 14px ${theme.mono}`, color: theme.muted }}>{count}</span>
    </div>
  );
}

function Chip({ label }: { label: string }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "9px 14px", borderRadius: 8, background: theme.raised, font: `600 15px ${theme.body}`, color: theme.text }}>
      <Volume2 size={16} />
      {label}
    </div>
  );
}

function AddButton({ press, done }: { press: number; done: boolean }) {
  return (
    <div
      style={{
        marginTop: 18,
        height: 56,
        borderRadius: 12,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 10,
        background: done ? theme.voices[1] : theme.accent,
        color: "#121519",
        font: `700 19px ${theme.body}`,
        transform: `scale(${1 - press * 0.05})`,
        boxShadow: done ? `0 0 40px -10px ${theme.voices[1]}` : undefined,
      }}
    >
      <Plus size={20} strokeWidth={2.6} />
      {done ? "Added to Voice 1" : "Add as Layer"}
    </div>
  );
}

export function Waveform({ peaks, level, color = theme.accent, height = 130 }: { peaks: number[]; level: number; color?: string; height?: number }) {
  return (
    <div
      style={{
        height,
        marginTop: 20,
        display: "flex",
        alignItems: "center",
        gap: 2,
        filter: level > 0.02 ? `drop-shadow(0 0 ${14 * level}px ${color})` : undefined,
      }}
    >
      {peaks.map((p, i) => (
        <i key={i} style={{ flex: 1, height: `${Math.max(2, Math.sqrt(p) * 100)}%`, background: color, borderRadius: 2, opacity: 0.6 + level * 0.4 }} />
      ))}
    </div>
  );
}
