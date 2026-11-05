import { memo, useState, type CSSProperties, type DragEvent } from "react";
import {
  ChevronDown,
  ChevronUp,
  Copy,
  Layers,
  Play,
  Plus,
  SlidersHorizontal,
  TriangleAlert,
  X,
} from "lucide-react";
import { colors } from "./Sequencer";
import { draggedSamples, sampleDrag } from "./SamplePanel";
import { heardLayer, kitAddress } from "./patchView";
import {
  modes,
  type LayerMode,
  type LibrarySample,
  type PatchAction,
  type PatchKit,
  type PatchVoice,
} from "./types";

export const Waveform = memo(function Waveform({
  peaks,
  color,
}: {
  peaks?: number[];
  color: string;
}) {
  const step = 300 / Math.max(1, peaks?.length ?? 100);
  const path =
    peaks
      ?.map((p, i) => {
        const y = Math.max(1, p * 55);
        return `M${i * step + step / 2},${(60 - y) / 2}v${y}`;
      })
      .join(" ") ?? "";
  return (
    <svg
      className="waveform"
      viewBox="0 0 300 60"
      preserveAspectRatio="none"
      role="img"
      aria-label="Waveform"
    >
      <path d="M0 30H300" stroke={color} strokeOpacity=".13" />
      <path
        d={path}
        stroke={color}
        strokeWidth="1.7"
        strokeLinecap="round"
        opacity=".75"
      />
    </svg>
  );
});

