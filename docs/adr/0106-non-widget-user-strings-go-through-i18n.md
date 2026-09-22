# ADR-0106: 위젯 밖 사용자 문자열(알림 제목 · IPC 기본값 · 폴백 라벨)도 `t()` 를 거치고, 기계 식별은 제목이 아니라 식별 필드로 한다

- **Status**: Accepted
- **Date**: 2026-09-03
- **Tags**: i18n, notifications, hooks, ipc, remote-attach, git-viewer, plugin, wire-format
- **Group**: i18n

## Context

본체의 egui 위젯 호출 인자는 `t()` 적용이 끝났지만, *위젯 호출 형태가 아닌* 경로로 사용자에게 도달하는 영어 고정 문자열이 남아 있었다.

| 위치 | 문자열 | 도달 표면 |
|------|--------|-----------|
| BEL 수신 cascade 의 `DomainIntent::PushNotification { title }` | `"Bell"` | 알림 패널 · 사이드바 배지 · `notification.list` · `notification.created` plugin 이벤트 |
| `notification.create` 핸들러의 `title` 생략 기본값 | `"Notification"` | 알림 패널(raw IPC · IpcSequence 핸들러가 title 없이 호출하는 경로) |
| CLI `tasty notify` 의 `--title` clap `default_value` | `"Notification"` | 알림 패널 — CLI 가 항상 영어 제목을 채워 보내므로 host 의 기본값은 CLI 경로에서 한 번도 도달하지 않았다(host 기본값을 CLI 자체 기본값이 가리고 있었다) |
| 알림 패널의 출처 워크스페이스명 폴백 | `"Unknown"` | 출처 워크스페이스가 닫힌 알림의 행 |
| 원격 attach mirror 트리 파서의 탭 제목 폴백 | `"Shell"` | 스냅샷에 `name` 이 없거나 트리가 비었을 때의 mirror 탭/placeholder pane |

세 가지 판단이 필요했다.

1. **`"Bell"` 은 식별자인가.** 훅 스크립트나 plugin 이 `title == "Bell"` 로 분기하고 있다면 번역이 호환을 깬다. 확인 결과 훅은 `HookEvent::Bell` 로 매칭되고 payload 는 공통 key(`surface_id`)뿐이며 셸 env 는 `TASTY_HOOK_EVENT=bell` 이다 — 제목은 훅에 전달되지 않는다. plugin 은 `notification.created` 이벤트에서 `title` 과 함께 `source` 를 받으며, 번들 plugin 중 제목 문자열을 비교하는 곳은 없다.
2. **`notification.create` 의 기본 제목은 프로토콜 기본값인가, UI 문구인가.** "title 필수" 로 규약을 바꾸는 대안이 있었다. 그러나 IpcSequence 핸들러 예제가 이미 title 없이 호출하고, 기본 제목은 어디에도 기계적으로 소비되지 않고 패널에만 표시된다. 한편 CLI `tasty notify` 는 자체 `default_value = "Notification"` 으로 항상 제목을 주입하고 있어, host 쪽만 `t()` 로 바꾸면 CLI 경로는 계속 영어로 남는다 — 기본 문구를 결정하는 지점이 CLI 와 host 둘로 갈라져 있었다.
3. **`tasty-i18n` 을 의존하지 않는 leaf 크레이트의 폴백은 어떻게 하나.** `tasty-git-core` 의 커밋 요약 `"(no message)"` / 작성자 `"(unknown)"` 이 같은 성격이다. 이 크레이트는 host(`src/core/attach_runtime.rs`, 원격 조회 응답 직렬화)와 git-viewer plugin(별도 프로세스, 자기 `Translator`)이 공유하며, `LogEntry` 는 [ADR-0056](0056-git-viewer-remote-attach-git-query-channel.md) 의 wire 타입이다.

## Decision

**사람에게 도달하는 문자열은 형태(위젯 인자 · intent 필드 · IPC 기본값 · `unwrap_or` 폴백)와 무관하게 `t()` 를 거친다. 같은 값이 기계 식별에도 쓰이면 표시 문자열과 식별 필드를 분리하고, 식별은 언제나 식별 필드로만 한다.** 구체적으로:

