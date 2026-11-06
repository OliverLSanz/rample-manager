// Pieces shared by the scenes: the caption block, the mouse pointer and the scene fade.
import type { CSSProperties, ReactNode } from "react";
import { Easing } from "remotion";
import { theme } from "./theme";
import { ramp } from "./timing";

export interface Line {
  at: number;
  text: ReactNode;
}

/** Eyebrow plus a headline that changes at each line's step. */
export function Caption({ eyebrow, lines, step, color = theme.accent }: { eyebrow: string; lines: Line[]; step: number; color?: string }) {
  return (
    <div style={{ position: "absolute", left: 120, top: 98, right: 120, height: 150 }}>
      <div style={{ font: `600 17px ${theme.mono}`, letterSpacing: "0.14em", color }}>{eyebrow}</div>
      {lines.map((line, i) => {
        const next = lines[i + 1]?.at ?? 1e6;
        const t = ramp(step, line.at - 0.6, line.at + 1.2);
        const out = ramp(step, next - 2, next - 0.6, Easing.in(Easing.cubic));
        if (t <= 0 || out >= 1) return null;
        return (
          <div
            key={i}
            style={{
              position: "absolute",
              top: 34,
              whiteSpace: "nowrap",
              font: `600 76px/1.1 ${theme.display}`,
              letterSpacing: "-0.03em",
              color: theme.text,
              opacity: t * (1 - out),
              transform: `translateY(${(1 - t) * 28 - out * 28}px)`,
            }}
          >
            {line.text}
          </div>
        );
      })}
    </div>
  );
}

export function Pointer({ x, y, press = 0, opacity = 1, scale = 1 }: { x: number; y: number; press?: number; opacity?: number; scale?: number }) {
  if (opacity <= 0) return null;
  return (
    <svg
      width={22 * scale}
      height={28 * scale}
      viewBox="0 0 22 28"
      style={{
        position: "absolute",
        left: x - 3 * scale,
        top: y - 2 * scale,
        transform: `scale(${1 - press * 0.15})`,
        transformOrigin: `${3 * scale}px ${2 * scale}px`,
        opacity,
        filter: "drop-shadow(0 3px 5px #000a)",
        zIndex: 10,
      }}
    >
      <path d="M3 2 L3 22 L8.2 17.4 L11.6 25.2 L15 23.7 L11.7 16.1 L18.6 16 Z" fill="#fff" stroke="#111316" strokeWidth="1.4" strokeLinejoin="round" />
    </svg>
  );
}

export interface Stop {
  at: number;
  x: number;
  y: number;
  /** Click when the pointer arrives. */
  click?: boolean;
}

/** Pointer that travels between stops, arriving at each stop's step. */
export function Path({ stops, step, scale = 1.4, travel = 2 }: { stops: Stop[]; step: number; scale?: number; travel?: number }) {
  const k = stops.findLastIndex((s) => s.at <= step);
  let { x, y } = stops[Math.max(0, k)];
  if (k >= 0 && k < stops.length - 1) {
    const a = stops[k];
    const b = stops[k + 1];
    const move = Math.min(travel, b.at - a.at);
    const t = Easing.inOut(Easing.cubic)(Math.min(1, Math.max(0, (step - (b.at - move)) / move)));
    x = a.x + (b.x - a.x) * t;
    y = a.y + (b.y - a.y) * t;
  }
  const press = Math.max(0, ...stops.filter((s) => s.click).map((s) => 1 - Math.abs(step - s.at) / 0.4));
  const first = stops[0].at;
  const last = stops[stops.length - 1].at;
  const opacity = ramp(step, first - 1.5, first) * (1 - ramp(step, last + 1, last + 3));
  return <Pointer x={x} y={y} press={press} opacity={opacity} scale={scale} />;
}

/**
 * A scene fades in over the one before it (which stays opaque and only recedes),
 * so a transition never shows two half-transparent layouts.
 */
export function sceneStyle(step: number, from: number, to: number): CSSProperties {
  const enter = ramp(step, from - 2, from, Easing.out(Easing.cubic));
  const exit = ramp(step, to - 2, to, Easing.in(Easing.cubic));
  return {
    position: "absolute",
    inset: 0,
    background: theme.bg,
    opacity: enter,
    transform: `translateY(${(1 - enter) * 50}px) scale(${1 - exit * 0.03})`,
    filter: exit > 0 ? `blur(${exit * 6}px)` : undefined,
  };
}

export const panel: CSSProperties = {
  background: theme.panel,
  border: `1px solid ${theme.line}`,
  borderRadius: 18,
  boxShadow: "0 40px 90px -40px #000",
};

export const fmt = (n: number, digits = 0) => (Math.round(n * 10 ** digits) / 10 ** digits).toFixed(digits);