/** One Voice of the selected Kit: what it plays, how, and its ordered Layers. */
export const VoiceCard = memo(function VoiceCard({
  channel,
  kit,
  source,
  voice,
  samples,
  waveforms,
  activeIndex,
  onPatch,
  onMode,
  onAudition,
  onAdd,
  onEdit,
  onDropSample,
}: {
  channel: number;
  /** The selected Kit. */
  kit: PatchKit;
  /** The Kit this Voice plays from, which can differ after a folder migration. */
  source: PatchKit;
  voice: PatchVoice;
  samples: Map<string, LibrarySample>;
  waveforms?: Record<string, number[]>;
  activeIndex: number;
  onPatch: (action: PatchAction) => void;
  onMode: (channel: number, mode: LayerMode) => void;
  onAudition: (channel: number) => void;
  onAdd: (channel: number) => void;
  onEdit: (channel: number) => void;
  /** A Library Sample dropped on this card becomes its last Layer. */
  /** Samples dropped on this Voice, in order. */
  onDropSample: (channel: number, samples: string[]) => void;
}) {
  // A Layer drag type per Voice: Layers reorder within their own Voice only.
  const layerDrag = `application/x-rample-layer-${channel}`;
  const [dropping, setDropping] = useState<"sample" | number | null>(null);
  const accepts = (e: DragEvent, type: string) =>
    [...e.dataTransfer.types].includes(type);
  const color = colors[channel];
  const mode = modes.find((m) => m.id === voice.mode) ?? modes[0];
  const heard = heardLayer(voice, activeIndex);
  const sample = heard && samples.get(heard.sampleId);
  const count = voice.layers.length;
  const number = channel + 1;
  return (
    <section
      className={`sample-card ${dropping === "sample" ? "dropping" : ""}`}
      aria-label={`Voice ${number}`}
      style={{ "--voice": color } as CSSProperties}
      onDragOver={(e) => {
        if (accepts(e, sampleDrag)) {
          e.preventDefault();
          e.dataTransfer.dropEffect = "copy";
          setDropping("sample");
        } else if (accepts(e, layerDrag)) {
          e.preventDefault();
          e.dataTransfer.dropEffect = "move";
        }
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null))
          setDropping(null);
      }}
      onDrop={(e) => {
        setDropping(null);
        const sample = e.dataTransfer.getData(sampleDrag);
        const layer = e.dataTransfer.getData(layerDrag);
        if (sample) {
          e.preventDefault();
          onDropSample(channel, draggedSamples(sample));
        } else if (layer && count > 0) {
          e.preventDefault();
          onPatch({ type: "moveLayer", channel, layer, to: count - 1 });
        }
      }}
    >
      <div className="voice-heading">
        <h2>
          <i />
          VOICE {String(number).padStart(2, "0")}
        </h2>
        <span className="voice-count">
          {source.id !== kit.id
            ? `FROM ${kitAddress(source)}`
            : `${count} ${count === 1 ? "LAYER" : "LAYERS"}`}
        </span>
      </div>
      <div className="mode-row">
        <label title={mode.description}>
          <Layers size={11} />
          <select
            aria-label={`Layer mode for voice ${number}`}
            value={voice.mode}
            onChange={(e) => onMode(channel, e.target.value as LayerMode)}
          >
            {modes.map((m) => (
              <option key={m.id} value={m.id}>
                {m.title}
              </option>
            ))}
          </select>
          <ChevronDown size={9} />
        </label>
        {voice.mode !== "manual" && count > 0 && (
          <span>
            {activeIndex >= 0 && activeIndex < count ? activeIndex + 1 : "–"} /{" "}
            {count}
          </span>
        )}
      </div>
      <Waveform peaks={heard && waveforms?.[heard.id]} color={color} />
      <div
        className="sample-detail"
        title={
          sample
            ? `${sample.name} · ${(sample.rate / 1000).toFixed(1)} kHz · ${sample.bits} bit`
            : ""
        }
      >
        {sample ? (
          <>
            <span>{sample.name}</span>
            <small>{sample.duration.toFixed(2)} s</small>
          </>
        ) : heard ? (
          <span className="warning">Unavailable Sample</span>
        ) : (
          <span>
            {channel === 0 ? "Voice 1 needs a Sample" : "No Layers yet"}
          </span>
        )}
      </div>
      <ol className="layer-list" aria-label={`Layers of voice ${number}`}>
        {voice.layers.map((layer, index) => {
          const name = samples.get(layer.sampleId)?.name;
          const selected = voice.selectedLayer === layer.id;
          const position = index + 1;
          return (
            <li
              key={layer.id}
              className={`${selected ? "selected" : ""} ${heard?.id === layer.id ? "heard" : ""} ${dropping === index ? "drop-before" : ""}`}
              draggable
              onDragStart={(e) => {
                e.dataTransfer.setData(layerDrag, layer.id);
                e.dataTransfer.effectAllowed = "move";
              }}
              onDragOver={(e) => {
                if (!accepts(e, layerDrag)) return;
                e.preventDefault();
                e.stopPropagation();
                e.dataTransfer.dropEffect = "move";
                setDropping(index);
              }}
              onDrop={(e) => {
                const dragged = e.dataTransfer.getData(layerDrag);
                if (!dragged) return;
                e.preventDefault();
                e.stopPropagation();
                setDropping(null);
                if (dragged !== layer.id)
                  onPatch({
                    type: "moveLayer",
                    channel,
                    layer: dragged,
                    to: index,
                  });
              }}
              onDragEnd={() => setDropping(null)}
            >
              <button
                className="layer-name"
                aria-label={`Select Layer ${position}: ${name ?? "Unavailable Sample"}`}
                aria-pressed={selected}
                title={name ?? "This Sample is no longer in the Library"}
                onClick={() =>
                  onPatch({ type: "selectLayer", channel, layer: layer.id })
                }
              >
                <b>{String(position).padStart(2, "0")}</b>
                <span>{name ?? "Unavailable Sample"}</span>
                {!name && <TriangleAlert size={11} className="warning" />}
              </button>
              <span className="layer-actions">
                <button
                  aria-label={`Move Layer ${position} up`}
                  title="Move up"
                  disabled={index === 0}
                  onClick={() =>
                    onPatch({
                      type: "moveLayer",
                      channel,
                      layer: layer.id,
                      to: index - 1,
                    })
                  }
                >
                  <ChevronUp size={11} />
                </button>
                <button
                  aria-label={`Move Layer ${position} down`}
                  title="Move down"
                  disabled={index === count - 1}
                  onClick={() =>
                    onPatch({
                      type: "moveLayer",
                      channel,
                      layer: layer.id,
                      to: index + 1,
                    })
                  }
                >
                  <ChevronDown size={11} />
                </button>
                <button
                  aria-label={`Repeat Layer ${position}`}
                  title="Repeat this Sample as another Layer"
                  onClick={() =>
                    onPatch({
                      type: "addLayer",
                      channel,
                      sample: layer.sampleId,
                    })
                  }
                >
                  <Copy size={10} />
                </button>
                <button
                  aria-label={`Remove Layer ${position}`}
                  title="Remove this Layer"
                  onClick={() =>
                    onPatch({ type: "removeLayer", channel, layer: layer.id })
                  }
                >
                  <X size={11} />
                </button>
              </span>
            </li>
          );
        })}
      </ol>
      <button
        className="add-layer"
        aria-label={`Add Sample to Voice ${number}`}
        title={
          source.id !== kit.id
            ? `Add a Library Sample to Voice ${number} of Kit ${kitAddress(source)}`
            : "Add a Library Sample as a new Layer"
        }
        onClick={() => onAdd(channel)}
      >
        <Plus size={11} /> Add Sample
      </button>
      <div className="sample-actions">
        <button
          disabled={count === 0}
          onClick={() => onAudition(channel)}
          aria-label={`Audition voice ${number}`}
        >
          <Play size={10} fill="currentColor" />
          Audition<kbd>{number}</kbd>
        </button>
        <button
          className="icon-button"
          disabled={count === 0}
          title="Edit the selected Sample with live sequence preview"
          aria-label={`Edit sample for voice ${number}`}
          onClick={() => onEdit(channel)}
        >
          <SlidersHorizontal size={14} />
        </button>
      </div>
    </section>
  );
});