1. BEL 알림 제목은 `notification.bell_title` 번역값이다. 벨의 기계 식별은 `HookEvent::Bell` / `event_kind: "bell"` / plugin 이벤트 `source` 가 담당하며 변경하지 않는다. 제목 비교로 분기하는 소비자는 만들지 않는다.
2. `notification.create` 의 `title` 은 **선택 파라미터로 유지**하고, 생략 시 호스트가 `notification.default_title` 번역값을 채운다. 프로토콜 레벨의 기본값은 "없음(absent)" 이고 그 자리에 보여줄 문구는 UI 의 몫이다. **CLI `tasty notify` 의 자체 기본값은 제거해 `--title` 을 안 주면 `title` 을 생략(`null`)으로 보낸다 — 기본 문구의 소유자는 host 하나다.** CLI 와 host 는 같은 바이너리라 버전 skew 는 없다.
3. 알림 패널의 사라진 워크스페이스명은 `notification_panel.unknown_workspace`, mirror 탭 제목 폴백은 `attach.tab_title_fallback` 이다. 후자는 원격 스냅샷 규약(`Pane::to_attach_json` 이 항상 `name` 을 실음)상 정상 경로에서는 도달하지 않지만, 방어 폴백도 사용자 표면이므로 예외로 두지 않는다.
4. `tasty-i18n` 을 의존하지 않는 leaf 크레이트는 **문구를 소유하지 않는다**(`tasty-ui-widgets` 의 호출자 주입 원칙을 `tasty-git-core` 같은 데이터 크레이트로 일반화). 데이터 크레이트는 "값 없음" 을 빈 문자열/`Option` 으로 돌려주고, 표시 문구는 소비자(plugin `Translator` / host `t()`)가 고른다. `tasty-git-core` 는 이에 따라 `LogEntry.summary` / `author` 가 없으면 **빈 문자열**을 돌려주고, git-viewer plugin 이 빈 값을 자기 lang 의 `git_viewer.no_message` / `git_viewer.unknown_author` 로 그린다 — 하드코딩 허용 예외 목록에 등록하지 않는다. *(구현 확정 보강 2026-09-03: 코드 이전 완료 — `LogEntry` doc · plugin `render.rs` `summary_text`/`author_text`)* *(구현 확정 보강 2026-09-04: 범위를 **전송 계층**까지 넓힌다 — `tasty-ipc` 도 문구를 소유하지 않는다. 포트 파일 조회 실패는 `PortFileError`(`HomeUnresolved` / `NotFound { path }` / `Invalid`)로 **조건만** 알리고, 사용자 문장은 CLI 가 `cli.port_file.*` 로 고른다. plugin 프로세스 논거는 여기 해당하지 않지만(이 크레이트를 링크하는 프로세스는 host 바이너리 하나뿐이다) 의존 방향 논거는 그대로다 — wire framing 크레이트가 번역 테이블을 알 이유가 없고, 그것을 의존하면 UI 가 없는 소비자(`tasty-remote`·`tasty-host-plugin`)까지 전이 의존을 진다. 그 크레이트의 `Display` 는 영어 기본 렌더링으로 남아 `lang/en.toml` 값과 문자 단위로 같고, 정합은 `tasty-cli` 테스트가 강제한다.)*

## Consequences

- **얻은 것**: `general.language = "ko"` 에서 벨·기본 알림·패널 폴백·mirror 탭 제목이 모두 한국어로 보인다. "제목은 표시 전용, 식별은 필드" 가 명문화되어 이후 알림 제목을 자유롭게 번역·수정할 수 있다. `notification.create` 의 호출 **방식**은 그대로다 — `tasty notify <body>` · IpcSequence · plugin 호출은 그대로 동작하고, 바뀐 것은 CLI 코드의 영어 기본값 제거(`"title": "Notification"` → `null`) 뿐이다.
- **잃은 것**: `notification.list` 응답과 `notification.created` 이벤트의 `title` 값이 언어 설정에 따라 달라진다 — 제목을 파싱하던 외부 스크립트가 있었다면 `source` / 훅 이벤트로 옮겨야 한다(번들 코드에는 해당 없음).
- **운영 비용 / 유지 부담**: 새 호스트 알림 제목·IPC 기본값·폴백 라벨을 추가할 때마다 세 lang 파일에 키를 추가해야 한다. 위젯 호출 기준 스캔 정규식으로는 이 형태가 잡히지 않으므로 리뷰 시 `title:` 필드 초기화와 `unwrap_or("…")` 도 함께 본다.
- *(구현 확정 보강 2026-09-04)* 실패 **조건**이 문자열이 아니라 타입이 된다 — `tasty-ipc` 소비자는 "인스턴스 없음"(`NotFound`)과 "포트 파일 손상"(`Invalid`)을 메시지 파싱 없이 구분한다. 대신 문구를 만드는 책임이 소비자로 옮겨가므로, 포트 파일을 새로 조회하는 코드는 `read_port_file_from` 을 직접 부르지 말고 문구를 소유하는 함수를 거쳐야 한다.
- *(구현 확정 보강 2026-09-03)* `tasty-git-core` 의 wire 의미가 좁아진다: `summary` / `author` 의 빈 문자열 = "git 에 없음". 원격 attach 조회에서 이 결정 이전 버전 host 가 보낸 `"(no message)"` / `"(unknown)"` 는 데이터로 취급돼 원격 언어 그대로 표시되고, 현행 host 는 빈 값을 보내므로 plugin 로케일로 그려진다.

