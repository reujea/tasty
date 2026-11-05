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
  "rules": "See which colours text and icons use on each ground. Work through the nine groups to compare role tokens and contrast in the current theme.",
  "rules-map": "See which ground belongs at each location and which grounds it can contain. Compare Mocha and Latte side by side to see the same structure in both themes.",
  "components": "Explore reusable components, from buttons and fields to lists and scrollbars. Try the demos, compare the fixed states, then check the usage notes, dimensions, and tokens.",
  "overlays-dialogs": "Explore dialogs for confirming an action or editing a value. Check their messages, actions, and dismissal, then compare the scrim and frame over different grounds.",
  "overlays-windows": "Browse the larger windows used for settings, launchers, and data tables. Compare their navigation, dimensions, and tab views to choose a layout for your content.",
  "overlays-popups": "Explore menus, search bars, switch overlays, and modifier hints. Check their placement and dismissal, and compare opaque overlays above a web page.",
  "overlays-banners": "See how banners bring messages and actions into the work area. Compare their scopes and queues, and try mouse actions that leave keyboard focus with your work.",
  "overlays-tutorial": "Explore tutorial markers, callouts, and the topic list. See how these elements work together to guide someone through the parts of the app.",
  "layouts": "Explore how sidebars, tabs, split regions, status bars, and title bars form the app shell. Compare states and views, then check dimensions and tokens in the always-visible Specs.",
  "loading": "Explore startup screens through to readiness, first-run shell setup, shutdown, and boot errors. Compare the guidance and progress indicators at each stage.",
  "dag": "Inspect a task graph in a tab or workspace popup. Compare execution states, edges, lists, and detail views. Create and edit tasks through the CLI.",
  "plugins": "Explore the Explorer and the Markdown, HTML, and image viewers. These tools sit alongside terminals in the work area, so you can read files and check results without leaving your workspace.",
  "explorer-ops": "See how the Explorer creates, moves, and inspects files. Compare examples of search, remote writes, deletion, progress, name conflicts, properties, and operation results.",
};
