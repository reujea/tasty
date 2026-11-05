import React from "react";
import { Select, MultiSelect, SearchableSelect } from "../../components/index.js";
import { Spec, Stage, Meta, Note } from "../parts/specimen.jsx";

// Tasty Gallery — Components › Form controls: SearchableSelect (2026-10-19,
// searchable selection). Rendered inside components.jsx's forms section.




  const FONTS = ["JetBrains Mono", "Fira Code", "Cascadia Code", "D2Coding", "Iosevka Term", "Source Code Pro", "SF Mono", "Menlo", "Hack", "IBM Plex Mono", "Noto Sans Mono CJK KR", "Ubuntu Mono"];
  const PIN = [{ value: "", label: "Default (monospace)" }];
  const LONG = [...FONTS, "Sarasa Term K Nerd Font Mono Semibold Extended"];
  const THEMES = [["Mocha", "mocha"], ["Latte", "latte"]];
  const cap = { fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" };
  const mono = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" };

  const Cell = ({ label, h, children }) => (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", minHeight: h, alignItems: "stretch" }}>
      <div style={cap}>{label}</div>{children}
    </div>
  );
  const Themed = ({ attr, children, style }) => (
    <div data-theme={attr || undefined} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-panel)", color: "var(--tasty-text-primary)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", ...style }}>{children}</div>
  );
  const W = { width: "var(--tasty-field-width-lg)" };

  function SettingsRow({ label, children }) {
    return (
      <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-settings-label-width) auto", columnGap: "var(--tasty-settings-label-gap)", alignItems: "center", justifyContent: "start", minHeight: "var(--tasty-settings-row-min-height)" }}>
        <span style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)" }}>{label}</span>
        {children}
      </div>
    );
  }

  function GallerySearchableSelect() {
    if (!SearchableSelect) return null; // until the bundle recompiles
    return (
      <>
        <Spec id="searchable-select" title="SearchableSelect — Select with a search field in its open list"
          when={<>A <b>Select variant</b> for one value out of a long list the user would rather type into — today the <b>font family</b> rows in Appearance › General and the per-font override. <b>Closed it is a Select</b>: same 28 height, 12 left, 28 chevron room, chevron, borders. Open, the popup is the <b>menu popover</b> with a <b>search field on top</b>, a separator, then the shared <code>MenuItem</code> option rows. Short fixed lists — <b>Tab / Workspace / Category switch modifier</b>, exit behaviour, language — stay a plain <b>Select</b> (no search).</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            {THEMES.map(([label, attr]) => (
              <Themed key={label} attr={attr}>
                <div style={cap}>{label} · same trigger, side by side (200 wide)</div>
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
                  {[["Select", <Select key="s" options={["Catppuccin Mocha", "Catppuccin Latte"]} style={W} />],
                    ["Searchable · no value", <SearchableSelect key="sp" options={FONTS} value={null} placeholder="Choose a font" style={W} />],
                    ["MultiSelect", <MultiSelect key="m" options={["Waiting", "Ready", "Running"]} defaultValue={["Waiting"]} style={W} />],
                    ["SearchableSelect", <SearchableSelect key="ss" options={FONTS} pinned={PIN} defaultValue="JetBrains Mono" searchPlaceholder="Search fonts" emptyLabel="No matching fonts" style={W} />]].map(([k, node]) => (
                    <div key={k} style={{ display: "grid", gridTemplateColumns: "136px auto", alignItems: "center", gap: "var(--tasty-space-md)" }}><span style={mono}>{k}</span>{node}</div>
                  ))}
                </div>
              </Themed>
            ))}
          </Stage>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            {THEMES.map(([label, attr]) => (
              <Themed key={label} attr={attr}>
                <div style={cap}>{label} · trigger states</div>
                <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 200px)", gap: "var(--tasty-space-md) var(--tasty-space-lg)" }}>
                  <Cell label="closed"><SearchableSelect options={FONTS} value="JetBrains Mono" block /></Cell>
                  <Cell label="hover — border-strong"><SearchableSelect options={FONTS} value="JetBrains Mono" hoverTrigger block /></Cell>
                  <Cell label="disabled — neutral box"><SearchableSelect options={FONTS} value="JetBrains Mono" disabled block /></Cell>
                  <Cell label="placeholder — no value"><SearchableSelect options={FONTS} value={null} placeholder="Choose a font" block /></Cell>
                  <Cell label="long value — end ellipsis"><SearchableSelect options={LONG} value="Sarasa Term K Nerd Font Mono Semibold Extended" block /></Cell>
                  <Cell label="live — click, type, ↑↓ Enter"><SearchableSelect options={FONTS} pinned={PIN} defaultValue="Fira Code" searchPlaceholder="Search fonts" emptyLabel="No matching fonts" block /></Cell>
                </div>
              </Themed>
            ))}
          </Stage>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            {THEMES.map(([label, attr]) => (
              <Themed key={label} attr={attr}>
                <div style={cap}>{label} · open list</div>
                <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 200px)", gap: "var(--tasty-space-lg)" }}>
                  <Cell label="open — focus in search, selected = check" h={340}><SearchableSelect block open focusSearch query="" activeIndex={1} options={FONTS} pinned={PIN} value="JetBrains Mono" searchPlaceholder="Search fonts" /></Cell>
                  <Cell label="typed “co” → filtered, active = 1st match" h={340}><SearchableSelect block open focusSearch query="co" activeIndex={1} options={FONTS} pinned={PIN} value="JetBrains Mono" searchPlaceholder="Search fonts" /></Cell>
                  <Cell label="no match — pinned still shown" h={340}><SearchableSelect block open focusSearch query="zzz" activeIndex={0} options={FONTS} pinned={PIN} value="JetBrains Mono" searchPlaceholder="Search fonts" emptyLabel="No matching fonts" /></Cell>
                  <Cell label="loading — list still empty" h={180}><SearchableSelect block open focusSearch query="" activeIndex={0} options={[]} pinned={PIN} value="" loading loadingLabel="Loading fonts…" searchPlaceholder="Search fonts" /></Cell>
                  <Cell label="loading — partial list, row at end" h={180}><SearchableSelect block open focusSearch query="" activeIndex={-1} options={FONTS.slice(0, 3)} value="Fira Code" loading loadingLabel="Loading fonts…" searchPlaceholder="Search fonts" /></Cell>
                  <Cell label="selected row filtered out — trigger keeps it" h={180}><SearchableSelect block open focusSearch query="mono" activeIndex={0} options={FONTS} value="Fira Code" searchPlaceholder="Search fonts" /></Cell>
                  <Cell label="hover (row 4) vs keyboard-active (row 2)" h={200}><SearchableSelect block open query="" activeIndex={1} hoverIndex={3} options={FONTS.slice(0, 5)} value="JetBrains Mono" searchPlaceholder="Search fonts" /></Cell>
                  <Cell label="no pinned option (generic use)" h={200}><SearchableSelect block open query="" activeIndex={0} options={FONTS.slice(0, 5)} value="JetBrains Mono" searchPlaceholder="Search" /></Cell>
                  <Cell label="i18n — de, long strings" h={200}><SearchableSelect block open focusSearch query="qq" activeIndex={0} options={FONTS} pinned={[{ value: "", label: "Standard (Festbreitenschrift)" }]} value="" searchPlaceholder="Schriftarten durchsuchen" emptyLabel="Keine passenden Schriftarten" /></Cell>
                </div>
              </Themed>
            ))}
          </Stage>
          <Meta
            specs={[
              ["trigger", "= Select · select-height 28 · pad 12 left / 28 chevron room · 1px select-border · radius 4 · chevron ⌄ 14 at select-chevron-offset 8, rotates 180° when open"],
              ["trigger states", "rest select-border · hover border-strong · open/focus select-border-focus + 1px ring · disabled state-disabled-fill/-border/-fg · placeholder text-placeholder · long value end-ellipsis, never grows"],
              ["popup", "4 below trigger · min-width = trigger · max-width 320 · max-height 300 (whole popup) · menu-bg · 1px menu-border · menu-radius · shadow-popover · padding 4"],
              ["search field", "content width (popup − 2 × 4) · 28 tall · pad-x 8 · search icon 14 + gap 8 · input-raised-bg / input-raised-border · focus = border-focus edge, NO ring · body 14 · placeholder text-placeholder"],
              ["separator", "1px menu-separator (border-strong) · 4 above · 4 below (menu-sep) · a second one under pinned rows"],
              ["option row", "MenuItem · 28 · pad-x 12 · rest text-secondary · hover menu-item-bg-hover · keyboard-active surface-active · selected = menu-item-selected-fg + trailing check 14 (no fill) · label ellipsis"],
              ["status rows", "no match / loading · 28 · pad-x 12 · body 14 · text-secondary-raised · no hover, cursor default, aria-disabled · loading = Spinner 14 + gap 8 + text, after any rows already present"],
              ["scroll", "only the option list scrolls (.tasty-scroll); field, separators, pinned rows stay put"],
            ]}
            tokens={[
              { tok: "--tasty-select-search-list-max-height", use: "→ size-300 (font-combo-list-max-height now aliases this)" },
              { tok: "--tasty-select-search-inset", use: "→ space-xs 4 (font-combo-search-inset now aliases this)" },
              { tok: "--tasty-select-search-menu-gap", use: "→ space-xs 4" },
              { tok: "--tasty-select-search-menu-max-width", use: "→ size-320" },
              { tok: "--tasty-select-search-field-height", use: "→ input-height → control-height 28" },
              { tok: "--tasty-select-search-field-padding-x", use: "→ space-sm 8" },
              { tok: "--tasty-select-search-field-gap", use: "→ input-gap → space-sm 8" },
              { tok: "--tasty-select-search-field-bg", use: "→ input-raised-bg → bg-panel", color: "var(--tasty-select-search-field-bg)" },
              { tok: "--tasty-select-search-field-border", use: "→ input-raised-border → border-strong", color: "var(--tasty-select-search-field-border)" },
              { tok: "--tasty-select-search-field-border-focus", use: "→ input-border-focus → border-focus", color: "var(--tasty-select-search-field-border-focus)" },
              { tok: "--tasty-select-search-placeholder", use: "→ input-placeholder → text-placeholder", color: "var(--tasty-select-search-placeholder)" },
              { tok: "--tasty-select-search-icon-fg", use: "→ input-icon-fg → text-muted", color: "var(--tasty-select-search-icon-fg)" },
              { tok: "--tasty-select-search-status-fg", use: "→ text-secondary-raised", color: "var(--tasty-select-search-status-fg)" },
              { tok: "--tasty-select-search-trigger-border-hover", use: "→ border-strong", color: "var(--tasty-select-search-trigger-border-hover)" },
              { tok: "--tasty-select-search-value-placeholder", use: "→ text-placeholder", color: "var(--tasty-select-search-value-placeholder)" },
            ]} />
          <Note><b>Decisions (§6).</b> <b>1 · Focus</b>: opening moves keyboard focus into the search field; the field shows it with a <span className="tok">--tasty-border-focus</span> edge only — the trigger already draws the open ring, two rings would compete. <b>2 · Keyboard cursor: yes</b>. <span className="ic">↑ ↓</span> move the active row over the visible, enabled rows (pinned first), <span className="ic">Enter</span> commits and closes, <span className="ic">Esc</span> closes and returns focus to the trigger, <span className="ic">Tab</span> closes. Focus never leaves the field, so typing keeps working. On open the active row is the selected one; after each keystroke it moves to the first match. A plain Select still has no cursor (native). <b>3 · Query is cleared</b> on every close; reopening shows the full list with the current value checked. <b>4 · Search stays inside the list</b>, not a combobox trigger: the trigger keeps the Select shape in every state, and typing never overwrites the visible current value. <b>5 · Pinned options are supported</b> (<code>pinned</code>): always shown above their own separator, ignore the query. "Default (monospace)" is the way back to the default, so it should not disappear because the user typed "fira". This changes the product's current behaviour (shown only when the query is a substring of "monospace"). <b>6 · Tokens move to shared names</b> <span className="tok">--tasty-select-search-*</span>; <span className="tok">--tasty-font-combo-list-max-height</span> and <span className="tok">-search-inset</span> stay as aliases (no rename, same values). <b>7 · Modifier combos are a plain Select</b> (Tab / Workspace / Category switch, drag flip): fixed short lists, no search.</Note>
          <Note>Matching: case-insensitive substring on the label (same as the current font combo). A filtered-out selected value keeps its label in the trigger. Contrast: status rows use <span className="tok">--tasty-text-secondary-raised</span> on surface-raised (Mocha text-secondary, Latte n1100 5.17:1). Option rows are the MenuItem rows, unchanged. i18n keys: <code>settings.appearance.font_search_placeholder</code> “Search fonts” / “글꼴 검색” / “フォントを検索” / “Schriftarten durchsuchen” · <code>…font_no_match</code> “No matching fonts” / “일치하는 글꼴 없음” / “一致するフォントがありません” / “Keine passenden Schriftarten” · <code>…font_loading</code> “Loading fonts…” / “글꼴을 불러오는 중…” / “フォントを読み込み中…” / “Schriftarten werden geladen…”. The popup grows up to 320 for long strings; past that rows ellipsize and the full label is in <code>title</code>.</Note>
        </Spec>

        <Spec id="searchable-select-appearance" title="Appearance › General — font rows on SearchableSelect"
          when={<>The font family row (and each font override row) uses <b>SearchableSelect</b> at <span className="tok">--tasty-field-width-lg</span> 200. Its trigger is exactly as tall as the other Select rows on the same screen (On close · Language) — 28 — and carries the same ⌄ chevron instead of the old ▼ glyph.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            {THEMES.map(([label, attr]) => [false, true].map((open) => (
              <Themed key={label + open} attr={attr} style={{ width: 440, minHeight: open ? 420 : undefined }}>
                <div style={cap}>{label} · Appearance › General · {open ? "open + typed “mono”" : "closed"}</div>
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-settings-row-gap)" }}>
                  <SettingsRow label="Font family:">
                    {open
                      ? <SearchableSelect open focusSearch query="mono" activeIndex={1} options={FONTS} pinned={PIN} value="JetBrains Mono" searchPlaceholder="Search fonts" emptyLabel="No matching fonts" style={W} />
                      : <SearchableSelect options={FONTS} pinned={PIN} value="JetBrains Mono" style={W} />}
                  </SettingsRow>
                  <SettingsRow label="Font size:"><span style={{ ...mono, fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-primary)" }}>14</span></SettingsRow>
                  <SettingsRow label="On close:"><Select options={["Ask", "Keep running", "Quit"]} style={W} /></SettingsRow>
                  <SettingsRow label="Language:"><Select options={["English", "한국어", "日本語", "Deutsch"]} style={W} /></SettingsRow>
                </div>
              </Themed>
            )))}
          </Stage>
          <Meta specs={[["row", "settings label · settings-label-gap · trigger field-width-lg 200 · row min settings-row-min-height"], ["height check", "font trigger 28 = Select 28 (select-height) · chevron ⌄ icon chevronDown 14"], ["override rows", "same control, same width, per-font override in the Appearance list"]]} />
        </Spec>
      </>
    );
  }

export { GallerySearchableSelect };
