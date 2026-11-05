/** Static files the site derives from site/vendor/ at build time. Nothing here is written into the source tree. */
const TIERS = import.meta.glob("../../vendor/tokens/{primitives,semantic,components,base}.css", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const SOURCES = import.meta.glob("../../vendor/{components,ui_kits/terminal}/**/*.jsx", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const CARDS = import.meta.glob("../../vendor/guidelines/*.html", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** Tier order matters: each tier reads the one above. */
export const tokensCss = () =>
  ["primitives", "semantic", "components", "base"]
    .map((tier) => TIERS[`../../vendor/tokens/${tier}.css`])
    .join("\n");

const CSS_LITERAL = /^const (?:[A-Z][A-Z_]*_CSS|CSS)\s*=\s*`([^`]*)`;/gm;

/** The component rules, for pages rendered without client JavaScript. Hydrated pages also inject them at runtime. */
export const kitCss = () =>
  Object.keys(SOURCES)
    .sort()
    .flatMap((file) => [...SOURCES[file].matchAll(CSS_LITERAL)].map((m) => m[1].trim()))
    .join("\n\n");

/* Follows the page theme, unless the card pins one: the Latte swatches stay light in a dark page.
   The `storage` event fires in a frame when another document on the origin writes the key. */
const THEME_SYNC = `<script>
(function () {
  var r = document.documentElement;
  if (r.hasAttribute("data-theme")) return;
  function apply(v) {
    if (v === "latte" || v === "light") r.setAttribute("data-theme", "latte");
    else if (v) r.removeAttribute("data-theme");
    else if (matchMedia("(prefers-color-scheme: light)").matches) r.setAttribute("data-theme", "latte");
  }
  try { apply(localStorage.getItem("tasty-theme")); } catch (e) {}
  addEventListener("storage", function (e) { if (e.key === "tasty-theme") apply(e.newValue); });
})();
</script>`;

/** Guideline documents by file name, served as they are except for two paths and the theme script. */
export const cards = () =>
  Object.fromEntries(
    Object.entries(CARDS).map(([file, source]) => [
      file.split("/").pop()!.replace(/\.html$/, ""),
      source
        .replace(/href="(?:\.\.\/)+styles\.css"/g, 'href="../tokens.css"')
        // A card sits at design/cards/<name>.html and the icon at the site root.
        .replace(/(?:\.\.\/)+assets\/icons\/icon_256\.png/g, "../../tasty-icon.png")
        .replace(/<\/head>/, `${THEME_SYNC}\n</head>`),
    ]),
  );
