// Opening: a wall of badly named sample files, clicking out of time. The rewind
// pulls it all back into one point, where the title lands.
import { Easing } from "remotion";
import { REWIND_STEPS, SCENES, chaosClicks, random } from "../score";
import { theme } from "../theme";
import { ramp, useStep } from "../timing";

const roots = [
  "kick", "Kick", "KICK", "snare", "snr", "hat", "HH_closed", "clap", "perc", "rim",
  "bass", "sub", "808", "tom", "shaker", "ride", "crash", "fx", "noise", "vox",
  "stab", "chord", "riff", "loop", "break", "foley", "click", "zap", "boom", "tick",
];
const tails = [
  "_final", "_final_v3 (2)", "_NEW", "_old", " copy", "_use this", "_120bpm", "_Cmin", "_wet",
  "_dry", "_mono", "_stereo", "_v2", "_bounce", "_take4", "_edit", "_ok", " (1)", "_LOUD", "_x",
];
const exts = [".wav", ".wav", ".wav", ".aif", ".WAV", ".flac", ".mp3", ".aiff"];

const rand = random(42);
const COLUMNS = 13;
const ROWS = 30;
const columns = Array.from({ length: COLUMNS }, (_, c) => ({
  speed: (0.35 + rand() * 0.6) * (c % 2 ? 1 : -1),
  offset: rand() * 400,
  names: Array.from({ length: ROWS }, () => {
    const n = String(Math.floor(rand() * 99) + 1).padStart(rand() > 0.5 ? 2 : 1, "0");
    const root = roots[Math.floor(rand() * roots.length)];
    const tail = tails[Math.floor(rand() * tails.length)];
    const ext = exts[Math.floor(rand() * exts.length)];
    return rand() > 0.5 ? `${root}_${n}${tail}${ext}` : `${n} ${root}${tail}${ext}`;
  }),
  opacity: 0.18 + rand() * 0.3,
}));
const flashes = chaosClicks().map((at, i) => ({
  at,
  column: (i * 5 + 3) % COLUMNS,
  row: (i * 7 + 11) % ROWS,
  color: theme.voices[i % 4],
}));

const ROW_HEIGHT = 44;

export function Chaos() {
  const step = useStep();
  const { from, to } = SCENES.chaos;
  const rewindStart = to - REWIND_STEPS;
  // Scroll forwards, then the rewind spins the wall backwards and faster.
  const rewind = ramp(step, rewindStart, to, Easing.in(Easing.quad));
  const travel = step * 6 - rewind * rewind * 900;
  const collapse = ramp(step, to - 5, to, Easing.in(Easing.cubic));
  const intro = ramp(step, from, from + 3);

  const line1 = ramp(step, 2, 5);
  const line2 = ramp(step, 12, 15);
  const textOut = ramp(step, rewindStart + 4, to - 3, Easing.in(Easing.cubic));

  return (
    <div style={{ position: "absolute", inset: 0, overflow: "hidden", background: theme.bg }}>
      <div
        style={{
          position: "absolute",
          inset: -200,
          display: "flex",
          gap: 36,
          justifyContent: "center",
          transform: `perspective(1400px) rotateX(18deg) rotateZ(-6deg) scale(${(1 - collapse * 0.92) * (1.05 + step * 0.002)})`,
          opacity: intro * (1 - collapse),
          filter: `blur(${rewind * 3}px)`,
        }}
      >
        {columns.map((column, c) => {
          const y = ((column.offset + travel * column.speed) % (ROWS * ROW_HEIGHT) + ROWS * ROW_HEIGHT) % (ROWS * ROW_HEIGHT);
          return (
            <div key={c} style={{ position: "relative", width: 300, flexShrink: 0 }}>
              <div style={{ transform: `translateY(${y - ROWS * ROW_HEIGHT}px)` }}>
                {[0, 1].map((copy) =>
                  column.names.map((name, r) => {
                    const flash = flashes.find((f) => f.column === c && f.row === r);
                    const glow = flash ? Math.max(0, 1 - (step - flash.at) / 4) * (step >= flash.at ? 1 : 0) : 0;
                    return (
                      <div
                        key={`${copy}-${r}`}
                        style={{
                          height: ROW_HEIGHT,
                          font: `500 19px ${theme.mono}`,
                          whiteSpace: "nowrap",
                          color: glow > 0 ? flash!.color : theme.soft,
                          opacity: column.opacity + glow * (1 - column.opacity),
                          textShadow: glow > 0 ? `0 0 ${24 * glow}px ${flash!.color}` : undefined,
                        }}
                      >
                        {name}
                      </div>
                    );
                  }),
                )}
              </div>
            </div>
          );
        })}
      </div>
      <div
        style={{
          position: "absolute",
          inset: 0,
          background: `radial-gradient(ellipse 50% 42% at 50% 50%, ${theme.bg}f2 30%, ${theme.bg}00 100%)`,
          opacity: intro,
        }}
      />
      <div
        style={{
          position: "absolute",
          inset: 0,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: 6,
          opacity: 1 - textOut,
          transform: `scale(${1 - textOut * 0.2})`,
          filter: `blur(${textOut * 8}px)`,
        }}
      >
        <h1 style={{ ...headline, opacity: line1, transform: `translateY(${(1 - line1) * 24}px)` }}>
          Thousands of samples.
        </h1>
        <h1
          style={{
            ...headline,
            opacity: line2,
            transform: `translateY(${(1 - line2) * 24}px)`,
            background: theme.gradient,
            WebkitBackgroundClip: "text",
            color: "transparent",
          }}
        >
          Four voices.
        </h1>
      </div>
    </div>
  );
}

const headline: React.CSSProperties = {
  margin: 0,
  font: `600 104px/1.08 ${theme.display}`,
  letterSpacing: "-0.03em",
  color: theme.text,
};