## Alternatives Considered

- **`"Bell"` 을 식별자로 간주해 제목은 영어 고정, 표시만 별도 필드로 번역**: 훅·plugin 어디에도 제목을 식별자로 쓰는 소비자가 없어 분리할 대상이 없다. 표시 필드를 하나 더 만드는 것은 `NotificationStore` · IPC · plugin 이벤트 스키마를 모두 넓히는 비용만 든다.
- **`notification.create` 의 `title` 을 필수로 변경**: 기존 IpcSequence 핸들러(title 없는 호출)가 깨지고, CLI 는 자체 영어 기본값을 계속 들고 있어야 한다. 기본 제목은 기계적으로 소비되지 않으므로 규약을 좁힐 실익이 없다.
- **CLI 가 `tasty_i18n::t("notification.default_title")` 로 기본값을 채워 보내기**: 같은 번역 테이블을 공유하므로 동작은 하지만, 기본 문구를 결정하는 지점이 CLI 와 host 둘로 갈라진 채 남는다. 프로토콜 기본값을 "없음" 으로 두고 host 한 곳이 채우는 쪽이 단순하다.
- **`tasty-git-core` 폴백을 하드코딩 허용 예외로 등록**: 예외 목록의 기준은 "번역하면 의미가 변하는 고유명사·고정 식별자" 인데 "(no message)" 는 사람에게 보여주는 대체 문구라 기준에 맞지 않는다. 예외로 두면 원격 attach 시 원격 인스턴스 언어가 로컬 화면에 새는 문제도 그대로 남는다.
- **`tasty-git-core` 가 `tasty-i18n` 을 의존해 직접 번역**: leaf 데이터 크레이트가 host 의 전역 번역 테이블에 의존하게 되고, plugin 프로세스에서는 그 테이블이 비어 있어 키가 그대로 노출된다. 의존 방향도 [ADR-0089](0089-crate-split-follows-dependency-direction.md) 와 어긋난다.

## Reconsideration Triggers

- 알림 제목을 기계적으로 소비해야 하는 외부 계약(예: OS 알림 그룹핑 키, 원격 attach 알림 동기화)이 생겨 언어 독립 값이 필요해지면 — 그때는 제목이 아니라 별도 `kind` 필드를 추가한다.
- `notification.create` 에 title 없는 호출을 거부해야 할 보안·감사 요구가 생기면.
- git-viewer 가 plugin 이 아니라 host 내장 뷰로 옮겨져 `tasty-git-core` 의 소비자가 host 하나뿐이 되면(호출자 주입 대신 host `t()` 직접 호출로 단순화 가능).

## References

- [dev-guide/i18n](../dev-guide/i18n.md) — 적용 범위 표 · 공용 위젯/leaf 크레이트 호출자 주입
- [features/notifications](../features/notifications/index.md) — 제목은 표시 전용 · `notification.create` 기본 제목
- [features/hooks](../features/hooks/index.md) — Bell 훅 payload(공통 key 뿐)
- [features/remote-attach](../features/remote-attach/index.md) — mirror 탭 제목 폴백
- [ADR-0056](0056-git-viewer-remote-attach-git-query-channel.md) — `tasty-git-core` 공유 crate · `LogEntry` wire 타입
