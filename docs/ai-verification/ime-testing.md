# IME 시뮬레이션 검증

`surface.ime_*` IPC(**debug 빌드 전용** — 사용자 입력기 조합의 재현이라 release 표면에 없다, [ADR-0115](../adr/0115-input-reproduction-ipc-debug-isolation.md). window-local, **local-only** — 로컬 caller 만, `crates/tasty-ipc/src/method_meta.rs::PREFIX_RULES`)로 IME 입력을 프로그래밍 방식으로 시뮬레이션해 한글/CJK 입력 파이프라인 버그를 재현·검증한다. 핸들러는 `src/adapters/ipc/handler/ime.rs`. (debug 포트 `~/.tasty-debug/tasty.port`, 격리 인스턴스면 `$TASTY_HOME/tasty.port`.)

메서드: `surface.ime_enable` · `surface.ime_preedit {text}` · `surface.ime_commit {text}` · `surface.ime_status` · `surface.ime_disable`.

```python
# call() = ipc-usage.md 의 헬퍼 (read_line 사용)
call("surface.ime_enable")
call("surface.ime_preedit", {"text": "ㅎ"}); time.sleep(0.1)   # 조합 단계
call("surface.ime_preedit", {"text": "한"}); time.sleep(0.1)
call("surface.ime_commit",  {"text": "한"})                    # 확정 → PTY 전송
call("surface.ime_status")   # {"active": true, "preedit_text": null, "has_preedit": false}
call("surface.ime_disable")
```

## 검증 시나리오

1. **Preedit 렌더링 위치** — 터미널에 텍스트 입력 후 `ime_enable`+`ime_preedit "한"` → [스크린샷](screenshot-methods.md)으로 preedit 오버레이가 커서 위치에 파란 배경으로 셀 그리드 정렬되는지.
2. **연속 커밋 후 위치 이동** — `preedit "한"`→`commit "한"`→`preedit "글"` → preedit 이 오른쪽으로 이동하는지. (셸 에코 처리 전 다음 preedit 시작 시 커서 미갱신 — `sleep 0.1`.)
3. **IME 활성 중 ASCII** — ASCII 는 `surface.send` 로(KeyboardInput 통과), 한글은 IME 경로 → `tasty read since-mark --strip-ansi` 로 결과 확인.
4. **분할 패널** — `tasty split --level surface --target-surface this --direction horizontal` 후 아래 surface 에서 preedit 위치 검증.

## 검증 체크리스트

1. `ime_enable` → `ime_status` 가 `active: true`
2. `ime_preedit "한"` → `preedit_text: "한"`, `ime_preedit ""` → 클리어
3. `ime_commit "한"` → PTY 전송(`read since-mark`)
4. `ime_disable` → `active: false` + preedit 클리어
5. 연속 preedit→commit 시 텍스트 누적
6. 스크린샷에서 preedit 오버레이 위치·파란 배경 정렬

### 리팩터 후 flush/clear 회귀 확인 (handle_keyboard_input 상환 시)

1. 한글 조합 중(예: "한" 입력 중) **split/close 등 팝업 없는 단축키** → 조합 문자가 PTY 로 확정 전송(flush). 유실·중복 없음.
2. 한글 조합 중 **command palette·notifications(intent 팝업)** → 현재 동작상 **flush**(dispatch 지연으로 체크 시 미포커스). 상환 후에도 동일한지.
3. 열림·비포커스 search_bar 상태에서 조합 중 `find` 재입력 → 조합 문자 **폐기**(clear), PTY 미전송.
4. 조합 중 **Ctrl+letter**(예: 'ㅊ' 조합 중 Ctrl+C) → physical 폴백으로 control char(0x03) 전송, 조합문자 아님.
5. 위 3케이스에서 `ime_status.has_preedit` 가 처리 후 `false`.

## 제한

