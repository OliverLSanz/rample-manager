import { AbsoluteFill, Audio, staticFile, useCurrentFrame } from "remotion";
import { SCENES, frameToStep } from "./score";
import { Build } from "./scenes/Build";
import { Chaos } from "./scenes/Chaos";
import { Editor } from "./scenes/Editor";
import { Library } from "./scenes/Library";
import { Outro } from "./scenes/Outro";
import { SequencerScene } from "./scenes/SequencerScene";
import { Title } from "./scenes/Title";

/**
 * Shows a scene between two steps; `lead` starts it early to overlap a transition.
 * Scenes keep reading the absolute frame, so their timing matches the score.
 */
function Scene({ from, to, lead = 0, children }: { from: number; to: number; lead?: number; children: React.ReactNode }) {
  const step = frameToStep(useCurrentFrame());
  return step >= from - lead && step < to ? <>{children}</> : null;
}

export function Video() {
  return (
    <AbsoluteFill style={{ background: "#111316" }}>
      <Audio src={staticFile("soundtrack.wav")} />
      <Scene {...SCENES.chaos}>
        <Chaos />
      </Scene>
      <Scene {...SCENES.title}>
        <Title />
      </Scene>
      <Scene {...SCENES.library} lead={2}>
        <Library />
      </Scene>
      <Scene {...SCENES.sequencer} lead={2}>
        <SequencerScene />
      </Scene>
      <Scene {...SCENES.editor} lead={2}>
        <Editor />
      </Scene>
      <Scene {...SCENES.build} lead={2}>
        <Build />
      </Scene>
      <Scene {...SCENES.outro}>
        <Outro />
      </Scene>
    </AbsoluteFill>
  );
}
