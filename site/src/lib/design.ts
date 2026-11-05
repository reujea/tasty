import type { Lang } from "./guide";

/** Bilingual navigation and introductions for the shared English design examples. */
export interface DesignPage {
  /** Route under the section, with its trailing slash. "" is the section root. */
  slug: string;
  label: { en: string; ko: string };
  desc: { en: string; ko: string };
}

export interface DesignGroup {
  group: { en: string; ko: string };
  pages: DesignPage[];
}

export const DESIGN_NAV: DesignGroup[] = [
  {
    group: { en: "Guidelines", ko: "가이드라인" },
    pages: [
      {
        slug: "",
        label: { en: "Design foundations", ko: "디자인 살펴보기" },
        desc: { en: "brand · color · type · spacing", ko: "브랜드 · 색 · 글꼴 · 간격" },
      },
      {
        slug: "tokens/",
        label: { en: "Design tokens", ko: "디자인 토큰" },
        desc: { en: "primitive → semantic → component", ko: "원시 → 의미 → 컴포넌트" },
      },
    ],
  },
];

/** Route of a design page in one language. */
export const designPath = (slug: string, lang: Lang) =>
  `${lang === "ko" ? "ko/design/" : "design/"}${slug}`;

export const pick = <T>(v: { en: T; ko: T }, lang: Lang) => (lang === "ko" ? v.ko : v.en);

/** The gallery uses shared routes. `key` matches the page folder in site/vendor/gallery/. */
export interface GalleryPage {
  key: string;
  /** Route under `design/gallery/`; "" is the gallery root. */
  slug: string;
  label: string;
  /** A short note on the /design/ tiles. The gallery sidebar is text only and does not show it. */
  desc: string;
}

export const GALLERY_GROUPS: { group: string; pages: GalleryPage[] }[] = [
  {
    group: "Foundations",
    pages: [
      { key: "foundations", slug: "", label: "Foundations", desc: "tokens" },
      { key: "icons", slug: "icons/", label: "Icons", desc: "glyphs" },
      { key: "rules", slug: "rules/", label: "Rules", desc: "ground × ink" },
      { key: "rules-map", slug: "rules-map/", label: "Location map", desc: "what nests where" },
    ],
  },
  {
    group: "Components",
    pages: [{ key: "components", slug: "components/", label: "Components", desc: "primitives" }],
  },
  {
    group: "Overlays",
    pages: [
      { key: "overlays-dialogs", slug: "overlays-dialogs/", label: "Dialogs", desc: "modal confirms" },
      { key: "overlays-windows", slug: "overlays-windows/", label: "Windows", desc: "big surfaces" },
      { key: "overlays-popups", slug: "overlays-popups/", label: "Popups and menus", desc: "anchored" },
      { key: "overlays-banners", slug: "overlays-banners/", label: "Banners", desc: "floating" },
      { key: "overlays-tutorial", slug: "overlays-tutorial/", label: "Tutorial", desc: "marker · guide" },
    ],
  },
  {
    group: "Shell",
    pages: [
      { key: "layouts", slug: "layouts/", label: "Layouts", desc: "shells" },
      { key: "loading", slug: "loading/", label: "Startup", desc: "boot screen" },
    ],
  },
  {
    group: "Surfaces",
    pages: [
      { key: "dag", slug: "dag/", label: "Task DAG", desc: "graph surface" },
      { key: "plugins", slug: "plugins/", label: "Plugin surfaces", desc: "surfaces" },
      { key: "explorer-ops", slug: "explorer-ops/", label: "Explorer file ops", desc: "create · drag · progress · props · search" },
    ],
  },
];

export const GALLERY_PAGES: GalleryPage[] = GALLERY_GROUPS.flatMap((g) => g.pages);

/** Route of a gallery page. The gallery is published once, not per language. */
export const galleryPath = (slug: string) => `design/gallery/${slug}`;

/** Introductory copy for the site; specimen content comes from the design kit. */
export const GALLERY_INTROS: Record<string, string> = {
  "foundations": "See how Tasty's colours, spacing, and typography work in context. Each example shows where a token is used and what it controls.",
  "icons": "Find an icon by its name or purpose. Hover over a tile to learn its role, and use the shared Icon component to keep icons consistent across the app.",
  "components": "Try the buttons, fields, and other reusable components. Hover and focus them to explore their states, then check the usage notes, dimensions, and tokens for each example.",
  "overlays-dialogs": "Explore dialogs for confirming an action or editing a value. Each example shows the message, available actions, and how the dialog closes.",
  "overlays-windows": "Browse the larger windows used for settings, launchers, and data tables. Compare their navigation and dimensions to choose a layout for your content.",
  "overlays-popups": "Try menus and popups that open beside a control. Each example shows where it appears and whether it closes on an outside click, Escape, or key release.",
  "overlays-banners": "See how banners bring a message or action into the work area. They stay below the tab bar and accept mouse input while leaving keyboard focus with your work.",
  "overlays-tutorial": "Explore the tutorial markers, explanations, and topic list opened from Tools → Tutorial. Use them to guide someone through the parts of the app.",
  "layouts": "Explore how Tasty arranges navigation and content. Compare list-and-detail layouts with tabs and sections. Turn on Specs to inspect the grid and dimensions.",
  "dag": "Follow an agent task graph in a tab or workspace popup. Both views use the same canvas to show progress. These views are for inspection; create and edit tasks through the CLI.",
  "loading": "Preview the screen shown while Tasty starts. It combines the logo, a spinner, and the current startup phase until the app is ready.",
  "plugins": "Explore the Explorer and the Markdown, HTML, and image viewers. These tools sit alongside terminals in the work area, so you can read files and check results without leaving your workspace.",
  "explorer-ops": "See how the Explorer creates, moves, and inspects files. The examples cover new items, drag and drop, progress and name conflicts, the Properties popup, the preview panel, and search."
};
