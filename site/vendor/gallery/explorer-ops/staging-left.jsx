import React from "react";
import { Button } from "../../components/index.js";
import { XThemes } from "./create-drag-search.jsx";
import * as ExplorerOpsY_ns from "./index.jsx";
import { Section, Spec, Stage, Meta, Note } from "../parts/specimen.jsx";

// Tasty Gallery — Explorer file operations: temporary staging folder left behind (2026-10-12, request explorer-staging-left).
// Result card (result_card) when copy / cross-disk move / their undo could not remove its hidden `.tasty-copy-*` folder.




const Y = () => ExplorerOpsY_ns;

const STAGING_COPY = {
  en: "Temporary folder couldn't be removed ({error})",
  ko: "임시 폴더를 지우지 못했습니다 ({error})",
  ja: "一時フォルダーを削除できませんでした（{error}）",
};
const stagingLine = (path, err, lang = "en") => [path, STAGING_COPY[lang].replace("{error}", err)];

function StagingLeftSection() {
  const { YResult } = Y();
  const copyPaths = <Button variant="ghost" ground="raised" size="sm">Copy paths</Button>;
  const undo = <Button variant="ghost" ground="raised" size="sm">Undo</Button>;
  return (
    <Section id="stagingleft" title="Temporary folder left behind">
      <Spec id="result-card-one-extra-line-kind-raised-to-at-least-warning" title="Result card — one extra line, kind raised to at least Warning"
        when={<>Copy, cross-disk move and their undo stage through a hidden <code>.tasty-copy-*</code> folder. When it can't be removed at the end, the card keeps the <b>items' own title</b> (the items did finish as reported) but its kind is raised to <b>at least Warning</b>: a leftover folder on disk is something to act on, and a green Success card next to a failure line reads as a contradiction. Danger stays Danger. The folder gets the existing ResultLine shape — path, then a <b>fixed reason</b> with the OS error in parentheses — placed <b>after</b> any item lines. Actions are unchanged: Copy paths (includes the folder path), Undo when the job allows it, no Retry. The card stays until closed, like every Warning card.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XThemes gap="var(--tasty-space-md)" render={() => (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: "var(--tasty-toast-max-width)" }}>
              <YResult variant="warning" title="Copied 2 items to Documents"
                lines={[stagingLine("~/Documents/.tasty-copy-AbC123", "Permission denied")]}
                actions={<>{undo}{copyPaths}</>} />
              <YResult variant="warning" title="Copy cancelled · 12 of 40 done"
                lines={[stagingLine("~/Documents/.tasty-copy-Q7x2Lm", "Directory not empty")]}
                actions={copyPaths} />
              <YResult variant="warning" title="Moved 37 of 40 · 3 failed"
                lines={[["~/Downloads/build/app-release-unsigned.apk", "Permission denied"], ["~/Downloads/archive.zip", "Copied; original not removed (in use)"], stagingLine("/Volumes/usb/.tasty-copy-9fKe01", "Device or resource busy")]}
                more={1}
                actions={<><Button variant="secondary" size="sm">Retry 3</Button>{copyPaths}</>} />
              <YResult variant="warning" title="Undid copy"
                lines={[stagingLine("~/Downloads/.tasty-copy-u5Rt8W", "Permission denied")]}
                actions={copyPaths} />
            </div>
          )} />
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: "var(--tasty-toast-max-width)" }}>
            <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)" }}>ko · ja reason line</span>
            <YResult variant="warning" title="문서로 항목 2개를 복사했습니다" lines={[stagingLine("~/Documents/.tasty-copy-AbC123", "Permission denied", "ko")]} actions={<><Button variant="ghost" ground="raised" size="sm">실행 취소</Button><Button variant="ghost" ground="raised" size="sm">경로 복사</Button></>} />
            <YResult variant="warning" title="2 項目を書類にコピーしました" lines={[stagingLine("~/Documents/.tasty-copy-AbC123", "Permission denied", "ja")]} actions={<><Button variant="ghost" ground="raised" size="sm">元に戻す</Button><Button variant="ghost" ground="raised" size="sm">パスをコピー</Button></>} />
          </div>
        </Stage>
        <Meta
          specs={[["kind", "max(items' kind, Warning) · Success / Info → Warning · Danger stays"], ["title", "items' result, unchanged"], ["line", "ResultLine: folder path (left-ellipsis) + fixed reason (OS error)"], ["order", "after item failure lines · counts toward 'and N more'"], ["actions", "Copy paths (+ folder) · Undo if allowed · no Retry"], ["lifetime", "until closed"], ["i18n", "explorer.result.staging_left — en / ko / ja below"]]}
          tokens={[{ tok: "--tasty-toast-max-width", use: "card width" }, { tok: "--tasty-text-secondary", use: "path", color: "var(--tasty-text-secondary)" }, { tok: "--tasty-text-muted", use: "reason", color: "var(--tasty-text-muted)" }]} />
        <Note>New key <code>explorer.result.staging_left</code>: en “{STAGING_COPY.en}” · ko “{STAGING_COPY.ko}” · ja “{STAGING_COPY.ja}”. <code>{"{error}"}</code> is the OS error text as shown on other result lines.</Note>
      </Spec>
    </Section>
  );
}

export { StagingLeftSection };
