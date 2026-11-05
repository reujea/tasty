import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { Icon } from "../../../components/core/Icon.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { PopupTitleBar } from "./info_modal.jsx";

// Restore-failure / save-halt NON-FOCUS notice (2026-10-19).
// Composed from the shared popup parts: PopupTitleBar (title + ×), DS Icon / IconButton / Button.
// No Scrim, no modal, no autofocus. Sits in the window's top-right NOTICE COLUMN:
//   work area top-right, inset notice-column-inset (8) · below any window-scope banner ·
//   below an open search bar of the top-right pane · above the PendingIndicator chip.
// Input: the notice never takes focus on appear and never captures Enter / Esc. Keys, selection and
// scroll outside it keep going to the terminal. Once the user clicks INTO it, its controls are normal
// buttons (Tab / Shift+Tab within the page order; Esc while focus is inside = Continue, focus returns
// to the surface it came from). × and Continue are the same action: dismiss and keep working in the
// initial state. Dismissing never quits the app and never touches the saved layout. The error stays
// in Notifications.



const RESTORE_COPY = {
  en: {
    restore: { title: "Couldn't restore your last session", body: "Tasty started with a new default workspace. The saved layout was not changed, moved or deleted. This message stays in Notifications." },
    halt: { title: "Layout changes aren't being saved", body: "Your terminals keep running and you can keep typing. Tabs, splits and other layout changes made from now on may not be restored next time. This message stays in Notifications." },
    cause: { denied: "Tasty doesn't have permission to read the saved layout.", corrupt: "The saved layout is damaged and can't be read.", write: "Tasty can't write to the saved layout." },
    label: "Saved layout", copy: "Copy path", cont: "Continue", close: "Close",
  },
  ko: {
    restore: { title: "마지막 세션을 복원하지 못했습니다", body: "새 기본 워크스페이스로 시작했습니다. 저장된 레이아웃은 바꾸거나 옮기거나 지우지 않았습니다. 이 내용은 알림에 남아 있습니다." },
    halt: { title: "레이아웃 변경이 저장되지 않고 있습니다", body: "터미널은 계속 실행되고 입력도 그대로 할 수 있습니다. 지금부터 만든 탭·분할 등 레이아웃 변경은 다음 실행 때 복원되지 않을 수 있습니다. 이 내용은 알림에 남아 있습니다." },
    cause: { denied: "저장된 레이아웃을 읽을 권한이 없습니다.", corrupt: "저장된 레이아웃이 손상되어 읽을 수 없습니다.", write: "저장된 레이아웃에 쓸 수 없습니다." },
    label: "저장된 레이아웃", copy: "경로 복사", cont: "계속", close: "닫기",
  },
  ja: {
    restore: { title: "前回のセッションを復元できませんでした", body: "新しい既定のワークスペースで起動しました。保存されたレイアウトは変更・移動・削除していません。この内容は通知に残ります。" },
    halt: { title: "レイアウトの変更が保存されていません", body: "ターミナルは動作を続け、入力もそのまま続けられます。これ以降のタブや分割などのレイアウト変更は、次回復元されない場合があります。この内容は通知に残ります。" },
    cause: { denied: "保存されたレイアウトを読み取る権限がありません。", corrupt: "保存されたレイアウトが破損しているため読み取れません。", write: "保存されたレイアウトに書き込めません。" },
    label: "保存されたレイアウト", copy: "パスをコピー", cont: "続行", close: "閉じる",
  },
};

// variant: "restore" (boot: store unreadable) | "halt" (running: saving stopped)
// cause:   "denied" | "corrupt" | "write"
function RestoreNotice({ variant = "restore", cause = "denied", path = "~/.local/share/tasty/session/layout.journal", lang = "en", onContinue, onCopy, width, style }) {
  const TitleBar = PopupTitleBar;
  const c = RESTORE_COPY[lang] || RESTORE_COPY.en;
  const v = c[variant];
  return (
    <div role="status" aria-live="polite" aria-label={v.title}
      style={{ width: width || "var(--tasty-restore-notice-width)", maxWidth: "100%", boxSizing: "border-box", display: "flex", flexDirection: "column",
        background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)",
        boxShadow: "var(--tasty-shadow-popover)", overflow: "hidden", ...style }}>
      {TitleBar && <TitleBar title={v.title} onClose={onContinue} closeLabel={c.close} />}
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-restore-notice-gap)", padding: "var(--tasty-restore-notice-padding)" }}>
        <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)" }}>
          <span style={{ flex: "none", display: "inline-flex", color: "var(--tasty-restore-notice-glyph)", marginTop: "var(--tasty-size-1)" }}><Icon name="alertCircle" size="var(--tasty-icon-size-md)" /></span>
          <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-body)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-primary)", textWrap: "pretty" }}>{c.cause[cause]}</span>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
          <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", textTransform: "uppercase", letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-restore-notice-label-fg)" }}>{c.label}</span>
          <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-xs) var(--tasty-space-xs) var(--tasty-space-xs) var(--tasty-space-sm)",
            background: "var(--tasty-restore-notice-path-bg)", border: "var(--tasty-border-width) solid var(--tasty-restore-notice-path-border)", borderRadius: "var(--tasty-radius)" }}>
            <span style={{ flex: 1, minWidth: 0, alignSelf: "center", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)",
              color: "var(--tasty-restore-notice-path-fg)", overflowWrap: "anywhere", userSelect: "text", cursor: "text" }}>{path}</span>
            <IconButton size="sm" aria-label={c.copy} title={c.copy} onClick={onCopy}><Icon name="copy" /></IconButton>
          </div>
        </div>
        <p style={{ margin: 0, fontSize: "var(--tasty-font-size-term-sm)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-restore-notice-body-fg)", textWrap: "pretty" }}>{v.body}</p>
        <div style={{ display: "flex", justifyContent: "flex-end" }}>
          <Button variant="secondary" size="sm" onClick={onContinue}>{c.cont}</Button>
        </div>
      </div>
    </div>
  );
}

// The window's top-right notice column. Children stack in priority order (restore / halt notice, then
// PendingIndicator). topOffset = window banner height + gap, and/or the top-right pane's open search bar.
function NoticeColumn({ children, topOffset = "0px", style }) {
  return (
    <div style={{ position: "absolute", top: "calc(var(--tasty-notice-column-inset) + " + topOffset + ")", right: "var(--tasty-notice-column-inset)", zIndex: 30,
      width: "var(--tasty-notice-column-width)", maxWidth: "calc(100% - 2 * var(--tasty-notice-column-inset))",
      display: "flex", flexDirection: "column", alignItems: "flex-end", gap: "var(--tasty-notice-column-gap)", pointerEvents: "none", ...style }}>
      {React.Children.map(children, (ch) => ch && <div style={{ pointerEvents: "auto", maxWidth: "100%", display: "flex" }}>{ch}</div>)}
    </div>
  );
}

export { RestoreNotice, NoticeColumn, RESTORE_COPY };
