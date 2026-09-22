# ADR-0560: 붙여넣기는 사용자 입력이고, 두 붙여넣기 경로가 만나는 자리에서 기록한다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: input, typing-guard, paste, clipboard, command-palette, identity-principle-1, ipc, adr-0521

## Context

`surface.is_typing` 은 `typing`(최근 5 초 안의 사용자 입력)과 `idle_seconds`(마지막 사용자 입력 뒤 경과, 없으면 `-1`)를 준다. 둘은 `CoreState::record_typing` 이 surface 마다 남기는 시각에서 나온다. 에이전트의 `send`/`tell` 은 그 시각을 안 건드린다 — 이 값은 "사람이 손대고 있나" 를 에이전트가 읽는 채널이고, 원칙 1([identity.md](../identity.md) §2.1)의 좌변이다.

그 값을 읽는 쪽은 셋이다(`git grep -n 'surface.is_typing\|surface.send_wait_idle\|is_typing(' -- src crates` 전수, 2026-09-23):

- `surface.is_typing` IPC 와 그것을 부르는 CLI `tasty is-typing`.
- `surface.send_wait_idle`(CLI `tasty send text --wait-idle`) — `typing` 이 참이면 보내지 않는다.
- claude plugin 의 API 에러 뒤 자동 재개([ADR-0521](0521-claude-auto-resume-after-an-api-error-is-opt-in-and-judged-at-send-time.md) 결정 5) — 실패한 턴이 시작된 뒤 `idle_seconds` 가 갱신됐으면 재개를 취소한다.

셋 다 "사람이 입력창에 뭔가 넣었으면 끼어들지 않는다" 로 읽는다. 그러니 좌변을 넓히면 셋 모두 **더 보수적으로**(덜 보낸다) 바뀐다.

이 결정 전 `record_typing` 을 부르는 자리는 키보드 경로 하나(`src/view/main/keyboard.rs` 의 `handle_keyboard_input` 끝)와 IME 경로 셋(`src/view/main/ime.rs`)이었다. 사용자가 입력창에 내용을 넣는 경로를 전수로 세면(`SendToSurface` 를 사용자 출처로 내는 자리 전부와 파일 드롭):

| 경로 | 입력창에 내용을 넣나 | 기록(이 결정 전) |
|---|---|---|
| 키보드(`keyboard_input`) | 넣는다 | 한다 |
| IME 조합·확정(`ime_commit`) | 넣는다 | 한다 |
| 붙여넣기 단축키 → `run_paste` → `paste_to_terminal` | 넣는다 | **간접으로 한다** — 아래 실측 |
| 명령 팔레트 "paste" → `run_paste` | 넣는다 | **안 한다** |
| 클릭 커서 이동(`click_cursor`) | 안 넣는다(화살표 시퀀스) | 안 한다 |
| 마우스 보고 · 휠(`mouse_report` · `mouse_wheel`) | 안 넣는다 | 안 한다 |
| 파일 드롭(`DispatchFile`) | 안 넣는다 — 번들 핸들러는 전부 `open_surface`(새 탭) | 안 한다 |
| mirror 이미지 붙여넣기 뒤 원격 경로 삽입 | 넣는다(업로드가 끝난 뒤) | 붙여넣는 순간은 위 두 붙여넣기 행과 같다(단축키는 수식키 키다운만, 팔레트는 없음). 삽입 순간(`src/app/image_upload.rs` 의 `dispatch_paste`)은 안 한다 |

**실측(2026-09-23, 격리 `TASTY_HOME` debug 인스턴스 · Xvfb · 이 결정 전 바이너리)**: 키보드 Ctrl+V 는 `record_typing` 을 부르지 않는 `run_paste` 로 가지만, **Ctrl 키다운이 따로 온 winit `KeyboardInput` 이라** 단축키가 아니므로 `handle_keyboard_input` 끝까지 내려가 기록한다. `xdotool key ctrl+v` 뒤 `{"idle_seconds":1.36,"typing":true}`, `xdotool key ctrl` 단독 뒤 `{"idle_seconds":0.61,"typing":true}`. 즉 수식키가 붙은 붙여넣기 바인딩(4 프리셋 기본값이 전부 그렇다)은 수식키 키다운이 먼저 기록해 구멍이 아니었다 — Ctrl 로 실측했고, Alt·Shift 키다운은 같은 경로를 지나지만 따로 재지 않았다. 구멍은 **키가 surface 에 닿지 않는 붙여넣기**다 — 명령 팔레트를 열어 "paste" 를 고르면 입력창에 텍스트가 들어가는데(`PALETTE_MARK_B` 가 화면에 나타남) `idle_seconds` 는 14.15 초 그대로였다. 수식키 없는 키 하나에 붙여넣기를 묶은 사용자 바인딩도 같은 형태다(단축키로 소비돼 기록 자리에 안 닿는다).

