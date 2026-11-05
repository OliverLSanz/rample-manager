import { memo, useEffect, useRef, useState, type CSSProperties } from "react";
import { Minus, Plus, Play, Square } from "lucide-react";
import { type Voice, type Action, type Transport } from "./types";
export const colors = ["#f68061", "#83c9ad", "#afa0df", "#dfc275"];
const option = /Mac/i.test(navigator.platform) ? "Option" : "Alt";
export const SequencerRow = memo(function SequencerRow({
  channel,
  voice,
  playhead,
  onAction,
  onVelocity,
}: {
  channel: number;
  voice: Voice;
  playhead: number;
  onAction: (a: Action) => Promise<void>;
  onVelocity: (v: { channel: number; step: number }) => void;
}) {
  return (
    <div
      className={`sequence-row ${voice.muted ? "muted" : ""}`}
      style={{ "--voice": colors[channel] } as CSSProperties}
    >
      <div className="voice-controls">
        <b>{String(channel + 1).padStart(2, "0")}</b>
        <div>
          <div className="mute-solo">
            <button
              aria-label={`Mute voice ${channel + 1}`}
              aria-pressed={voice.muted}
              onClick={() => void onAction({ type: "toggleMute", channel })}
            >
              M
            </button>
            <button
              aria-label={`Solo voice ${channel + 1}`}
              aria-pressed={voice.solo}
              onClick={() => void onAction({ type: "toggleSolo", channel })}
            >
              S
            </button>
          </div>
          <input
            type="range"
            aria-label={`Volume for voice ${channel + 1}`}
            min="0"
            max="1"
            step="0.01"
            value={voice.volume}
            onChange={(e) =>
              void onAction({
                type: "setVolume",
                channel,
                value: Number(e.target.value),
              })
            }
          />
        </div>
      </div>
      <div className="steps">
        {voice.steps.map((on, step) => (
          <button
            key={step}
            aria-label={`Voice ${channel + 1}, step ${step + 1}`}
            aria-pressed={on}
            className={`step ${on ? "on" : ""} ${playhead === step ? "current" : ""}`}
            title={`Voice ${channel + 1}, step ${step + 1} · velocity ${voice.velocities[step]}. ${option}-click to edit velocity.`}
            onClick={(e) => {
              if (e.altKey) onVelocity({ channel, step });
              else void onAction({ type: "toggleStep", channel, step });
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              onVelocity({ channel, step });
            }}
          >
            {on ? (
              voice.layerMode === "velocity" ? (
                <span>{voice.velocities[step]}</span>
              ) : (
                <i />
              )
            ) : null}
          </button>
        ))}
      </div>
    </div>
  );
});
export const TransportBar = memo(function TransportBar({
  playing,
  bpm,
  transport,
  onAction,
}: {
  playing: boolean;
  bpm: number;
  transport: Transport;
  onAction: (a: Action) => Promise<void>;
}) {
  const [text, setText] = useState(String(Math.round(bpm)));
  const focused = useRef(false);
  useEffect(() => {
    if (!focused.current) setText(String(Math.round(bpm)));
  }, [bpm]);
  function commit() {
    focused.current = false;
    const value = Number(text);
    if (Number.isFinite(value) && text.trim()) {
      const normalized = Math.min(300, Math.max(30, Math.round(value)));
      setText(String(normalized));
      void onAction({ type: "setBpm", value: normalized });
    } else setText(String(Math.round(bpm)));
  }
  return (
    <div className="transport">
      <button
        className="play-button"
        aria-label={playing ? "Stop sequencer" : "Play sequencer"}
        onClick={() => void onAction({ type: "togglePlayback" })}
      >
        {playing ? (
          <Square size={12} fill="currentColor" />
        ) : (
          <Play size={12} fill="currentColor" />
        )}
        {playing ? "Stop" : "Play"}
      </button>
      <div className="bpm-control">
        <input
          aria-label="Tempo in BPM"
          type="number"
          min="30"
          max="300"
          value={text}
          onFocus={() => {
            focused.current = true;
          }}
          onChange={(e) => setText(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
          }}
        />
        <div>
          <span>BPM</span>
          <div>
            <button
              aria-label="Decrease tempo"
              onClick={() => void onAction({ type: "adjustBpm", delta: -1 })}
            >
              <Minus size={10} />
            </button>
            <button
              aria-label="Increase tempo"
              onClick={() => void onAction({ type: "adjustBpm", delta: 1 })}
            >
              <Plus size={10} />
            </button>
          </div>
        </div>
      </div>
      <input
        type="range"
        aria-label="Tempo"
        className="tempo-slider"
        min="30"
        max="300"
        step="1"
        value={bpm}
        onChange={(e) =>
          void onAction({ type: "setBpm", value: Number(e.target.value) })
        }
      />
      <div className="transport-right">
        <div
          className="level-meter"
          aria-label="Output level"
          title={`Native audio · ${(transport.sampleRate / 1000).toFixed(1)} kHz`}
        >
          {Array.from({ length: 9 }, (_, i) => (
            <i
              key={i}
              style={{
                height: 7 + i * 2,
                background:
                  transport.peak > Math.pow(i / 12, 2) + 0.001
                    ? i === 8
                      ? "#f68061"
                      : "#83c9ad"
                    : undefined,
              }}
            />
          ))}
        </div>
        <output className={playing ? "playing" : ""}>
          {transport.step >= 0 && transport.running
            ? `${Math.floor(transport.step / 4) + 1}.${(transport.step % 4) + 1}`
            : "1.1"}
        </output>
        <kbd>SPACE</kbd>
      </div>
    </div>
  );
});
