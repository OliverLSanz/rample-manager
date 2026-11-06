// The last hit: name, platforms and where to find the link.
import { Easing, Img, staticFile } from "remotion";
import { SCENES } from "../score";
import { theme } from "../theme";
import { ramp, useStep } from "../timing";
import { Glow } from "./Title";

export function Outro() {
  const step = useStep();
  const { from } = SCENES.outro;
  const local = step - from;
  const flash = Math.max(0, 1 - local / 3);
  const icon = ramp(step, from, from + 3, Easing.out(Easing.back(1.7)));
  const name = ramp(step, from + 1, from + 4);
  const platforms = ramp(step, from + 5, from + 8);
  const link = ramp(step, from + 10, from + 13, Easing.out(Easing.back(1.4)));
  const arrow = Math.sin(Math.max(0, local - 13) * 0.9) * 6;

  return (
    <div style={{ position: "absolute", inset: 0, background: theme.bg, overflow: "hidden" }}>
      <Glow />
      <div
        style={{
          position: "absolute",
          inset: 0,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Img
          src={staticFile("icon.png")}
          style={{ width: 168, height: 168, transform: `scale(${icon})`, filter: "drop-shadow(0 30px 50px #000a)" }}
        />
        <div
          style={{
            font: `600 104px/1 ${theme.display}`,
            letterSpacing: "-0.03em",
            color: theme.text,
            marginTop: 44,
            opacity: name,
            transform: `translateY(${(1 - name) * 26}px)`,
          }}
        >
          Rample Manager
        </div>
        <div
          style={{
            font: `500 32px ${theme.body}`,
            color: theme.soft,
            marginTop: 26,
            opacity: platforms,
            transform: `translateY(${(1 - platforms) * 20}px)`,
          }}
        >
          Free for macOS &amp; Windows · Works offline, no account
        </div>
        <div
          style={{
            marginTop: 64,
            display: "flex",
            alignItems: "center",
            gap: 18,
            padding: "20px 34px",
            borderRadius: 999,
            background: theme.accent,
            color: "#121519",
            font: `700 30px ${theme.display}`,
            opacity: link,
            transform: `scale(${0.8 + 0.2 * link})`,
            boxShadow: `0 20px 60px -20px ${theme.accent}`,
          }}
        >
          Link in the description
          <svg width="30" height="30" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" style={{ transform: `translateY(${arrow}px)` }}>
            <path d="M12 4v15m0 0-6-6m6 6 6-6" />
          </svg>
        </div>
        <div style={{ marginTop: 26, font: `500 22px ${theme.mono}`, color: theme.muted, opacity: link }}>
          oliverlsanz.com/RampleManager
        </div>
      </div>
      <div style={{ position: "absolute", inset: 0, background: "#fff", opacity: flash * 0.22 }} />
    </div>
  );
}
