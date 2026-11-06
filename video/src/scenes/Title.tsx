// The title lands on the first downbeat: the pads keep time with the kick.
import { AudioLines } from "lucide-react";
import { Easing } from "remotion";
import { SCENES } from "../score";
import { useLayout } from "../layout";
import { theme } from "../theme";
import { ramp, useStep } from "../timing";
import { Pads } from "./Pads";

export function Title() {
  const step = useStep();
  const { vertical } = useLayout();
  const { from, to } = SCENES.title;
  const local = step - from;
  const beat = Math.floor(local / 4);
  const sinceBeat = local - beat * 4;
  const hit = Math.max(0, 1 - sinceBeat / 3);

  const enter = ramp(step, from, from + 3, Easing.out(Easing.back(1.6)));
  const exit = ramp(step, to - 3, to, Easing.in(Easing.cubic));
  const flash = Math.max(0, 1 - local / 2.5);
  const lines = ["Your samples.", "Your kits.", "Ready for the Rample."];

  return (
    <div style={{ position: "absolute", inset: 0, background: theme.bg, overflow: "hidden" }}>
      <Glow />
      <div
        style={{
          position: "absolute",
          inset: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexDirection: vertical ? "column" : "row",
          gap: vertical ? 90 : 120,
          opacity: 1 - exit,
          transform: `translateY(${-exit * 80}px)`,
        }}
      >
        <div style={{ transform: `scale(${0.6 + 0.4 * enter + hit * 0.015})` }}>
          <Pads size={vertical ? 400 : 430} column={beat % 4} hit={hit} />
        </div>
        <div style={{ width: vertical ? 900 : 1000 }}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 18,
              color: theme.accent,
              opacity: ramp(step, from + 1, from + 4),
              transform: `translateX(${(1 - ramp(step, from + 1, from + 4)) * -30}px)`,
            }}
          >
            <AudioLines size={58} strokeWidth={2.2} />
            <div style={{ lineHeight: 1 }}>
              <div style={{ font: `700 46px ${theme.display}`, letterSpacing: "0.12em", color: theme.text }}>
                RAMPLE
              </div>
              <div style={{ font: `600 15px ${theme.mono}`, letterSpacing: "0.62em", color: theme.muted, marginTop: 6 }}>
                MANAGER
              </div>
            </div>
          </div>
          <div
            style={{
              font: `600 17px ${theme.mono}`,
              letterSpacing: "0.14em",
              color: theme.accent,
              margin: "54px 0 18px",
              opacity: ramp(step, from + 4, from + 7),
            }}
          >
            FOR SQUARP RAMPLE &amp; RAMPLE TURBO
          </div>
          {lines.map((line, i) => {
            const t = ramp(step, from + 6 + i * 6, from + 9 + i * 6);
            return (
              <div
                key={line}
                style={{
                  font: `600 ${vertical ? 84 : 92}px/1.06 ${theme.display}`,
                  letterSpacing: "-0.03em",
                  opacity: t,
                  transform: `translateY(${(1 - t) * 30}px)`,
                  ...(i === 1
                    ? { background: theme.gradient, WebkitBackgroundClip: "text", color: "transparent" }
                    : { color: theme.text }),
                }}
              >
                {line}
              </div>
            );
          })}
        </div>
      </div>
      <div style={{ position: "absolute", inset: 0, background: "#fff", opacity: flash * 0.28 }} />
    </div>
  );
}

export function Glow() {
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        background: `radial-gradient(circle at 18% 20%, #f6806118, transparent 38%), radial-gradient(circle at 85% 80%, #83c9ad12, transparent 40%), radial-gradient(circle at 70% 10%, #afa0df10, transparent 35%)`,
      }}
    />
  );
}
