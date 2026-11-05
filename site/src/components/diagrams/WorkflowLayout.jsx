import React from "react";
import { TabStrip, TerminalPane } from "../../../vendor/ui_kits/terminal/work.jsx";

const noop = () => {};

/** A static composition of the design kit, not a running terminal. */
export function WorkflowLayout({ lang = "en", background = false, caption: captionOverride }) {
  const ko = lang === "ko";
  const labels = background
    ? (ko ? ["내가 읽는 로그", "탭으로 가져온 작업"] : ["The log I am reading", "Task brought into a tab"])
    : (ko ? ["구현하는 곳", "테스트하는 곳"] : ["Implementation", "Tests"]);
  const caption = ko
    ? "디자인 컴포넌트로 구성한 배치 예시입니다. 실제 터미널은 아래 순서로 만들어 보세요."
    : "A layout example built from design components. Follow the steps below in Tasty.";
  const lines = background
    ? (ko ? [["확인 중인 출력", "읽던 위치를 유지합니다."], ["기존 프로세스와 출력 기록", "새 탭에서 이어서 확인합니다."]]
      : [["Output under review", "Keep your place."], ["Existing process and output", "Continue in a new tab."]])
    : [["project / src", "git status"], ["project / tests", ko ? "프로젝트의 테스트 명령 실행" : "Run your project's test command"]];
  return <figure className="workflow-diagram">
    <div className="workflow-diagram__grid" aria-hidden="true" inert="">
      {labels.map((label, i) => <div className="workflow-diagram__pane" key={label}>
        <TabStrip tabs={[{ id: "terminal", title: label, kind: "terminal", owner: "user", activity: "idle" }]}
          active="terminal" onSelect={noop} onClose={noop} onNew={noop} onSearch={noop} />
        <TerminalPane session={{ id: String(i + 1), lines: lines[i] }} focused={false} />
      </div>)}
    </div>
    <figcaption>{captionOverride ?? `${labels.join(" / ")} — ${caption}`}</figcaption>
  </figure>;
}
