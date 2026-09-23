# Claude/Codex 짝 핸들러의 호환 경계

두 plugin은 서로 다른 CLI를 실행한다. 공용 판정은 `tasty-plugin-agent-common`에서
공유하고, 이미 공개된 응답과 각 CLI의 기동·종료 계약은 유지한다. 함수 이름이나
본문이 비슷하다는 이유만으로 응답 형상·번역 키·CLI 플래그를 통일하지 않는다.

## 공개 응답과 번역 형식

| 표면 | Claude | Codex | 유지 근거와 검증 |
|---|---|---|---|
| `children` | bare 배열. `child_surface_id` 등으로 remap하고 전경 프로세스 이름/PID를 보충 | 호스트의 `{"children": […]}` 응답 그대로 | 기존 호출자의 JSON 해석을 보존한다. 각 plugin의 `children_response_is_*` 시험이 자기 변환을 고정한다 |
| `kill` 성공 | `{"killed": true}` | 호스트의 `killed_surface_id`·`child_index`를 포함한 응답 그대로 | 기존 성공 응답을 보존한다. 각 plugin의 `kill_response_is_*` 시험이 자기 변환을 고정한다 |
| spawn 경고 | 위치 placeholder와 `claude.spawn.warning_*` 키 | 이름 placeholder와 `codex.spawn_warning.*` 키 | 기존 번역 카탈로그와 치환 방식을 함께 유지한다. `build_spawn_warning_*` 시험은 임계값·idle/stale 후보를 확인한다 |

`children`의 전경 정보 보충과 `kill`의 error scanner 해제는 별개의 동작이다.
Claude는 scanner를 소유하므로 성공한 kill의 surface id로 즉시 해제한다. Codex에
없는 scanner를 형식 통일을 위해 추가하거나 Claude의 정리를 제거하지 않는다.

응답을 그대로 출력하는 CLI 외에 저장소 밖에서 JSON을 해석하는 호출자가 있을 수 있다.
따라서 저장소 내부 소비자가 없다는 사실은 공개 응답 변경의 근거가 아니다. 두 응답을
통일해야 하는 구체적인 요구가 생기면 호출자 호환과 위 시험을 함께 검토한다.

placeholder의 형태를 유지하는 것은 정보 손실을 허용하는 뜻이 아니다. 예를 들어
`surface`/`surface_id`의 오형식 오류는 어느 키가 틀렸는지 양쪽 모두 알려야 한다.
양쪽의 `a_malformed_target_surface_names_which_of_the_two_keys_was_wrong` 시험이
세 로케일에서 그 정보량을 확인한다. 판정은 공용 `params::target_surface`가 소유한다.

## 완료 알림의 대칭과 의도된 차이

두 plugin의 spawn/tell은 caller와 target에 대한 완료 hook을 등록한다. 각각
idle·needs-input·process-exit의 once hook을 사용하며, 발화 후 같은 target과
command를 가진 형제를 정리한다. target이 살아 있으면 다시 등록한다.

| 관심사 | Claude | Codex |
|---|---|---|
| idle 이벤트 | `claude-idle` | `codex-idle` |
| 입력 대기 이벤트 | `needs-input`: Notification/PreToolUse 경로 | `needs-input`: PermissionRequest 경로 |
| 종료 이벤트 | host의 `process-exit` | host의 `process-exit` |
| 알림 수신 경로 | completion-log append | completion-log append |
| 추가 관측 | 별도 상시 `claude-error-stalled` hook과 scanner | notify 때 샌드박스 실패 화면 힌트 조회 |

완료 알림은 caller의 PTY에 새 발화를 주입하지 않는다. 일반 `tell`이 대상 PTY에
메시지를 보내는 것과 구분한다. 경로·로그 소비 계약은
[child 완료 알림](external-interaction.md#child-완료-알림--completion-log)을 따른다.

등록 루프는 공용 `host_call::register_completion_hooks`이고 이벤트 목록은 각 plugin의
매니페스트가 근거다. Codex에도 needs-input이 있으므로 이벤트 부재를 비대칭의
이유로 쓰지 않는다. `siblings_to_unset`·`rearm_if_still_alive` 관련 시험은 그룹 격리와
재등록을 검사하며, 실제 셸 hook 실행·로그 도착은 아래 실행 검증의 별도 범위다.

## 기동·재시작 검증

spawn은 host의 자식 관계·surface 생성 뒤 각 CLI 명령을 전송한다. respawn은 기존
자식의 host 재기동 경로를 거친 뒤 CLI 명령을 다시 보낸다. reboot는 같은 세션을
resume하되, Claude는 전경 프로세스 이탈/복귀를, Codex는 종료/배너 마커 증가를
관찰한다. 서로 다른 timeout·CLI 옵션·프로필 처리와 hook 실패 전파 범위는
[Claude](../plugins/claude/index.md), [Codex](../plugins/codex/index.md),
[host 호출 오류 처리](error-handling.md)에 있는 기존 계약을 따른다.

실행 검증에서는 앱을 띄우기 전부터 가짜 CLI만 찾는 PATH, 별도 HOME·TASTY_HOME·
CLI 설정 디렉터리를 사용한다. 실제 host/plugin, PTY, 기동 명령, 세션 hook,
completion hook의 셸 명령과 로그를 연결해 spawn/tell/respawn/reboot/kill을 확인한다.
가짜 CLI는 실제 API 대신 입력·세션·종료 마커와 hook 신호를 제공한다. 이 검증은
실제 CLI의 API·승인 UI·버전별 화면 동작 검증을 대신하지 않는다.

공용 크레이트 변경도 plugin 버전 검사의 workspace 의존 폐포에 포함된다. 같은 버전의
파일 동기화와 발행 버전의 정합은 별개다. 현재 규칙은 [빌드](build.md)와
[plugin 제작](plugin-development.md)을 따르며, 과거의 “공용 변경은 게이트가 못 본다”는
설명을 재사용하지 않는다.
