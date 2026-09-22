# ADR-0426: `file_handler.reload` 는 적용되지 않은 user 항목을 `rejected` 필드로 알린다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, cli, file-handler, agent-facing, compatibility, settings
- **Group**: file-handler

## Context

`file_handler.reload` 는 user 설정(`<TASTY_HOME>/file-handlers.toml`)을 다시 읽어 user 출처 handler 를
통째로 갈아 끼운다. 응답은 `{"path", "exists"}` 뿐이었다. 그런데 registry 는 user 항목을 두 자리에서
버린다.

1. **설치 전**: id 에 `<owner>/` 접두사가 없으면 설치하지 않는다(`install_user`).
2. **finalize**: 모든 출처를 겹쳐도 detector 나 action 이 비면 그 id 를 등록하지 않는다
   (`ensure_finalized`). finalize 는 조회 때 게으르게 돌아 **reload 시점에는 아직 일어나지 않았다.**

두 버림 모두 `warn!` 한 줄로만 남았다. 실측(2026-09-21, base `290d89f56`, 격리 헤드리스 데몬):
세 항목(접두사 없는 `md-as-html` · action 없는 `user/no-action` · 온전한 `user/good`)을 적고
`tasty file-handler reload` → 응답 `{"exists": true, "path": …}`, CLI rc 0. 로그에는 접두사 경고 한
줄만 있었다 — 두 번째 버림은 reload 가 끝난 뒤 조회가 일어날 때까지 로그에도 없다.

사용자와 에이전트는 설정이 무시된 것을 dispatch 결과로만 알 수 있다. 에이전트가 자기 작업의 결과를
알 수 있어야 한다는 [identity](../identity.md) 원칙 2 에 어긋난다.

## Decision

**응답에 `rejected` 필드를 더한다.** 값은 이 reload 가 **적용하지 않은** user 항목의 배열이다 —
`[{"id": <TOML 에 적힌 그대로>, "reason": <코드>}]`, 그런 것이 없으면 `[]`. `path` · `exists` 는
그대로다. "적용하지 않은" 이지 "버린" 이 아니다 — 아래 셋째 사유의 항목은 registry 에 남아 있다가
대상이 나타나면 적용된다.

사유 코드는 셋이다.

| 코드 | 뜻 | 항목은 |
|------|----|--------|
| `missing_owner_prefix` | id 에 `<owner>/` 접두사가 없어 설치하지 않았다 | 버려졌다 |
| `missing_detector_or_action` | user 가 만든 항목(`user/…`)인데 detector 나 action 이 없어 등록되지 않는다 | 버려졌다 |
| `target_not_contributed` | 다른 출처(host · plugin)의 handler 를 patch 하는 항목인데 그 대상이 지금 registry 에 없다 — plugin 이 안 떠 있거나 id 가 틀렸다 | 남아 있다. 대상이 contribute 되면 그대로 적용된다 |

셋째를 둘째와 가르는 조건은 "id 의 owner 접두사가 `user/` 가 아니고 그 id 의 contribution 이 user
것뿐인가" 다. plugin 이 안 떠 있는 것과 id 오타는 여기서 가를 수 없다(둘 다 contribution 이 user 것
하나뿐이다). 그래서 코드 하나가 두 경우를 합친 뜻을 가진다. 헤드리스는 부팅 시 번들 plugin 이 떠
있지 않으므로, 정상적인 plugin patch 도 plugin 을 켜기 전까지 이 사유로 보고된다. "대상이 contribute 되면
그대로 적용된다" 는 병합이 user patch 를 늘 마지막에 두기 때문에 참이다 — reload 없이도 그렇다
([ADR-0427](0427-file-handler-merge-applies-user-patches-last.md)).

둘째 · 셋째는 finalize 를 기다리지 않고 reload 가 **같은 판정**으로 미리 고른다
("어느 출처든 detector 하나 · action 하나를 가졌는가" — finalize 는 마지막 non-None 이 이기므로
이것과 같다). 한 판정을 두 자리가 따로 들고 있으므로 단위 시험이 보고와 finalize 결과를 대조한다.

`FileHandlerRegistry::reload_user_config` 가 이 목록을 돌려주고, `Core::reload_file_handlers` 가
응답에 싣는다. CLI 는 응답 JSON 을 그대로 찍으므로 `tasty file-handler reload` 출력에 같이 보인다.
부팅 로드(`install_user_config`)는 돌려줄 호출자가 없어 로그로만 남긴다.

## Consequences

- **얻은 것**: 설정 일부가 적용되지 않은 것을 reload 응답 한 번으로 안다. 어느 항목이 왜 빠졌는지,
  그 항목이 버려졌는지 남아 있는지까지 온다.
- **얻은 것**: action 없는 항목의 버림이 reload 시점에 드러난다. 전에는 조회가 일어나기 전까지 로그에도
  없었다.
- **잃은 것 / 한계**: 파일을 읽거나 파싱하지 못해 reload 자체가 멈춘 경우는 항목을 모르므로 `rejected`
  가 빈 배열이다. 그 경우 이전 user 설정이 그대로 남는데 응답은 여전히 그 사실을 말하지 않는다. 이
  ADR 의 범위는 **항목 단위 거절**이고, 전체 중단의 신호는 따로 정한다(아래 트리거).
