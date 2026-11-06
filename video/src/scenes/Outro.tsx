// The last hit: name, platforms and the website. No platform-specific call to action
// ("link in the description"): the same video plays on YouTube, the website and socials.
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
  const cta = ramp(step, from + 7, from + 9);
  const url = ramp(step, from + 8, from + 11, Easing.out(Easing.back(1.4)));
  // The button breathes with the beat once it has landed.
  const glow = local > 11 ? 0.5 + 0.5 * Math.cos(((local - 11) / 4) * 2 * Math.PI) : 0;

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
            marginTop: 72,
            font: `600 20px ${theme.mono}`,
            letterSpacing: "0.18em",
            color: theme.accent,
            opacity: cta,
          }}
        >
          TRY IT NOW
        </div>
        <div
          style={{
            marginTop: 20,
            display: "flex",
            alignItems: "center",
            gap: 20,
            padding: "24px 40px",
            borderRadius: 999,
            background: theme.accent,
            color: "#121519",
            // Mono so the "l" in oliverlsanz can't be read as a capital I.
            font: `600 42px ${theme.mono}`,
            letterSpacing: "-0.02em",
            opacity: url,
            transform: `scale(${0.85 + 0.15 * url})`,
            boxShadow: `0 24px ${70 + 30 * glow}px -20px ${theme.accent}`,
          }}
        >
          <svg width="38" height="38" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 4v11m0 0-4.5-4.5M12 15l4.5-4.5M5 20h14" />
          </svg>
          oliverlsanz.com/RampleManager
        </div>
      </div>
      <div style={{ position: "absolute", inset: 0, background: "#fff", opacity: flash * 0.22 }} />
    </div>
  );
}
