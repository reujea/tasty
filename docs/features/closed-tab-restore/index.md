# 닫힌 항목 복원 (Closed item restore)

- **Status**: Implemented
- **주체**: 로컬 사용자 전용 (`restore_closed`, 기본 `Ctrl+Shift+T`)
- **설계**: 없음. 사용자와 에이전트의 구분은 [포커스 정책](../../design/policies/focus.md)을 따른다.
- **코드**: 닫기 입력과 원 대상 해석 `src/app/journal/commands/close.rs`, 불변 capture `src/runtime/journal_product/worker/capture.rs`, undo 구조 `crates/tasty-core/src/undo.rs`, 복원 준비 `src/app/journal/commands/assembly.rs`, 완료 후 사용자 선택 `src/app/journal/commands/view_completion.rs`.
- **화면**: 없음 (복원은 입력 때의 pane 에 즉시 반영)

## 목적

사용자가 실수로 닫은 surface/tab/pane/workspace 를 즉시 되돌린다. 닫기 시점의 구조와 콘텐츠를 journal의 불변 자료로 보관했다가 최근 기록부터 단축키로 복원한다. 일반적인 같은 슬롯 재개는 이 기록을 유지한다. 반복 사용 목적의 영구 저장은 [layout-presets](../layout-presets/index.md).

## 내부 동작

