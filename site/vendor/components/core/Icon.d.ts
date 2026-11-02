import * as React from "react";

/** Every canonical Tasty glyph name. Mirrors icons/<name>.svg and icons.json. */
export type IconName =
  | "plus" | "close" | "refresh" | "edit" | "trash" | "copy" | "check" | "search" | "filter" | "swap" | "more" | "download" | "star" | "starFill"
  | "chevronRight" | "arrowRight" | "chevronDown" | "chevronUp" | "chevronLeft" | "chevronsLeft" | "chevronsRight" | "move" | "fit"
  | "terminal" | "agent" | "markdown" | "html" | "split" | "splitH" | "paneEmpty" | "folder" | "folderOpen" | "file" | "image" | "list" | "layoutGrid" | "layoutDetail" | "listView" | "layers" | "columns"
  | "clipboard" | "textLeft" | "scriptFile" | "remote" | "port" | "gitBranch" | "gitTree"
  | "eye" | "eyeOff" | "lock" | "globe" | "globeOff"
  | "alertTriangle" | "alertCircle" | "helpCircle" | "shieldCheck" | "bell"
  | "tools" | "settings" | "plug" | "rocket" | "command" | "theme" | "keyboard" | "mouse" | "sun" | "hash"
  | "cmdKey" | "optionKey" | "shiftKey"
  | "folderPlus" | "filePlus" | "info" | "link";

export interface IconProps extends React.SVGAttributes<SVGSVGElement> {
  /** Which canonical glyph to render (see IconName). */
  name: IconName;
  /** Explicit px size. Omit to size from the wrapping control's CSS (md default). */
  size?: number | string;
  /** Accessible label; when set the icon is exposed as role="img", else aria-hidden. */
  title?: string;
}

/** name -> inner SVG markup. Source of truth: icons/<name>.svg. */
export const ICON_PATHS: Record<IconName, string>;
/** Sorted list of available glyph names. */
export const ICON_NAMES: IconName[];
/** Glyph names rendered filled (fill=currentColor) rather than stroke-only. */
export const FILL_GLYPHS: Set<IconName>;

/**
 * Renders one canonical glyph by name, inline, recolored via currentColor.
 * The single icon primitive — never inline an <svg><path> or <img> an .svg.
 */
export function Icon(props: IconProps): JSX.Element;