- **운영 비용 / 유지 부담**: "이 id 가 등록되는가" 를 두 자리가 따로 판정한다 — reload 보고의
  `is_complete` 와 finalize(`ensure_finalized`)의 병합 끝 검사다. 한쪽 규칙(예: 필수 필드)이 바뀌면
  다른 쪽도 같이 고쳐야 하고, 안 고치면 보고가 실제 등록과 어긋난다. 그 어긋남은
  `reload_user_config_reports_the_entries_it_dropped` 가 보고와 `all_handlers()` 를 대조해 잡는다.
- **호환성**: 필드 추가만이다. 기존 필드의 뜻과 CLI rc 는 그대로다. 알 수 없는 필드를 무시하는
  호출자는 영향이 없다.

## Alternatives Considered

- **A. 적용하지 않은 항목이 있으면 JSON-RPC 오류로 답한다.** 안 고른 이유: 나머지 항목은 이미 적용됐다. 오류로
  답하면 "아무것도 안 됐다" 로 읽히고, 기존 호출자의 성공 경로(rc 0)가 바뀐다.
- **B. `rejected` 를 id 문자열 배열로만 싣는다.** 안 고른 이유: 같은 증상(설정이 안 먹었다)에 원인이
  여럿이라, 사유 없이는 무엇을 고칠지 모른다.
- **C. 대상이 없는 patch 는 보고에서 뺀다**(사유 코드를 둘로 유지). 안 고른 이유: id 오타도 같이
  빠진다. 오타는 plugin 이 떠도 영영 적용되지 않는데, 에이전트가 그것을 알 길이 이 필드뿐이다.
  처음 판(사유 둘)은 그 항목을 `missing_detector_or_action` 으로 불러 **틀린 원인**을 댔다 — 정상
  patch 를 지우거나 고치게 만드는 거짓이었다. 필드 추가 직후라 사유 하나를 더해도 wire 소비자가 없다.
- **D. reload 에서 finalize 를 강제로 돌려 그 결과로 보고한다.** 안 고른 이유: finalize 는 host ·
  plugin 항목도 함께 판정해 user 가 만들지 않은 버림이 섞일 수 있다. 보고의 주어는 이 reload 가 받은
  user 항목이어야 한다.

## Reconsideration Triggers

**채널이 붙는 것**

- `crates/tasty-file-handler/src/registry_tests.rs` 의 `reload_user_config_reports_the_entries_it_dropped`
  가 실패한다 — 보고가 finalize 의 실제 판정과 어긋났다는 뜻이다. 변이 확인: 판정 함수를 늘 참으로
  바꾸면 이 시험이 죽는다(2026-09-21).
- 같은 파일의 `a_patch_whose_plugin_has_not_contributed_is_reported_as_target_not_contributed` ·
  `a_patch_leaves_the_report_once_its_plugin_contributes` 가 실패한다 — 대상 없는 patch 의 사유가
  바뀌었거나, 대상이 나타난 뒤에도 보고에 남는다. 변이 확인: "`user/` 가 아닌가" 조건을 늘 거짓으로
  두면 둘 다 죽고, 늘 참으로 두면 `user/…` 항목이 새 사유로 불려 위 시험이 죽는다(2026-09-21).
- `tests/e2e_tests.rs` 의 `file_handler_reload_reports_the_entries_it_dropped` 가 실패한다 — 응답
  모양이 바뀌었다. 변이 확인: 응답 조립에서 항목을 비우면 죽는다(2026-09-21).

**원리적으로 안 붙는 것**

- "contribution 이 user 것뿐인가" 조건은 지금 변이로 재지 못한다 — host · plugin 선언은 타입상
  detector · action 을 둘 다 가져 그 조건이 거짓인 채로 판정에 닿는 입력이 없다. 필수 필드가 없는
  host · plugin 선언이 가능해지면 이 조건이 살아나고 그때 시험을 더한다.
- user 항목을 적용하지 않는 **새 자리**가 registry 에 생긴다. 그 자리는 이 보고에 자동으로 안 들어간다.
  재는 법: registry 의 `warn!` 중 user 항목을 버리는 것을 세어 사유 코드 수와 견준다.
- 파싱 · 읽기 실패로 reload 가 멈춘 사실을 응답에 실어야 한다는 요구가 생긴다. 그때 이 응답에 필드를
  하나 더 정한다.

## References

- [file-handler 기능 문서](../features/file-handler/index.md) — 인터페이스 절의 reload 응답
- [ADR-0425](0425-headless-file-dispatch-answers-that-this-build-cannot-open-files.md) — 같은 namespace 의
  dispatch 가 거짓 성공을 고친 결정
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-file-handler/src/registry.rs` 의
  `reload_user_config` · `install_user_decls` · `RejectedUserHandler` · `is_complete` · `src/core/file.rs` 의
  `ReloadFileHandlersOutcome` · `src/adapters/ipc/handler/file_handler.rs` 의 `handle_reload`