- 트리거: `restore_closed` 단축키([keybindings](../keybindings/index.md), 4 프리셋 공통 `Ctrl+Shift+T`).
- 스택 top 의 항목 복원. Surface/Tab 은 입력 때 정한 pane 에(그 pane 이 사라졌으면 복원하지 않는다). Pane 은 닫기 시점 split geometry(방향/비율/형제 pane)를 최대한 재현해 현재 워크스페이스 트리에 재삽입하고, 그 형제 pane 이 그새 사라졌으면 입력 때 정한 pane 기준으로 대체 분할한다. workspace 가 0개일 때 Surface/Tab/Pane 복원은 default workspace 를 먼저 만든 뒤.
- 닫기 capture는 live 터미널의 scrollback을 `general.restore_surface_content`가 켜져 있을 때만 불변 payload에 담는다. 아직 활성화하지 않은 지연 탭은 기존 payload 참조를 그대로 옮긴다. 저장 실패를 별도 scrollback 파일이나 메모리 사본으로 성공 처리하지 않는다.
- 닫기 capture의 terminal 내용은 worker 큐 상한(`tasty_ipc::admission::QUEUED_BYTES_LIMIT`, 64 MiB)에서 다른 대기 요청과 이 닫기의 내용 없는 크기를 뺀 예산 안에서만 담는다. terminal마다 디스크를 읽지 않는 scrollback 직렬화 하한으로 먼저 판정하고, 하한이 작은 terminal부터 담아 내용을 지키는 terminal 수를 늘린다. 하한이 남은 예산보다 크면 그 history를 읽지 않는다. 읽은 뒤 실제 증가분이 남은 예산을 넘어도 그 terminal은 내용 없이 둔다.
- 예산을 넘는 terminal은 scrollback 없이(cwd와 `restore.command`만) 기록하고 닫기를 진행한다. 내용을 잘라 일부만 담지 않는다. 같은 대상의 다른 surface는 예산 안이면 내용과 함께 기록한다. 다른 대기 요청이 예산을 점유해 생긴 부족도 기다리지 않고 같은 방식으로 처리한다. 닫기는 사용자가 기다리는 즉시 조작이고, 기다리는 사이 구조가 바뀔 수 있기 때문이다.
- 내용을 뺀 닫기가 확정되면 그 엔진 창에 경고 알림 하나를 남기고(`recovery.close_history_omitted_*`) 같은 사실을 warn 로그로 남긴다. 닫기 응답에는 내용을 뺀 surface ID 목록 `history_omitted`가 붙는다. 원격 사용자의 닫기(`remote.user`)도 같은 capture 경로다. 이때 서버 창에는 알림을 남기지 않고, 원격 구조 결과(`StructuralResult.history_omitted`)에 생략한 terminal 수를 실어 요청한 attach client 창에 경고 toast를 한 번 띄운다([Attach 메커니즘](../../dev-guide/attach-behavior.md)). 원 요청의 View가 사라졌거나 바뀌었으면 다른 창에 경고를 옮기지 않는다. idempotent replay 회신과 실패한 닫기는 경고하지 않는다.
- 이 완화는 history 내용 때문에 생긴 예산 부족에만 적용한다. 내용 없는 닫기 요청 자체가 예산을 넘거나 구조 journal 확정이 실패하면 닫기는 실패로 끝나고 기존 실패 피드백을 따른다.
- 복원할 때 이 설정이 꺼져 있으면 기록에 scrollback이 있어도 터미널에 넣지 않는다. 기록 자체는 지우지 않으므로 설정을 다시 켠 뒤 복원하면 그 scrollback이 쓰인다([layout-persistence](../layout-persistence/index.md#surface-내용-복원-현재-터미널-scrollback)).
- 닫기 시점 surface 메타 `restore.command`(예: claude plugin 의 `claude -r <id>`)가 있었으면 셸 시작 직후 자동 실행 → TUI 세션 재개([layout-persistence](../layout-persistence/index.md) 의 동일 메커니즘).
- 내용 없이 기록한 terminal은 복원하면 빈 scrollback으로 같은 cwd에서 새 셸을 열고 `restore.command`가 있으면 실행한다. 함께 기록한 다른 terminal의 scrollback은 그대로 복원한다.
- 복원할 항목 없으면 no-op (토스트/알림 없음).

## 저장과 수명

복원 기록은 엔진별 구조 journal에 최대 10개 보관한다. 닫기 준비에서 구조 snapshot과 콘텐츠를 불변 payload로 저장하고 임시 admission 참조로 보호한다. 닫기 batch가 확정되면 undo 추가 사건과 payload 참조가 함께 기록된다.

복원은 최근 기록부터 선택하여 새 ID로 구조를 만들고, 복원 완료 확정 시 기록을 소비한다. 같은 슬롯을 정상 재개하면 기록을 유지하며, 구조를 초기화하는 시작은 기존 기록을 퇴출한다. 소비·퇴출된 자료도 과거 이벤트나 snapshot이 참조하면 남고 저장소의 retention·GC에 따라 회수한다. 시작 시 정리하는 임시 admission 참조와 확정 이벤트 참조는 다르다.

계약은 `crates/tasty-core/src/undo.rs`, `crates/tasty-core/src/command/retirement.rs`, `src/runtime/journal_product/worker/capture.rs`, `src/runtime/journal_product/worker/assembly.rs`에 구현되어 있다. 저장소 참조는 [이벤트 저장소](../../architecture/event-store.md)를 따른다.

## 사용자/에이전트 분리 (핵심)

복원 스택은 **사용자의 화면 상태**라 에이전트 표면에 없다([포커스 정책](../../design/policies/focus.md)):

- 복원을 트리거하는 CLI/IPC 없음 (`RestoreClosedItem` intent 는 사용자 단축키 전용).
- 복원 뒤 포커스 이동(복원한 워크스페이스로 활성 전환 · 복원한 pane 으로 포커스)은 **사용자 요청일 때만** 일어난다. 복원이 사용자 단축키 전용이라는 약속에 기대지 않고 cascade(`cascade_closed_item_restored`)가 origin 을 직접 본다 — 다른 요청이 복원에 닿아도 사용자의 활성 워크스페이스·포커스 pane 은 그대로다.
- **에이전트가 닫은 항목은 스택에 안 들어간다** — 창 경로와 IPC 가 같은 Core 닫기 함수를 부르고, 기록 여부(`save_snapshot`)는 origin 을 아는 진입점이 정한다. IPC `surface.close` 와 `tab.close`/`pane.close`(DomainIntent)는 false, 사용자 단축키/마우스 닫기는 true 다. 창 경로는 닫지 못한 대상(pane 의 마지막 탭 등)을 기록하지 않는다.
- **원격 attach 로 forward 된 close 도 같은 규칙이다** — 서버는 mirror 에서 온 close 를 요청 주체(`origin`)로 가르고, 원격 사용자의 손 조작만 스냅샷한다. 원격 에이전트의 CLI/IPC close 는 서버 스택에도 안 들어간다([remote-attach](../remote-attach/index.md), [Attach 메커니즘](../../dev-guide/attach-behavior.md)).

## 비-목표

- 에이전트용 복원 API — 에이전트는 자기가 만든 리소스를 ID 로 재생성하면 된다.
- 반복 사용 목적의 이름 있는 영구 프리셋 — [layout-presets](../layout-presets/index.md).
- PTY 프로세스 상태 복원 — 셸은 새로 시작, scrollback + `restore.command` 만 이어진다.

## 관련

- [layout-persistence](../layout-persistence/index.md) · [layout-presets](../layout-presets/index.md) · [work-area](../work-area/index.md)
