// YouTube thumbnail (1280×720): a short promise on the left, the app's real
// sequencer playing a full beat on the right. Render with `npm run thumbnail`.
import { AudioLines, ChevronDown, Grid2X2, Settings2 } from "lucide-react";
import { AbsoluteFill } from "remotion";
import "../../frontend/style.css";
import { SequencerRow, TransportBar } from "../../frontend/Sequencer";
import { idleTransport, type Voice } from "../../frontend/types";
import { voices } from "./score";
import { theme } from "./theme";

const PLAYHEAD = 4;
const noop = async () => {};

const pattern: Voice[] = voices.map((voice) => ({
  kitID: "D0",
  samplePath: voice.file,
  steps: Array.from({ length: 16 }, (_, s) => voice.steps.includes(s)),
  volume: 0.8,
  muted: false,
  solo: false,
  layerMode: "manual",
  velocities: Array.from({ length: 16 }, (_, s) => voice.velocity(s)),
}));

export function Thumbnail() {
  return (
    <AbsoluteFill style={{ background: theme.bg, overflow: "hidden" }}>
      <AbsoluteFill
        style={{
          background:
            "radial-gradient(circle at 12% 18%, #f6806130, transparent 42%), radial-gradient(circle at 78% 70%, #afa0df22, transparent 45%), radial-gradient(circle at 95% 10%, #83c9ad1c, transparent 35%)",
        }}
      />
      {/* The sequencer, tilted and bleeding off the right edge. */}
      <div
        style={{
          position: "absolute",
          left: 690,
          top: 150,
          width: 980,
          transform: "perspective(1600px) rotateY(-16deg) rotateX(6deg) rotateZ(-2deg)",
          transformOrigin: "left center",
          filter: "drop-shadow(0 50px 70px #000c)",
        }}
      >
        <div style={{ zoom: 1.32, width: 980 / 1.32 }}>
          <section className="sequencer" style={{ boxShadow: "0 0 0 1px #ffffff14" }}>
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
                    <span className={`${i % 4 === 0 ? "beat" : ""} ${PLAYHEAD === i ? "current" : ""}`} key={i}>
                      {String(i + 1).padStart(2, "0")}
                    </span>
                  ))}
                </div>
              </div>
              {pattern.map((voice, channel) => (
                <SequencerRow key={channel} channel={channel} voice={voice} playhead={PLAYHEAD} onAction={noop} onVelocity={() => {}} />
              ))}
            </div>
            <TransportBar playing bpm={110} transport={{ ...idleTransport, running: true, step: PLAYHEAD, peak: 0.7, sampleRate: 44100 }} onAction={noop} />
          </section>
        </div>
      </div>
      {/* Fade the panel under the text. */}
      <AbsoluteFill style={{ background: `linear-gradient(90deg, ${theme.bg} 46%, ${theme.bg}aa 54%, ${theme.bg}00 64%)` }} />
      <div style={{ position: "absolute", left: 72, top: 118, width: 620 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 14, color: theme.accent }}>
          <AudioLines size={46} strokeWidth={2.3} />
          <div style={{ lineHeight: 1 }}>
            <div style={{ font: `700 34px ${theme.display}`, letterSpacing: "0.12em", color: theme.text }}>RAMPLE</div>
            <div style={{ font: `600 12px ${theme.mono}`, letterSpacing: "0.62em", color: theme.muted, marginTop: 5 }}>MANAGER</div>
          </div>
        </div>
        <div style={{ font: `700 106px/0.98 ${theme.display}`, letterSpacing: "-0.04em", color: theme.text, marginTop: 56 }}>
          Rample kits,
          <br />
          <span style={{ background: theme.gradient, WebkitBackgroundClip: "text", color: "transparent" }}>made easy.</span>
        </div>
        <div
          style={{
            display: "inline-flex",
            marginTop: 48,
            padding: "14px 24px",
            borderRadius: 999,
            background: theme.accent,
            color: "#121519",
            font: `700 26px ${theme.display}`,
            boxShadow: `0 16px 40px -14px ${theme.accent}`,
          }}
        >
          Free · macOS &amp; Windows
        </div>
      </div>
    </AbsoluteFill>
  );
}
