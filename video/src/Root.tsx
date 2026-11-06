import { Composition } from "remotion";
import { DURATION_IN_FRAMES, FPS } from "./score";
import { Thumbnail } from "./Thumbnail";
import { Video } from "./Video";

export function Root() {
  return (
    <>
      <Composition id="Demo" component={Video} durationInFrames={DURATION_IN_FRAMES} fps={FPS} width={1920} height={1080} defaultProps={{ vertical: false }} />
      {/* 9:16 for Shorts, Reels and TikTok, from the same scenes. */}
      <Composition id="DemoVertical" component={Video} durationInFrames={DURATION_IN_FRAMES} fps={FPS} width={1080} height={1920} defaultProps={{ vertical: true }} />
      <Composition id="Thumbnail" component={Thumbnail} durationInFrames={1} fps={FPS} width={1280} height={720} />
    </>
  );
}