## Decision

**붙여넣기는 사용자 입력이다. 기록은 `MainView::run_paste`(`src/adapters/ui/input/shortcuts/copy_paste.rs`) 첫머리에서, 포커스된 surface 에 대해 한다.** `run_paste` 는 붙여넣기 단축키와 명령 팔레트가 공유하는 유일한 실행부라, 두 경로가 만나는 한 자리다. 이미지 붙여넣기(로컬 경로 삽입 · mirror 업로드)와 plugin 이 붙여넣기를 처리하는 surface(`egui_paste` capability)도 이 자리를 지난다.

"사용자 입력" 의 좌변은 **입력창에 내용을 넣는 사용자 경로** — 키보드 · IME · 붙여넣기 — 로 정한다. 마우스 보고 · 휠 · 클릭 커서 이동 · 파일 드롭은 넣지 않는다. 이 좌변은 `CoreState::record_typing` 의 문서 주석이 한 자리로 들고 있다.

## Consequences

- **얻은 것**: 팔레트로 붙여넣은 초안이 `surface.is_typing` 에 사람 있음으로 보인다 — 자동 재개가 그 초안 뒤에 재개 문구를 붙여 제출하지 않고, `--wait-idle` 전송도 5 초 동안 물러선다. 키 없는 붙여넣기 바인딩도 같다.
- **잃은 것**: 클립보드가 비어 아무것도 안 들어간 붙여넣기도 기록된다 — 사용자가 그 surface 에서 붙여넣기를 시도했다는 사실만 본다. 포커스된 surface 의 종류도 가리지 않는다 — 터미널도 `egui_paste` surface 도 아닌 surface(markdown · html · empty 등)에서 팔레트로 붙여넣으면 `paste_to_terminal` 은 아무것도 넣지 않는데 기록은 남는다(키보드 경로도 종류를 가리지 않아 같은 형태다). 소비자 셋이 모두 "기록 있음 = 덜 보냄" 방향이라 틀렸을 때 비용은 에이전트 전송이 한 번 미뤄지는 것이다.
- **잃은 것**: mirror 이미지 붙여넣기는 원격 경로가 실제로 들어가는 순간(업로드 완료)이 아니라 붙여넣은 순간을 기록한다. 업로드가 5 초를 넘기면 삽입 순간의 `typing` 은 거짓일 수 있다. 자동 재개가 보는 `idle_seconds`("턴 시작 뒤 입력이 있었나")에는 영향이 없다.
- **운영 비용 / 유지 부담**: 새 붙여넣기 진입점을 만들면 `run_paste` 를 거쳐야 기록된다. 회귀 시험은 `tests/gui_tests.rs` 의 `test_palette_paste_records_user_typing` 이고, 그 스위트는 `#[ignore]` 라 자동 채널이 없다(Xvfb 에서 손으로 돌린다).

## Alternatives Considered

