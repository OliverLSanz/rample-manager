// The 4×4 step grid of the app icon and the website hero.
import { theme } from "../theme";

const pattern = [
  [1, 0, 1, 0],
  [0, 1, 0, 1],
  [1, 1, 1, 1],
  [0, 0, 1, 0],
];

/** `column` is the playing column; `hit` (0–1) how hard it just played. */
export function Pads({ size, column, hit, lit = 1 }: { size: number; column: number; hit: number; lit?: number }) {
  return (
    <div
      style={{
        width: size,
        height: size,
        padding: size * 0.09,
        borderRadius: size * 0.22,
        background: "linear-gradient(160deg, #262c34, #1a1e24)",
        boxShadow: "0 40px 80px -30px #000c, inset 0 1px 0 #ffffff12",
        display: "grid",
        gridTemplateColumns: "repeat(4, 1fr)",
        gap: size * 0.045,
      }}
    >
      {pattern.flatMap((row, r) =>
        row.map((on, c) => {
          const color = theme.voices[r];
          const playing = on && c === column ? hit : 0;
          return (
            <div
              key={`${r}-${c}`}
              style={{
                borderRadius: "18%",
                background: on ? color : "#2b3139",
                opacity: on ? 0.35 + 0.65 * lit : 1,
                boxShadow: `inset 0 -2px 0 #0000002e${playing ? `, 0 0 ${size * 0.08 * playing}px ${color}, inset 0 0 0 2px #ffffff${Math.round(170 * playing).toString(16).padStart(2, "0")}` : ""}`,
                filter: playing ? `brightness(${1 + 0.18 * playing})` : undefined,
              }}
            />
          );
        }),
      )}
    </div>
  );
}
