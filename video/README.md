# Promo video

A 50-second demo of Rample Manager made in code with [Remotion](https://www.remotion.dev),
in the style of the website. Its scenes reuse the app's own components from
`../frontend` (the sequencer) with demo data.

```sh
npm ci
npm run studio   # edit with a live preview and timeline
npm run render            # out/rample-manager-demo.mp4, 16:9 1920×1080 at 60 fps
npm run render:vertical   # out/rample-manager-demo-vertical.mp4, 9:16 1080×1920 for Shorts/Reels/TikTok
npm run thumbnail         # out/youtube-thumbnail.png, 1280×720
```

- `src/score.ts` is the single score: tempo, scene boundaries, the beat and when each
  step is clicked in. The soundtrack and the animation both read it, so they stay in sync.
- `npm run audio` (run by `studio` and `render`) renders `public/soundtrack.wav` and
  `src/generated/analysis.json` from the Breakdance kit in the git-ignored
  `Generated Samples/` folder. Neither output is committed.
- Both formats come from the same scenes; `src/layout.ts` holds the format each scene
  reads to place its caption and panels.
- The text in the video is in English, like the app and the website.