- **`dispatch_paste`(`src/view/main/clipboard.rs`)에서 기록** — 텍스트가 실제로 입력창에 들어가는 자리라 mirror 업로드 뒤 삽입 순간까지 잡는다. 그러나 `egui_paste` surface 의 붙여넣기는 그 자리를 안 지나고, 클립보드가 빈 붙여넣기처럼 "사용자가 손댔다" 는 사실도 놓친다. 판정의 뿌리는 사용자 행동이지 PTY 쓰기가 아니라 기각.
- **`Core::apply` 의 `SendToSurface` 에서 출처가 사용자면 기록** — 한 자리로 모이지만 마우스 보고 · 휠 · 클릭 커서 이동도 사용자 출처 `SendToSurface` 라 라벨로 다시 걸러야 하고, egui-mesh surface 로 가는 키(`SendToSurface` 가 아님)와 IME preedit 기록은 그 자리를 안 지난다. 기존 4 자리를 옮기면 그 동작이 바뀐다. 기각.
- **마우스 입력까지 좌변에 넣기** — TUI 를 휠로 스크롤하는 것도 사람이 있다는 신호지만, 입력창 내용과 무관하다. `typing` 이라는 이름과 소비자 셋의 물음("입력창에 초안이 있을 수 있나")에서 벗어나고, 소비자 동작을 필요 이상으로 바꾼다. 기각.
- **파일 드롭까지 좌변에 넣기** — 번들 핸들러가 전부 새 탭을 열 뿐 입력창에 쓰지 않는다. 넣을 내용이 없는 경로라 기각.
- **아무것도 안 고치고 사실만 적기** — 팔레트 경로의 구멍이 실측으로 재현돼 기각.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 사용자 출처 `SendToSurface` 를 내는 새 자리가 생긴다 → 그것이 입력창에 내용을 넣는지 가려 좌변에 넣을지 정한다. 재는 법: `git grep -n -A8 'SendToSurface' -- src ':!*tests*' | grep 'from_user_'` 로 `SendToSurface` 를 감싼 사용자 출처 생성자(`src/intent.rs` 의 `from_user_shortcut` · `from_user_menu` · `from_user_context_menu` 셋 전부를 `from_user_` 로 덮는다)의 라벨 목록을 뽑아 이 ADR 의 표와 대조한다. 2026-09-23 실측 11 자리 · 라벨 6 종(`paste` 4 · `ime_commit` 1 · `keyboard_input` 1 · `mouse_report` 1 · `mouse_wheel` 2 · `click_cursor` 2), 생성자는 전부 `from_user_shortcut` 이다. 이 재는 법은 `SendToSurface {` 줄에서 생성자 줄까지가 창 8 줄 안이라는 것을 전제한다 — 같은 날 11 자리 모두 간격이 4 줄이라 최대 간격 4 다. payload 를 여러 줄로 짓는 새 자리는 창 밖으로 밀려 **조용히 빠지므로**, 비-시험 `SendToSurface` 등장 수(`git grep -n 'SendToSurface' -- src ':!*tests*'`, 같은 날 19 줄 — 위 11 자리 · 주석 5 · enum 정의 1 · `Core::apply` 의 match 팔 1 · IPC `send` 경로 1)와 대조해 뽑힌 11 자리 밖의 줄이 모두 생성자 자리가 아닌지 본다. 참고로 `git grep -n 'from_user_' -- src` 는 같은 날 169 줄(`from_user_shortcut` 79 · `from_user_menu` 66 · `from_user_context_menu` 24)이고, 앞 두 이름만 찾는 패턴은 145 줄이라 셋째 24 줄을 놓친다.
- 파일 핸들러 액션 종류에 입력창에 경로를 넣는 것이 생긴다 → 파일 드롭을 좌변에 넣는다. 재는 법: `crates/tasty-file-handler/src/types.rs` 의 `HandlerAction` 변형과 번들 plugin 매니페스트의 `[contributes.handler.action]` `kind`.
- `run_paste` 를 안 거치는 붙여넣기 진입점이 생긴다 → 기록 자리를 그쪽까지 넓힌다. 재는 법: `git grep -n 'paste_to_terminal\|dispatch_paste(' -- src` 의 호출자가 `run_paste` 와 mirror 업로드 완료(`src/app/image_upload.rs`) 밖에 없는가.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 사용자가 "붙여넣기만 했는데 에이전트 전송이 막혔다" 를 보고한다 → 빈 클립보드 기록(잃은 것)을 재검토한다. 재는 법: 그 상황의 클립보드 내용과 `surface.is_typing` 응답.

## References

- `src/adapters/ui/input/shortcuts/copy_paste.rs` 의 `run_paste` · `src/core/state.rs` 의 `record_typing`(좌변 정의) — 결정이 실현된 현재 위치
- `tests/gui_tests.rs` 의 `test_palette_paste_records_user_typing` — 회귀 시험(기록 줄을 빼는 변이에서 `idle_seconds=-1` 로 빨개짐을 확인)
- [ADR-0521](0521-claude-auto-resume-after-an-api-error-is-opt-in-and-judged-at-send-time.md) — 소비자 하나. 그 "잃은 것 — 붙여넣은 초안" 과 재검토 조건을 이 결정에 맞춰 정정했다
- 선행 결정 없음 — 탐색: `git grep -l 'record_typing\|is_typing\|paste_to_terminal\|run_paste' -- docs/adr/` (0521 만 걸린다 — 소비자 쪽 결정이다)
