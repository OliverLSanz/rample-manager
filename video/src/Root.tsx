import { Composition } from "remotion";
import { DURATION_IN_FRAMES, FPS } from "./score";
import { Video } from "./Video";

export function Root() {
  return (
    <Composition
      id="Prototype"
      component={Video}
      durationInFrames={DURATION_IN_FRAMES}
      fps={FPS}
      width={1920}
      height={1080}
    />
  );
}
