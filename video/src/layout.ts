// The video renders in two formats from the same scenes: 16:9 for YouTube and the
// website, 9:16 for Shorts, Reels and TikTok. Scenes read the format from here.
import { createContext, useContext } from "react";

export interface Layout {
  vertical: boolean;
  width: number;
  height: number;
  /** Left and right margin of captions and panels. */
  margin: number;
  /** Width between the margins. */
  content: number;
  /** Where panels start, below the caption. */
  top: number;
}

export const landscape: Layout = { vertical: false, width: 1920, height: 1080, margin: 120, content: 1680, top: 270 };
// Vertical keeps text and controls inside the area that Shorts/Reels overlays leave free.
export const portrait: Layout = { vertical: true, width: 1080, height: 1920, margin: 56, content: 968, top: 600 };

export const LayoutContext = createContext<Layout>(landscape);
export const useLayout = () => useContext(LayoutContext);