- 윈도우 레벨 상태(`MainView.ime_active`/`ime_preedit`)를 직접 조작 — OS 실제 IME 엔진 비관여.
- `surface_id` 지정 미지원 — 항상 포커스된 surface.
- 마우스 클릭에 의한 preedit 커밋은 시뮬레이션 불가(별도 경로).
- OS IME 후보창 위치(`set_ime_cursor_area`)는 호출되나 실제 OS IME 는 열리지 않음.
  egui-mesh 갈래(surface·popup)의 후보창 위치는 **plugin 이 알려온 값**을 host 가 창 좌표로
  올려 넘긴다([egui-mesh-channel](../dev-guide/egui-mesh-channel.md) "IME candidate 위치") —
  변환식 자체는 `plugin_bridge::mesh_ime_cursor_area` 단위 시험으로 재고, 후보창이 실제로
  그 자리에 뜨는지는 OS IME 가 그리는 창이라 tasty 프레임 캡처에 안 잡힐 수 있다. 그때는
  winit 에 넘긴 값을 stderr trace 로 관측한다 —
  `TASTY_LOG=tasty::view::main=trace` 로 띄우면 `ime cursor area: x=… y=… w=… h=…` 가
  찍힌다(dev 빌드의 **파일** 레이어는 `debug` 고정이라 trace 가 안 들어간다 — stderr 를
  받아야 한다). ★ **`xvfb-run` 으로 띄웠으면 그 줄은 stdout 파일에 있다** — xvfb-run 이
  명령을 `2>&1` 로 돌려 stderr 를 합친다. stderr 쪽만 grep 하면 0 건이 나와 경로가 끊긴
  것처럼 보인다(거짓 음성). **무엇으로 판정했는지 보고에 명시한다.**
  이 값은 조합 중이 아니어도 나온다 — plugin egui 의 `PlatformOutput::ime` 는 편집 위젯이
  focus 중이면 늘 채워지므로, IME 엔진 없이 **포커스만 주어도** 경로가 끝까지 흐르는지
  볼 수 있다. WM 없는 Xvfb 에서는 클릭 주입이 egui popup 에 안 닿으니
  `debug inject egui-key --key Tab` 으로 포커스 체인을 돌린다 — 몇 번째 Tab 이 입력란에
  닿는지는 그 popup 의 위젯 순서에 달렸으므로, Tab 하나마다 trace 줄 수의 **증가분**을
  재서 켜지는 자리와 꺼지는 자리를 갈라라. popup 을 닫은 뒤 증가분이 0 인 것이
  stale 캐시가 없다는 음성 대조다.
- 이 IPC/오버레이 경로는 **터미널 전용**이다. egui-mesh 의 IME 는 별도 경로이고, 그 안에서
  다시 둘로 갈린다 — **surface** 는 winit IME 를 `ime.rs` 의 forward 로 plugin 에 넘기고,
  **popup**(markdown 의 파일열기 팝업 경로 입력 필드 등)은 host egui ctx 에 들어온
  `Event::Ime` 를 `collect_mesh_popup_input` 이 긁어 넘긴다. 어느 쪽이든 plugin egui
  `TextEdit` 이 라이브 preedit 을 인라인 렌더한다([egui-mesh-channel](../dev-guide/egui-mesh-channel.md)
  "입력 forward"·"입력 게이트"). 어느 쪽도 이 IPC 로는 주입되지 않는다 —
  `surface.ime_*` 는 focused surface 의 **터미널** 오버레이 상태를 만지고, `debug.inject_key` 는
  키만 주입한다. 인라인 조합 표시는 헤드리스 시뮬레이션 불가 — 실제 OS IME 로 육안 검증한다.
  (markdown 주소창은 [ADR-0067](../adr/0067-markdown-webview-stage-b-scope-correction.md) 로 문서에 내장된
  HTML `<input>` 이 됐다 — IME 는 host native WebView 가 자체 처리하며 이 경로/egui-mesh 어느
  쪽에도 속하지 않는다.)
