// Tokens shared with the website (site/assets/style.css) and the app.
import { loadFont as loadDisplay } from "@remotion/google-fonts/SpaceGrotesk";
import { loadFont as loadMono } from "@remotion/google-fonts/JetBrainsMono";

const display = loadDisplay("normal", { weights: ["500", "600", "700"], subsets: ["latin"] });
const mono = loadMono("normal", { weights: ["500", "600", "700"], subsets: ["latin"] });

export const theme = {
  bg: "#111316",
  panel: "#1b1f25",
  raised: "#252a31",
  text: "#eeeff1",
  soft: "#b9bfc8",
  muted: "#89929e",
  line: "#ffffff14",
  accent: "#f68061",
  voices: ["#f68061", "#83c9ad", "#afa0df", "#dfc275"],
  display: `${display.fontFamily}, -apple-system, sans-serif`,
  mono: `${mono.fontFamily}, SFMono-Regular, monospace`,
  body: `-apple-system, BlinkMacSystemFont, "Segoe UI", Inter, sans-serif`,
  gradient: "linear-gradient(100deg, #f68061, #dfc275 45%, #83c9ad 75%, #afa0df)",
};
