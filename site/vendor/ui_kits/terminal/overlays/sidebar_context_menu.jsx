import React from "react";
import { Icon as TastyDesignSystem_41fd3f_Icon } from "../../../components/core/Icon.jsx";
import { MenuItem } from "../../../components/navigation/MenuItem.jsx";

// Tasty UI kit — sidebar workspace-category context menu + collapsed-rail popup.
// Used by chrome.jsx Sidebar (right-click → SidebarContextMenu) and CollapsedSidebar
// (`---` category button → RailCategoryPopup). Mirrors the tools_menu anchored-popup
// visual language (surface-raised · border-strong · radius · shadow-popover · 28px rows).
// Only shown when the "Workspace categories (folders)" setting is on.
//
// New i18n strings surfaced by this design (add to lang/{en,ko,ja}.toml):
//   workspace_category.add_workspace  — "워크스페이스 추가"
//   workspace_category.collapse       — "접기"
//   workspace_category.expand         — "펼치기"
// Existing keys reused: new_category / rename_category / delete_category / move_to_category.

// Resolved at render (see chrome.jsx DSIcon): the module-level `cx` map must not capture an undefined Icon.
const Icon = (p) => React.createElement(TastyDesignSystem_41fd3f_Icon, p);

// glyphs by name from the canonical set (icons/*.svg via <Icon name>)
const cx = {
  plus: <Icon name="plus" />,
  edit: <Icon name="edit" />,
  trash: <Icon name="trash" />,
  folder: <Icon name="folder" />,
  move: <Icon name="move" />,
  collapse: <Icon name="chevronDown" />,
  chevR: <Icon name="chevronRight" size={14} />,
};

// every anchored menu popup: menu-* container tokens + popup-content-margin (4) on all sides (2026-10-07)
const panelStyle = {
  background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)",
  borderRadius: "var(--tasty-menu-radius)", padding: "var(--tasty-popup-content-margin)", boxShadow: "var(--tasty-shadow-popover)",
  minWidth: 176,
};

// Non-interactive category-name header for the rail popup (a label, NOT a menu item).
function PopupHeader({ children, meta }) {
  return (
    <div style={{ display: "flex", alignItems: "baseline", gap: "var(--tasty-space-sm)",
      padding: "var(--tasty-space-xs) var(--tasty-space-sm) var(--tasty-space-sm)",
      borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)", marginBottom: "var(--tasty-space-xs)" }}>
      <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-semibold)",
        color: "var(--tasty-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{children}</span>
      {meta != null && <span style={{ flex: "none", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{meta}</span>}
    </div>
  );
}

function useDismiss(onClose) {
  React.useEffect(() => {
    const h = (e) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);
}

// ── Right-click context menu — items resolve to the target under the cursor ──
// target: { kind: "background" | "category" | "workspace", cat?, ws? }
// A `normal`-reserved category shows only additive actions (no rename/delete).
function SidebarContextMenu({ x, y, target, onClose, categories = [] }) {
  useDismiss(onClose);
  const t = target || { kind: "background" };
  const reserved = t.kind === "category" && t.cat && t.cat.reserved;
  // clamp within viewport
  const W = 200, left = Math.min(x, window.innerWidth - W - 8), top = Math.min(y, window.innerHeight - 200);
  const act = () => onClose();
  const moveTargets = (categories || []).filter((c) => !t.ws || c.id !== (t.cat && t.cat.id));
  return (
    <div onClick={onClose} onContextMenu={(e) => { e.preventDefault(); onClose(); }}
      style={{ position: "fixed", inset: 0, zIndex: 60 }}>
      <div onClick={(e) => e.stopPropagation()} role="menu" aria-label="Workspace categories"
        style={{ position: "fixed", left, top, ...panelStyle }}>
        {/* 2026-10-07 — flat list, no submenu: the product draws this with the OS-native menu.
            Inert "Move to category" header, then every OTHER category as a plain item. */}
        {t.kind === "workspace" && (
          <>
            <MenuItem label="Move to category" icon={cx.move} disabled />
            {moveTargets.filter((c) => !(t.cat && c.id === t.cat.id)).map((c) => (
              <MenuItem key={c.id} label={c.reserved ? "Workspaces" : c.name} onClick={act} />
            ))}
          </>
        )}
        {t.kind === "category" && (
          <>
            {/* Additive action first — create a workspace INTO this category
                (mirrors RailCategoryPopup's first item). Shown for reserved
                `normal` too: additive-only means rename/delete are barred, not add. */}
            <MenuItem label="Add workspace" icon={cx.plus} onClick={act} />
            <MenuItem separator />
            {!reserved && (
              <>
                <MenuItem label="Rename category" icon={cx.edit} onClick={act} />
                <MenuItem label="Delete category" icon={cx.trash} danger onClick={act} />
                <MenuItem separator />
              </>
            )}
          </>
        )}
        {t.kind === "workspace" && <MenuItem separator />}
        <MenuItem label="New category" icon={cx.plus} onClick={act} />
      </div>
    </div>
  );
}

// ── Collapsed-rail category popup — anchored to the RIGHT of the `---` button ──
// (same anchored pattern as the rail Tools button). Top line = the category name,
// a NON-clickable header; below it the action items for that category.
function RailCategoryPopup({ anchor, label, reserved, collapsed, count = 0, onToggleCollapse, onClose }) {
  useDismiss(onClose);
  const left = anchor.right + 6;
  const top = Math.min(anchor.top - 6, window.innerHeight - 190);
  const act = () => onClose();
  return (
    <div onClick={onClose} onContextMenu={(e) => { e.preventDefault(); onClose(); }}
      style={{ position: "fixed", inset: 0, zIndex: 60 }}>
      <div onClick={(e) => e.stopPropagation()} role="menu" aria-label={label}
        style={{ position: "fixed", left, top, ...panelStyle }}>
        <PopupHeader>{label}</PopupHeader>
        <MenuItem label="Add workspace" icon={cx.plus} onClick={act} />
        <MenuItem label={collapsed ? "Expand" : "Collapse"}
          icon={<span style={{ display: "inline-flex", transform: collapsed ? "rotate(-90deg)" : "none" }}>{cx.collapse}</span>}
          onClick={onToggleCollapse} />
        {!reserved && (
          <>
            <MenuItem separator />
            <MenuItem label="Rename category" icon={cx.edit} onClick={act} />
            <MenuItem label="Delete category" icon={cx.trash} danger onClick={act} />
          </>
        )}
      </div>
    </div>
  );
}

export { SidebarContextMenu, RailCategoryPopup };
