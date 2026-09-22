# ADR (Architecture Decision Record) 인덱스

아키텍처/정책 결정의 *근거·대안·재검토 조건* 을 기록한다. design/ 문서가 "지금 어떻게 동작하나" 를 기술한다면, ADR 은 "왜 그렇게 결정했나" 를 기술한다.

- 신규 작성: [`template.md`](template.md) 양식을 따른다. 파일명 `XXXX-<slug>.md`, 번호는 4 자리 — 현재 최대 번호 + 1(빈 번호 재사용 금지, template "작성 규칙").
- **Accepted 후의 본문 수정 범위·Status 갱신·Supersede 절차**는 [`template.md`](template.md) 의 "작성 규칙" 을 따른다 — 이 인덱스에 규칙을 복제하지 않는다.
- **외부(비-git) 위치 문서 참조 금지** + 필요한 근거는 `docs/` 로 재구성해 참조 — 상세·예외는 [`template.md`](template.md) 의 "작성 규칙" 참조.
- 커밋 형식: [`dev-guide/commit-convention.md`](../dev-guide/commit-convention.md) 의 "ADR 커밋" 항목.
- **그룹**: ADR 은 아래 주제 그룹 중 정확히 한 곳에 한 행으로 있다. 새 ADR 의 그룹 선택 기준과 Superseded 행의 자리는 [`template.md`](template.md) 의 "인덱스에 행을 추가할 때" 를 따른다. 그룹 머리말은 결정 사슬(A → B 는 B 가 A 를 잇거나 개정한다)과 현재 운영 규칙이 사는 문서를 가리킨다.

## 그룹

- 터미널 에뮬레이션 · 입력
- 창 · 워크스페이스 · 포커스 · 수명주기
- 단축키 · modifier-hint
- UI · 테마 · 디자인 토큰 · 갤러리
- 길이 타입 · DPI
- 국제화
- 원격 attach · mirror · 점유
- attention · 알림
- IPC 전송 · 상한 · 기한 · 압력
- IPC 계약 · 오류 코드 · 멱등 키
- 사건 피드 · 출력 위치
- 권한 · 신뢰 경계 · 보안
- 웹훅 · 훅 핸들러
- plugin 시스템 — 경계 · namespace · 수명
- plugin 렌더 채널 · webview
- 번들 plugin 기능 — markdown · git-viewer · explorer · image · clipboard
- 파일 핸들러 · 파일 피커
- 에이전트 통합 · 협업
- 저장소 · 메모리 DB
- 아키텍처 · 헤드리스 · 크레이트 경계
- CLI · 로깅 · 에이전트 표면
- 빌드 · 배포 · 버전
- 테스트 · flake · 하네스
- CI 게이트 · 복잡도
- 가드 설계 · 측정 규율
- 문서 · ADR 규약

## 터미널 에뮬레이션 · 입력

VTE 지원 범위 · 마우스 리포팅 · PTY 수명. 마우스 리포팅 우회는 0019 → 0022 → 0023 사슬이고, 안내 배너는 0055 → 0061(per-app 억제 · 더보기 진입)로 이어진다. PTY 종료 감지(EOF 뒤 재-wake)는 0523 이고, 같은 종료 판정을 기다리는 시험의 상한은 테스트 그룹의 0211 이다.
운영 문서: [features/terminal](../features/terminal/index.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0002 | [VTE 파싱을 입력(winit) 스레드 밖의 per-terminal 파서 스레드로 분리](0002-vte-parsing-off-input-thread.md) | Accepted | 2026-06-15 | performance, terminal, threading, input-latency, vte |
| 0008 | [인라인 그래픽 프로토콜(Sixel / Kitty / iTerm)은 보류](0008-inline-graphics-protocols-deferred.md) | Deferred | 2026-06-17 | terminal, graphics, sixel, kitty, image, vte, scope, deferred |
| 0011 | [XTWINOPS 창 조작·창 상태 질의는 미지원 (크기/타이틀 스택만 응답)](0011-xtwinops-window-ops-unsupported.md) | Accepted | 2026-06-18 | terminal, xtwinops, vte, window, user-agent-separation, security, scope |
| 0012 | [tmux control mode(DCS) 및 DECRQSS 는 미지원](0012-tmux-dcs-decrqss-unsupported.md) | Deferred | 2026-06-18 | terminal, vte, dcs, tmux, decrqss, scope, deferred |
| 0013 | [레거시·니치 입력 사설 모드는 미지원](0013-niche-input-private-modes-unsupported.md) | Deferred | 2026-06-18 | terminal, vte, dec-private-mode, mouse, input, scope, deferred |
| 0014 | [폰트 ligature 는 보류 (현재 미지원, 추후 지원 계획 있음)](0014-font-ligatures-deferred.md) | Deferred | 2026-06-18 | font, ligatures, appearance, settings, rendering, cell-grid, scope, deferred |
| 0017 | [Windows 절전(suspend/resume) 후 PTY 헬스 복구는 Windows 전용으로 구현한다](0017-windows-suspend-resume-pty-recovery.md) | Accepted | 2026-06-21 | pty, conpty, suspend, resume, power-management, windows, platform, lifecycle, terminal, cross-platform |
| 0019 | [마우스 버튼/드래그 리포팅 — 트래킹 앱에 전면 위임, 로컬 선택 우회는 보류](0019-mouse-button-reporting-app-delegation.md) | Accepted | 2026-06-24 | terminal, vte, mouse, mouse-reporting, sgr, input, selection, scope |
| 0022 | [Shift+우클릭 modifier 우회 + 트래킹 안내 toast](0022-shift-rightclick-context-menu-bypass.md) | Accepted | 2026-06-25 | terminal, mouse, mouse-reporting, context-menu, modifier, discoverability, ux |
| 0023 | [Shift+좌클릭 드래그 = 마우스 리포팅 우회 로컬 텍스트 선택 + 안내 toast 범용화](0023-shift-leftclick-selection-bypass.md) | Accepted | 2026-06-26 | terminal, mouse, mouse-reporting, selection, modifier, clipboard, discoverability, ux |
| 0034 | [터미널 PTY 셸을 호스트(tasty) 수명에 결박한다](0034-terminal-shell-host-lifetime-binding.md) | Accepted | 2026-07-04 | process-lifetime, reaper, job-object, pty, terminal, windows, conpty, orphan, cross-platform, adr-0009 |
| 0050 | [agent-native headless PTY 는 `terminal.*` 확장이 아니라 신규 `pty.*` 네임스페이스로 제공한다](0050-headless-pty-primitive.md) | Accepted | 2026-07-14 | pty, headless, agent-native, terminal, surface-independence, ipc, cli, permission, adopt-terminal, exit-code, concurrency-limit, idle-ttl, adr-0040 |
| 0055 | [마우스 캡처 안내 배너 per-app 억제를 캡처 억제와 독립된 축으로 둔다](0055-mouse-capture-banner-suppress-list.md) | Accepted | 2026-07-28 | terminal, mouse, mouse-reporting, banner, settings, ux, adr-0022 |
| 0061 | [마우스 캡처 배너에 "더보기"(⋯) 퀵 엔트리를 추가해 per-app 블랙리스트 진입 경로를 배너 자신으로 확장한다](0061-mouse-capture-banner-more-menu-quick-entry.md) | Accepted | 2026-08-06 | terminal, mouse, mouse-reporting, banner, popup, settings, ux, i18n, gallery, adr-0024, adr-0055 |
| 0081 | [버튼 없는 hover motion(1003)은 focused surface 에만 보고한다](0081-hover-motion-focused-surface-only.md) | Accepted | 2026-08-24 | mouse, input, tracking, focus, terminal, adr-0019 |
| 0130 | [휠 1노치가 옮기는 거리는 창 안에서 하나이고, 그 값은 사용자가 정한다](0130-wheel-notch-distance-is-uniform-and-user-set.md) | Accepted | 2026-09-04 | input, scroll, egui, plugin-bridge, settings, accessibility, wire-contract, adr-0108 |
| 0261 | [busy 는 순간값이 아니라 상태다 — 입력은 진입만 막고, 유지는 못 끊는다](0261-busy-is-a-state-and-input-blocks-only-entry.md) | Accepted | 2026-09-11 | busy-indicator, terminal, sidebar, osc133, shell-integration, input-echo, adr-0002 |
| 0292 | [소거는 현재 배경색으로 칸을 채우고 pen 을 안 건드린다](0292-erase-fills-with-the-current-background.md) | Accepted | 2026-09-20 | terminal, vte, erase, bce, sgr, termwiz, adr-0002 |
| 0307 | [출력 스캐너는 자기 커서로 읽는다 — 에이전트의 mark 를 공유하지 않는다](0307-the-output-scanner-reads-its-own-cursor.md) | Accepted | 2026-09-20 | terminal, output-buffer, ipc, plugin, claude, cursor, polling, adr-0085, adr-0266, adr-0306 |
| 0523 | [PTY EOF 뒤에는 자식 종료가 판정될 때까지 parser 스레드가 계속 깨운다](0523-pty-eof-keeps-waking-until-the-exit-is-settled.md) | Accepted | 2026-09-23 | pty, terminal, process-exit, waker, headless, attach, structural-delta, flaky, ci, adr-0002, adr-0211, adr-0481 |

## 창 · 워크스페이스 · 포커스 · 수명주기

창 · 워크스페이스 · 닫기 · 부팅/종료. 종료 사슬은 0077 → 0078, 캡처는 0044 → 0118 이다. 포커스 보존은 0113 · 0125 · 0497 · 0502 가 각각 다른 자리(삭제 이동 · 재정렬 · 에이전트가 만든 창 · 에이전트가 만든 탭)를 정한다. 에이전트 창에 워크스페이스를 만드는 지목은 0514(「CLI · 로깅 · 에이전트 표면」).
운영 문서: [design/policies/focus](../design/policies/focus.md) · [architecture/close-sequence](../architecture/close-sequence.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0001 | [시스템 트레이 — 전 OS best-effort 지원 (graceful degradation)](0001-system-tray-best-effort.md) | Accepted | 2026-06-17 | system-tray, platform, background, cross-platform, windows, macos, linux |
| 0003 | [네이티브 윈도우 데코레이션 대신 CSD(Client-Side Decorations) 채택](0003-client-side-decorations.md) | Accepted | 2026-06-15 | window, csd, titlebar, cross-platform, winit, macos, windows, linux |
| 0029 | [워크스페이스 카테고리 — active 는 전역 인덱스 단일 진실 소스 유지](0029-workspace-category-global-index.md) | Accepted | 2026-06-29 | workspace, workspace-category, sidebar, indexing, focus |
| 0044 | [스크린샷을 focus-독립 + ID 지정으로 만들어 debug 격리에서 release 로 승격한다](0044-screenshot-release-promotion-surface-target.md) | Accepted | 2026-07-10 | screenshot, ipc, cli, focus-independence, offscreen-render, gpu, debug-ipc, local-only, adr-0032, adr-0040 |
| 0076 | [surface close 정리 루프에서 다른 프로세스/스레드를 기다리는 구간을 걷어낸다](0076-close-path-per-surface-blocking-removal.md) | Accepted | 2026-08-22 | close-sequence, pty, observer, blocking, render-thread, latency, cross-platform, adr-0002 |
| 0077 | [종료를 프레임 구동 상태 머신으로 전개하고 부팅과 같은 로딩 화면을 씌운다](0077-shutdown-loading-screen.md) | Accepted | 2026-08-23 | shutdown, boot, ui, state-machine, plugin |
| 0078 | [종료 중 IPC 요청은 무시하지 않고 즉시 거절한다](0078-shutdown-rejects-pending-ipc.md) | Accepted | 2026-08-23 | shutdown, ipc, cli, error-handling, adr-0077 |
| 0082 | [전체화면은 기존 요소를 확대하지 않고 독립 무대로 만든다](0082-fullscreen-independent-stage.md) | Accepted | 2026-08-24 | fullscreen, stage, ui, render-pipeline, layout, webview, attach, screenshot |
| 0087 | [레이아웃은 창마다 슬롯 파일 하나를 쓰고, 슬롯 점유는 살아있는 engine 에서 파생시킨다](0087-layout-slot-occupancy-model.md) | Accepted | 2026-08-25 | layout-persistence, multi-window, slot, occupancy, storage, boot, scrollback, gc |
| 0091 | [GPU 호출 행(hang)은 관측 워치독으로만 다룬다 — 렌더 스레드 분리는 채택하지 않는다](0091-render-stall-watchdog-observation-only.md) | Accepted | 2026-08-30 | gpu, wgpu, winit, event-loop, hang, watchdog, diagnostics, crash-report, render-thread |
| 0094 | [surface id 공간은 `PTY_ID_BASE` 미만으로 강제한다 — 경계에서 거부하고 부팅 시 침범분을 정리한다](0094-surface-id-space-bounded-below-pty-base.md) | Accepted | 2026-09-02 | surface-id, headless-pty, id-space, memory-db, boot, ipc, validation, invariant |
| 0113 | [삭제로 인한 인덱스 이동에서도 포커스 대상을 보존한다 — 사라진 것을 보고 있었을 때만 시야가 움직인다](0113-close-preserves-the-focused-target.md) | Accepted | 2026-09-04 | focus, user-agent-separation, close, cascade, workspace, tab, pane, index-vs-id, remote-attach, invariant |
| 0117 | [창·모달 생성 실패는 패닉이 아니다 — 부팅은 진단 후 종료, 그 외는 취소 후 안내하며 안내 채널은 요청 origin 이 가른다](0117-window-and-modal-creation-failure-policy.md) | Accepted | 2026-09-04 | error-handling, window, modal, boot, panic, toast, info-modal, focus, identity-principle-1, i18n, shutdown |
| 0118 | [캡처는 읽기라 행동 대상 집합을 넓히지 않는다 — 명시한 `window_id` 는 모달까지 찍고, 자동 선택은 main 창 하나로 유지한다](0118-screenshot-reads-any-window-explicit-id-only.md) | Accepted | 2026-09-04 | screenshot, ipc, window, modal, focus, identity-principle-1, identity-principle-3, local-only, ai-verification |
| 0120 | [에이전트가 부르는 워크스페이스 닫기의 경계](0120-agent-workspace-close-boundaries.md) | Accepted | 2026-09-04 | agent-surface, workspace, close, remote-attach, occupancy, identity-principles, adr-0040, adr-0113 |
| 0125 | [카테고리 착지점은 id 를 들고, 재정렬 축은 제거 축과 같은 초크포인트로 모은다](0125-category-landing-points-hold-ids.md) | Accepted | 2026-09-05 | focus, workspace, reorder, index-vs-id, cascade, invariant, adr-0113 |
| 0175 | [창 소유 목록의 합산 여부는 이름이 아니라 성질로 판정한다](0175-window-owned-list-membership-is-judged-by-shape-not-by-name.md) | Accepted | 2026-09-05 | focus-independence, ipc, routing, list-aggregation |
| 0251 | [폴백으로 가는 dispatch 메서드는 술어가 아니라 사유로 판정한다](0251-unrouted-dispatch-methods-carry-a-reason-not-a-predicate.md) | Accepted | 2026-09-09 | ipc, routing, focus, guards, roster, multi-window, adr-0133, adr-0175 |
| 0497 | [에이전트가 만든 창은 사용자의 포커스를 가져가지 않는다](0497-an-agent-created-window-does-not-take-the-users-focus.md) | Accepted | 2026-09-22 | focus, window, multi-window, ipc, cli, user-agent-separation, identity, winit, x11, wayland, stacking |
| 0502 | [에이전트가 만든 탭은 사용자가 보던 탭을 바꾸지 않는다](0502-an-agent-created-tab-does-not-take-the-users-tab.md) | Accepted | 2026-09-23 | focus, tab, ipc, cli, user-agent-separation, identity, attach, file-handler, adr-0302, adr-0497 |

## 단축키 · modifier-hint

modifier-hint 는 0035(좁힘 + 지연) · 0038(빈 섹션) · 0064(타이머 리셋)가 서로 다른 결정이라 합치지 않는다. 단축키 이식은 0257 → 0269 이다.
운영 문서: [features/keybindings](../features/keybindings/index.md) · [design/policies/key-mapping](../design/policies/key-mapping.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0035 | [modifier-hint 오버레이 — 눌린 조합으로 섹션 좁힘 + Shift 단독 표시 지연 1.2초](0035-modifier-hint-combo-narrowing-and-shift-delay.md) | Accepted | 2026-07-05 | modifier-hint, overlay, keybindings, combo, subset, reveal-delay, shift, design-token, accessibility, debug-ipc, adr-0510 |
| 0038 | [modifier-hint 빈 조합 섹션은 "바인딩 없음" 플레이스홀더로 표시한다](0038-modifier-hint-empty-combo-placeholder.md) | Accepted | 2026-07-06 | modifier-hint, overlay, keybindings, empty-state, placeholder, design-token, i18n, accessibility, debug-ipc, adr-0035, adr-0510 |
| 0064 | [modifier-hint 표시 지연 타이머는 등록된 단축키가 실제로 소비되면 리셋한다](0064-modifier-hint-reveal-timer-reset-on-shortcut.md) | Accepted | 2026-08-08 | modifier-hint, overlay, reveal-delay, keybindings, discovery, user-agent-separation, adr-0035 |
| 0256 | [바인딩 문자열 파서는 그 문자열을 저장하는 크레이트에 둔다 — 매칭 레이어는 결과만 소비한다](0256-the-binding-parser-lives-with-the-setting-it-parses.md) | Accepted | 2026-09-09 | keybindings, parser, crate-boundary, feature-gate, headless, single-source |
| 0257 | [단축키 이식 번들은 스키마 태그가 붙은 TOML 한 장이고, 미설치 plugin 의 override 는 import 에서 버린다](0257-the-keybinding-bundle-is-a-toml-file-with-a-schema-tag.md) | Accepted | 2026-09-09 | keybindings, portability, import-export, toml, schema, plugin, crate-boundary, warnings |
| 0269 | [단축키 가져오기는 고른 행을 draft 에 얹고, 번들에 없는 plugin override 는 건드리지 않는다](0269-keybinding-import-applies-selected-rows-onto-the-draft.md) | Accepted | 2026-09-13 | keybindings, import-export, settings, draft, plugin, option-migration, conflict |

## UI · 테마 · 디자인 토큰 · 갤러리

디자인 작업의 흐름과 갤러리 완전성은 0510 한 편이다. 갤러리 미러는 0329(대조 전에 미러를 없앨 수 있는지 먼저 본다)가 원칙이고 0380(토스트 카드)이 그 적용이다 — 두 편은 서로 인용하지 않으므로 여기서 잇는다. 스케일 밖 값은 0126 → 0290 이다. 구조 전달 실패 toast 는 0401 → 0503(에이전트 origin 제외 개정)이다.
운영 문서: [design/systems/theme](../design/systems/theme.md) · [dev-guide/gallery-first](../dev-guide/gallery-first.md) · [dev-guide/popup-implementation](../dev-guide/popup-implementation.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0024 | [Banner — Modal/Popup/Toast 에 이은 4번째 오버레이 개념(별도 매니저)](0024-banner-fourth-overlay-concept.md) | Accepted | 2026-06-26 | ui, overlay, banner, popup, toast, ubiquitous-language, user-agent-separation |
| 0033 | [UI 색은 semantic role 접근자로만 — primitive 필드 직접 접근 전면 금지(위젯 포함)](0033-ui-color-semantic-role-only.md) | Accepted | 2026-07-03 | design-tokens, color, semantic, primitive, theme, ui-widgets, guard, enforcement, adr-0510 |
| 0036 | [플러그인 아이콘은 빌드타임 SVG 베이크 + `tasty-icons` 단일 소스로 그린다](0036-plugin-icon-buildtime-bake-tasty-icons-single-source.md) | Accepted | 2026-07-05 | plugin, icons, tasty-icons, single-source, build-time-bake, svg, vector, egui-mesh, design-parity, i18n, adr-0510, adr-0028, adr-0030 |
| 0058 | [plugin이 트리거하는 host 소유 popup은 즉시 ack + 이벤트 push로 비동기 결과를 회신한다](0058-plugin-triggered-host-popup-async-ack-push.md) | Accepted | 2026-08-01 | plugin, ipc, popup, async, event-bus, host-delegation, file-picker, host-agnostic, adr-0042, adr-0043, adr-0053, adr-0056 |
| 0063 | [Popup 닫힘 뒷정리는 `draw_popups` 가 아니라 `PopupDef.on_close` 훅 + `PopupManager::close()` 단일 관문으로 처리한다](0063-popup-close-hook-single-choke-point.md) | Accepted | 2026-08-08 | ui, popup, lifecycle, close-hook, choke-point, refactor |
| 0068 | [Host popup ↔ plugin popup z-order — 공유 z_seq + 조건부 GPU pass·sublayer 순서](0068-host-plugin-popup-shared-z-seq.md) | Accepted | 2026-08-12 | popup, plugin, z-order, gpu-rendering, egui |
| 0069 | [공용 `Table` 의 `selectable` 표는 셀 텍스트 선택을 포기하고 행 클릭을 보장한다](0069-table-row-click-over-cell-text-selection.md) | Accepted | 2026-08-13 | ui, shared-widgets, table, egui, hit-test, selectable-labels, explorer, port-scanner, gallery |
| 0071 | [네이티브 컨텍스트 메뉴는 "즉시 반환 + 프레임 폴링" 계약으로 바꾸고, 해소 타이밍은 플랫폼별로 다르게 둔다](0071-native-context-menu-async-contract.md) | Accepted | 2026-08-15 | native-menu, context-menu, linux, x11, gtk, winit, event-loop, async, no-hang, cross-platform |
| 0079 | [스크롤 어포던스 표준은 "스크롤바 숨김 + 가장자리 페이드" — 폭 예약은 예외로만 남긴다](0079-scroll-affordance-standard.md) | Accepted | 2026-08-23 | ui, scroll, egui, affordance, popup, table, remote-tool, port-scanner |
| 0080 | [latte 중성 램프의 AA 미달을 알려진 예외로 수용한다](0080-latte-neutral-ramp-contrast-exception.md) | Accepted | 2026-08-24 | theme, accessibility, contrast, latte, palette |
| 0084 | [plugin 이 트리거한 host popup 은 자진 신고한 부모 instance 로 스택을 이룬다](0084-plugin-triggered-host-popup-ownership.md) | Accepted | 2026-08-24 | popup, plugin, ipc, lifecycle, ownership |
| 0126 | [스케일 밖 값은 토큰으로 스냅하지 않는다 — `.5` 폰트 값은 토큰이 될 수 없다](0126-off-scale-font-values-are-not-snapped-to-tokens.md) | Accepted | 2026-09-04 | theme, design-tokens, font-size, corner-radius, status-dot, zoom, guards, adr-0033 |
| 0174 | [접근성 "모션 감소" 는 `Theme` 이 실어 나르고, 위젯의 기본값이 그것을 읽는다](0174-theme-carries-reduced-motion.md) | Accepted | 2026-09-05 | accessibility, theme, motion, widgets, defaults |
| 0176 | [모션 지속시간은 `Millis` 로 `Theme` 경계를 건넌다](0176-motion-durations-cross-the-theme-boundary-as-millis.md) | Accepted | 2026-09-05 | design-tokens, motion, typed-values, theme, code-generation |
| 0254 | [떠 있는 표면의 그림자는 두 값뿐이고, 뷰포트를 점유하는 쪽이 더 크다](0254-floating-surface-shadow-scope-rule.md) | Accepted | 2026-09-09 | design-tokens, theme, shadow, modal, popover, scrim, gallery, guards, adr-0139 |
| 0273 | [plugin popup 은 소속 범위의 종류만 선언하고 대상은 host 가 바인딩한다](0273-plugin-popup-declares-a-scope-kind-and-the-host-binds-the-target.md) | Accepted | 2026-09-14 | popup, plugin, scope, manifest |
| 0278 | [자식 파일 피커는 부모의 유효 범위를 상속하고 숨김 동안 작업을 보존한다](0278-child-file-picker-inherits-parent-scope-and-preserves-hidden-work.md) | Accepted | 2026-09-15 | popup, file-picker, scope, ownership, input |
| 0290 | [2026-09-17 결정이 divergence alias 집합과 스케일 밖 값 집합을 닫았다](0290-settled-role-gaps-close-the-divergence-and-off-scale-sets.md) | Accepted | 2026-09-19 | design-tokens, color, semantic, role, font-size, opacity, status-dot, theme, adr-0033, adr-0126 |
| 0299 | [스크롤 영역은 드래그 패닝 여부를 선언한다](0299-scroll-areas-declare-whether-they-pan-on-drag.md) | Accepted | 2026-09-20 | ui, scroll, input, guard, egui |
| 0300 | [scrim 은 popup 이 소속된 범위를 덮는다 — 늘 창 전체가 아니다](0300-the-scrim-covers-the-popups-scope-not-always-the-window.md) | Accepted | 2026-09-20 | popup, scope, scrim, plugin, design-tokens, gallery, adr-0273, adr-0254 |
| 0329 | [갤러리 미러는 대조하기 전에 없앨 수 있는지 먼저 본다](0329-a-mirror-is-removed-before-it-is-compared.md) | Accepted | 2026-09-20 | gallery, widgets, duplication, guards, toast, type-appearance, compiler-enforced |
| 0380 | [토스트 카드의 치수·색은 도출이 하나다 — 갤러리는 본체의 alpha 곱 순서를 따른다](0380-the-toast-card-geometry-and-color-have-one-derivation.md) | Accepted | 2026-09-21 | toast, gallery, shared-widgets, color, alpha, identity, measurement, adr-0510, adr-0122 |
| 0401 | [원격 연결 상태 사건은 사용자 행동 없이도 toast 를 띄울 수 있다 — toast 트리거 정책의 허용 부류](0401-remote-connection-events-may-raise-a-toast-without-a-user-action.md) | Accepted | 2026-09-21 | toast, attach, mirror, ui, identity-principle-1, user-agent-separation |
| 0460 | [셀 강조색의 alpha 는 CPU 에서 그 셀 배경 위에 합성한다](0460-cell-highlight-alpha-is-composited-on-the-cpu-over-the-cell-bg.md) | Accepted | 2026-09-21 | renderer, gpu, theme, search, selection, alpha, blending, compatibility |
| 0503 | [에이전트 intent 의 적용 실패는 사용자 toast 가 아니라 로그로 간다 — ADR-0401 의 구조 전달 실패 조항 개정](0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md) | Accepted | 2026-09-23 | toast, attach, mirror, intent, origin, identity-principle-1, user-agent-separation, adr-0401 |
| 0510 | [디자인 작업은 Claude Design 시안을 갤러리 → 본체 → 사이트 사본 순으로 정합한다 — 갤러리는 본체 UI 의 완전한 단일 출처다](0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md) | Accepted | 2026-09-23 | design-workflow, claude-design, gallery, gallery-first, design-parity, component-catalog, site, vendor, guards, adr-0138, adr-0506 |
| 0522 | [프리셋 surface 설정 화면의 draft 는 kind 를 바꿔도 값을 지우지 않는다](0522-the-preset-surface-settings-draft-keeps-values-across-kind-switches.md) | Accepted | 2026-09-23 | preset, layout-presets, draft, surface-settings, design-parity, egui, input, adr-0510 |

## 길이 타입 · DPI

튜플 생성자 봉인 조항은 0128 · 0145 · 0169 가 각자 적었고 0509 가 그 조항을 모았다(세 원본의 다른 조항은 유효). 인접한 0135(배율) · 0148(물리 상수의 용도) · 0161(전환 순서) · 0252(부류 하한)는 서로 다른 결정이라 합치지 않는다.
운영 문서: [concepts/typed-length](../concepts/typed-length.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0128 | [DPI 변환 정합은 타입 봉인이 아니라 소스 스캔 가드로 지킨다](0128-dpi-conversion-guarded-by-source-scan-not-sealed-types.md) | Accepted | 2026-09-04 | typed-length, dpi, guard, scale-factor, geometry, drift-guard |
| 0135 | [본체의 UI 길이 리터럴은 배율을 안 탄다 — 갤러리는 탄다](0135-ui-length-literals-do-not-follow-ui-scale-in-the-app.md) | Accepted | 2026-09-05 | theme, design-tokens, ui-scale, zoom, egui, gallery, guards, adr-0126, adr-0033 |
| 0145 | [길이 newtype 의 생성자는 당분간 열어 둔다 — 봉인의 이득이 정확성이 아니라 판정기의 정밀도이기 때문](0145-typed-length-constructors-stay-open-for-now.md) | Accepted | 2026-09-05 | typed-length, dpi, guard, tooling-cost |
| 0148 | [물리 px 상수는 "무엇을 위한 값인가" 로 갈라 다룬다](0148-physical-px-constants-are-split-by-what-they-are-for.md) | Accepted | 2026-09-05 | dpi, typed-length, layout, design-tokens, hidpi, adr-0145 |
| 0161 | [길이 상수의 `f32` → `LogicalPx` 전환은 이름 수가 아니라 경로 길이 순서로 간다](0161-length-constant-conversion-is-ordered-by-path-length.md) | Accepted | 2026-09-05 | typed-length, geometry, guards, refactor, census, frontier, false-negative |
| 0169 | [길이 타입의 튜플 생성자는 봉인하지 않는다 — 탈출구가 곧 같은 단언이기 때문](0169-the-tuple-constructor-of-length-types-stays-open.md) | Accepted | 2026-09-05 | typed-length, geometry, guards, dpi, sealing, census |
| 0252 | [축의 바늘에 걸린 `1` 둘 — 퇴화 방지 하한은 부류로, 정규화 좌표는 자리 명부로 뺀다](0252-a-degenerate-floor-is-a-class-and-unit-space-is-a-roster.md) | Accepted | 2026-09-09 | guards, design-tokens, exemption, class-vs-roster, predicate, ratchet, adr-0126, adr-0135, adr-0139 |
| 0509 | [길이 타입 튜플 생성자는 봉인하지 않는다 — ADR-0128 · ADR-0145 · ADR-0169 의 봉인 조항 통합](0509-length-constructor-sealing-clause-consolidated.md) | Accepted | 2026-09-23 | typed-length, dpi, guards, sealing, adr-conventions, adr-0128, adr-0145, adr-0169, adr-0506, adr-0148 |

## 국제화

언어팩은 0114 → 0124(빈 값 규칙) → 0276(plugin 번역)으로 이어진다.
운영 문서: [dev-guide/i18n](../dev-guide/i18n.md) · [features/language-packs](../features/language-packs/index.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0103 | [활성 로케일은 host 프로세스 env 로 plugin 에 전달한다 — 부팅 단일 스레드 구간에서 한 번 set 한다](0103-plugin-locale-via-host-process-env.md) | Accepted | 2026-09-03 | i18n, locale, plugin, boot, env, unsafe, language-pack |
| 0106 | [위젯 밖 사용자 문자열(알림 제목 · IPC 기본값 · 폴백 라벨)도 `t()` 를 거치고, 기계 식별은 제목이 아니라 식별 필드로 한다](0106-non-widget-user-strings-go-through-i18n.md) | Accepted | 2026-09-03 | i18n, notifications, hooks, ipc, remote-attach, git-viewer, plugin, wire-format |
| 0114 | [언어팩은 `~/.tasty/lang/<code>/pack.toml` 디렉토리이고 `[font]` 가 필수다 — 단일 `<code>.toml` 은 내장 오버라이드 전용, 팩 부재·형상 위반은 경고 토스트 + 영어 폴백(설정값 보존)](0114-language-pack-directory-shape-and-english-fallback.md) | Accepted | 2026-09-03 | i18n, language-pack, settings, font, boot, toast, discovery |
| 0124 | [빈 값이 "번역 없음" 이라는 규칙은 로드 경로와 무관하다 — 언어팩·내장 오버라이드·plugin 언어파일이 같은 규칙을 쓰고, 폴백 대상만 다르다](0124-blank-value-rule-is-load-path-independent.md) | Accepted | 2026-09-04 | i18n, language-pack, blank-value, unicode, trim, adr-0114 |
| 0193 | [폰트 resolve 는 글리프 유무만 해결한다 — RTL 어순은 범위 밖](0193-font-resolution-covers-glyph-coverage-not-rtl-ordering.md) | Accepted | 2026-09-07 | i18n, font, rtl, egui, scope-boundary, adr-0139, adr-0114 |
| 0276 | [plugin 사용자 번역은 host 언어 루트에서 같은 순서로 읽는다](0276-plugin-user-catalogs-share-the-host-language-root.md) | Accepted | 2026-09-15 | i18n, plugin, language-pack, compatibility |
| 0280 | [CLI 도움말은 표시 문구를 번역하고 프로토콜 값은 유지한다](0280-cli-help-localizes-presentation-not-protocol.md) | Accepted | 2026-09-15 | cli, i18n, plugin, compatibility |

## 원격 attach · mirror · 점유

프로필은 0015 → 0032(2-레이어, 0015 대체), 점유는 0040 → 0049 · 0052(0040 부분 대체) · 0060 · 0116 · 0156 · 0157 이다. 손실 재동기화는 0334 → 0400 → 0450, 닫은 항목 복원은 0264 → 0480, mirror markdown 은 0255 → 0268 이다.
운영 문서: [dev-guide/attach-behavior](../dev-guide/attach-behavior.md) · [features/remote-attach](../features/remote-attach/index.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0007 | [attach 는 원격을 대상으로 한다 (로컬 self-attach 는 debug 격리)](0007-attach-targets-remote.md) | Accepted | 2026-06-17 | attach, remote, debug-isolation, cli, user-agent-separation, security |
| 0015 | [원격 접속 프로필 = 범용 typed 레지스트리, attach 는 소비자](0015-remote-profiles-typed-registry.md) | Superseded by 0032 | 2026-06-19 | remote, profile, registry, attach, ssh, smb, extensibility, plugin, ubiquitous-language |
| 0032 | [원격 프로필을 ssh(연결) / tasty-attach(attach) 2-레이어로 분리](0032-remote-attach-two-layer-split.md) | Accepted | 2026-07-01 | remote, profile, attach, ssh, two-layer, ref, port-file, cli |
| 0040 | [점유를 약한/강한(soft/hard) 2계층으로 나누고 AI 에이전트를 점유 주체로 일반화한다](0040-occupancy-soft-hard-tiers-agent-occupant.md) | Superseded by 0052 (부분) | 2026-07-07 | occupation, soft-occupy, hard-occupy, actors, ai-agent, child-terminal, attach, readonly, marker, focus-independence, adr-0007, adr-0032 |
| 0045 | [mirror grid geometry 는 client 가 구동하고 remote 는 reflow 메커니즘으로 확정한다](0045-mirror-geometry-client-driven.md) | Accepted | 2026-07-11 | attach, remote, mirror, geometry, resize, protocol, client-driven, backward-compat, headless, adr-0007, adr-0040 |
| 0049 | [강한 점유(hard occupy)의 readonly 는 PTY 상호작용만 차단한다 — 로컬 selection 은 예외, 휠/링크클릭은 계속 차단](0049-hard-occupancy-selection-exception.md) | Accepted | 2026-07-13 | occupation, hard-occupy, readonly, selection, mouse, wheel, link-click, mirror, attach, adr-0040 |
| 0052 | [강한 점유는 attach heartbeat TTL 만료도 EOF 와 동등한 해제 사유로 인정한다](0052-attach-heartbeat-ttl-hard-occupancy-release.md) | Accepted | 2026-07-20 | occupation, hard-occupy, attach, heartbeat, ttl, disconnect, occupancy-registry, adr-0040 |
| 0053 | [로컬+원격 겸용 native 파일 피커 — attach 커스텀 이벤트 채널 + 하이브리드 신뢰 모델](0053-native-file-picker-remote-attach-channel.md) | Accepted | 2026-07-23 | file-picker, popup, attach, mirror, ipc, permission, fs-read, occupancy-trust, timeout, wire-format, tools-menu, adr-0042, adr-0032, adr-0040 |
| 0054 | [원격 파일 바이트 전송(bulk)은 attach 스트림 위 native binary 채널로 구현한다 (sftp/SMB 등 외부 프로토콜은 이 기능에 미사용)](0054-remote-filesystem-native-over-attach-stream.md) | Accepted | 2026-07-23 | remote, attach, mirror, file-transfer, native-protocol, stream, bulk-channel, no-base64, ssh-delegation, cross-platform, adr-0007, adr-0032, adr-0045 |
| 0056 | [git-viewer 원격(attach mirror) 조회 — 공유 crate + `git_viewer.query` 이벤트 채널](0056-git-viewer-remote-attach-git-query-channel.md) | Accepted | 2026-07-29 | git-viewer, plugin, attach, mirror, ipc, event-bus, wire-format, tools-menu, egui-mesh, popup, timeout, adr-0053, adr-0028, adr-0040 |
| 0059 | [explorer 원격(attach mirror) 브라우징 — 기존 list_dir 채널 재사용 + browse-only](0059-explorer-remote-attach-list-dir-reuse-browse-only.md) | Accepted | 2026-08-01 | explorer, attach, mirror, remote, list-dir, browse-only, occupancy-trust, view-store, wire-format, adr-0053, adr-0054, adr-0056 |
| 0060 | [hard-occupied workspace 로의 `terminal.spawn` 을 구조 변경 차단 대상에 포함한다](0060-block-terminal-spawn-into-hard-occupied-workspace.md) | Accepted | 2026-08-05 | occupation, hard-occupy, attach, terminal-spawn, agent-collaboration, adr-0040 |
| 0070 | [원격 포트 발견에 3중 상한(ssh `ConnectTimeout` + 프로세스 감시 + 호출 전체 예산)을 건다](0070-port-discovery-timeout.md) | Accepted | 2026-08-13 | remote-attach, ssh, port-discovery, timeout, no-hang, i18n, adr-0032, adr-0053 |
| 0086 | [mirror 워크스페이스로의 `terminal.spawn` 은 거부하고, 대상 판정을 최종 pane 으로 옮긴다](0086-reject-terminal-spawn-into-mirror-workspace.md) | Accepted | 2026-08-25 | attach, mirror, terminal-spawn, structural-forward, orphan-resource, ipc, adr-0060 |
| 0110 | [창 없는 parked engine 에도 mirror 이벤트를 즉시 적용한다 — 버퍼링·흐름제어 대신 로컬 PTY 출력과 같은 대칭](0110-mirror-events-apply-to-parked-engines.md) | Accepted | 2026-09-04 | remote-attach, mirror, parked-engine, multi-window, data-loss, performance |
| 0116 | [attach 점유는 핸드셰이크가 검증된 뒤에만 잡는다 — proto 불일치와 self-attach 는 점유 전에 거절한다](0116-attach-handshake-validated-before-occupancy.md) | Accepted | 2026-09-04 | attach, remote-attach, occupancy, handshake, protocol-version, self-attach, stream, adr-0040, adr-0052 |
| 0121 | [attach 신뢰경계는 원격 조회를 덮고 로컬 구조 op 는 덮지 않는다 — `remote.attach` 는 plugin 미개방](0121-attach-trust-boundary-covers-remote-queries-not-local-structural-ops.md) | Accepted | 2026-09-04 | ipc, permissions, remote-attach, plugin, trust-boundary, user-agent-separation, identity-principle-1, method-table, asymmetry |
| 0156 | [닫기 요청은 원격이 점유한 surface 를 파괴하지 않는다 — 사후 정리는 예외다](0156-a-close-request-does-not-destroy-an-occupied-surface.md) | Accepted | 2026-09-05 | attach, occupancy, close, data-loss, ipc, gui, symmetry, adr-0040, adr-0120 |
| 0157 | [끊긴 holder 는 재attach 를 막지 못한다](0157-a-disconnected-holder-does-not-block-a-reattach.md) | Accepted | 2026-09-05 | attach, occupancy, stream, ordering, headless, adr-0040, adr-0052 |
| 0255 | [attach mirror 의 markdown surface 는 픽셀이 아니라 **원문**을 나른다 — 새 role 과 lazy 조회 채널](0255-markdown-attach-mirror-forwards-content-not-pixels.md) | Accepted | 2026-09-09 | markdown, attach, mirror, remote, wire-format, webview, surface-role, lazy-fetch, budget, occupancy-trust, adr-0053, adr-0056, adr-0059, adr-0065 |
| 0264 | [mirror 의 "닫은 항목 복원" 은 원격에서 실행하고, 복원 스택은 워크스페이스로 스코프한다](0264-mirror-restore-closed-item-runs-on-the-remote.md) | Accepted | 2026-09-12 | attach, mirror, remote, restore, closed-item, wire-format, focus, user-agent-separation, adr-0040, adr-0045, adr-0086 |
| 0267 | [mirror surface 의 cwd 는 서버가 push 하고, 그 경로는 원격 출처로 타입에서 구분한다](0267-mirror-surface-cwd-is-pushed-by-the-server.md) | Accepted | 2026-09-13 | attach, mirror, remote, cwd, wire-format, provenance, newtype, inherit-cwd, file-picker, adr-0056, adr-0059, adr-0086, adr-0255 |
| 0268 | [끊긴 mirror markdown 문서는 옛 원문 대신 끊김을 보이고, 재연결은 변경 신호 한 번으로 되돌린다 — ADR-0255 의 항목 5·6 개정](0268-a-disconnected-mirror-markdown-shows-the-disconnect-and-reconnect-refetches.md) | Accepted | 2026-09-13 | markdown, attach, mirror, remote, reconnect, disconnect, surface-kind, deferred-plugin, adr-0255 |
| 0334 | [버린 스트림 프레임은 그것을 받겠다고 말한 client 에게만 알린다](0334-a-dropped-stream-frame-is-told-to-the-clients-that-asked-for-it.md) | Accepted | 2026-09-20 | attach, stream, ipc, capability, backpressure, forward-compat, adr-0312 |
| 0400 | [attach 손실은 연결 단위로 재동기화한다 — 종류마다 계약을 두고, 한 연결에는 그중 가장 강한 것을 쓴다 — ADR-0334 의 통지 지연 조항 개정](0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md) | Accepted | 2026-09-21 | attach, stream, ipc, backpressure, resync, mirror, cli, observability, adr-0334 |
| 0450 | [밀린 손실 통지는 sink 에 자리가 나는 순간 갚는다 — ADR-0400 의 통지 지연 조항 개정](0450-a-pending-loss-notice-is-queued-the-moment-the-sink-has-room.md) | Accepted | 2026-09-21 | attach, stream, ipc, backpressure, resync, cli, adr-0400, adr-0334 |
| 0480 | [forward 된 구조 op 는 누가 요청했는지를 싣고, 에이전트의 close 는 서버 복원 스택에 안 남는다 — ADR-0264 의 결정 4 개정](0480-a-forwarded-close-carries-who-asked-for-it.md) | Accepted | 2026-09-22 | attach, mirror, remote, restore, closed-item, wire-format, user-agent-separation, identity, adr-0264, adr-0395 |
| 0481 | [점유 워크스페이스의 forward 아닌 구조 변경도 기존 StructuralDelta 로 holder 에게 보낸다](0481-a-server-side-structure-change-reaches-the-holder-as-a-delta.md) | Accepted | 2026-09-22 | attach, mirror, remote, wire-format, structural-delta, pty-exit, occupancy, adr-0040, adr-0264 |
| 0482 | [forward 가 사라진 surface 를 지목하면 IPC 와 같은 "no live surface" 사유로 거절한다](0482-a-forward-naming-a-gone-surface-is-answered-like-ipc.md) | Accepted | 2026-09-22 | attach, mirror, remote, error-message, wire-format, parity, adr-0395 |

## attention · 알림

0039(공유 primitive) → 0062(kind-aware store) → 0098 · 0104 · 0109(mirror · 소유자 · 홀더) → 0107(IPC/CLI 해제).
운영 문서: [features/surface-highlight](../features/surface-highlight/index.md) · [features/notifications](../features/notifications/index.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0039 | [Surface highlight 는 producer 중립 공유 primitive](0039-surface-highlight-shared-primitive.md) | Accepted | 2026-07-07 | surface-highlight, notification, ipc, cli, state, focus-independence |
| 0062 | [Surface attention 상태를 kind-aware `AttentionStore` 로 확장한다](0062-attention-store-kind-aware-primitive.md) | Accepted | 2026-08-08 | surface-highlight, attention, notification, state, adr-0039 |
| 0098 | [mirror surface 의 attention 은 서버 push 만을 소스로 갖는다 — 로컬 발동은 억제하고 forward 하지 않는다](0098-mirror-local-attention-raise-suppressed.md) | Accepted | 2026-09-03 | attention, surface-highlight, remote-attach, mirror, source-of-truth, osc133, notification, ipc |
| 0104 | [attention 은 소유자만 발동하고, 확인(해제)은 실제로 본 주체가 한다 — 미러의 해제 edge 를 소유 인스턴스로 전달한다](0104-mirror-attention-clear-forwarded-to-owner.md) | Accepted | 2026-09-03 | attention, surface-highlight, remote-attach, mirror, clear, edge-trigger, occupancy, stream-control |
| 0107 | [attention 해제를 IPC/CLI 로 노출하고, 상태 변경은 IPC 핸들러가 직접 적용한다](0107-attention-clear-ipc-symmetry.md) | Accepted | 2026-09-03 | attention, surface-highlight, ipc, cli, headless, cascade, intent, attach, mirror, api-symmetry |
| 0109 | [하드 점유 중인 surface 의 attention 은 홀더만 해제한다 — 서버 로컬 포커스·알림 읽음은 게이트된다](0109-hard-occupancy-attention-clear-holder-only.md) | Accepted | 2026-09-03 | attention, surface-highlight, occupancy, hard-occupy, remote-attach, readonly, adr-0040, adr-0049 |

## IPC 전송 · 상한 · 기한 · 압력

수신 상한은 0304 → 0327(무응답 종료 개정) → 0391 · 0392, 응답 기한은 0328 → 0366 → 0452 와 0411 → 0451(훅 스텝의 대기 스레드 — 0498, 「웹훅 · 훅 핸들러」), dispatch 회차는 0313 → 0410 · 0465 → 0413 이다. 압력 관측은 0305 → 0340(histogram) → 0333 · 0412 · 0435 · 0436 · 0466 · 0467 · 0468, plugin 채널 포화는 0315 → 0339 · 0360 이다.
운영 문서: [dev-guide/api-conventions](../dev-guide/api-conventions.md) · [features/telemetry](../features/telemetry/index.md) · [dev-guide/timer-hub](../dev-guide/timer-hub.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0004 | [IPC transport = 127.0.0.1 loopback TCP (동적 포트)](0004-ipc-transport-tcp.md) | Accepted | 2026-06-16 | ipc, transport, tcp, loopback, security, trust-boundary, cross-platform |
| 0122 | [winit 루프에 스케줄되는 실패 가능한 release IPC op 은 완료 채널로 결과를 돌려준다 — fire-and-forget 금지](0122-winit-scheduled-fallible-ipc-returns-outcome.md) | Accepted | 2026-09-04 | ipc, window, event-loop, agent, error-handling, identity-principle-1, fire-and-forget, completion-channel, adr-0117 |
| 0304 | [IPC 수신에 상한을 둘 둔다 — 요청 한 줄의 바이트와 동시 연결 수](0304-ipc-admission-carries-two-bounds-a-line-and-a-connection-count.md) | Accepted | 2026-09-20 | ipc, resource-bounds, transport, reliability |
| 0305 | [요청 압력은 프로세스 게이지다 — caller 별 관측과 다른 축이다](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md) | Accepted | 2026-09-20 | ipc, telemetry, diagnostics, observability |
| 0313 | [dispatch 회차 예산은 동시 연결 상한과 같은 수이고, 이월은 별도 배선 없이 성립한다](0313-the-dispatch-round-budget-is-the-connection-bound.md) | Accepted | 2026-09-20 | ipc, dispatch, backpressure, headless |
| 0315 | [plugin 채널의 두 방향은 포화에 다르게 답한다 — 호스트→plugin 은 거절, plugin→호스트 는 대기](0315-the-two-directions-of-a-plugin-channel-answer-saturation-differently.md) | Accepted | 2026-09-20 | plugin, host-plugin, resource-bounds, backpressure, reliability, adr-0304, adr-0311 |
| 0327 | [전송 계층은 침묵 대신 답하고, 그 답의 쓰기에는 시간 상한이 있다 — ADR-0304 의 무응답 종료 조항 개정](0327-the-transport-answers-instead-of-going-silent-and-its-writes-are-bounded.md) | Accepted | 2026-09-20 | ipc, transport, resource-bounds, reliability |
| 0328 | [응답 대기의 상한은 호출자가 싣고, 만료는 "실행 여부 불명" 이다](0328-the-response-wait-is-bounded-by-the-caller-and-expiry-means-the-outcome-is-unknown.md) | Accepted | 2026-09-20 | ipc, transport, timeout, wire, idempotency, reliability |
| 0333 | [압력 게이지는 local-only 한 메서드 하나로 읽고, 응답은 모수마다 갈린다](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md) | Accepted | 2026-09-20 | telemetry, ipc, cli, method-effect, local-only, pressure, memory, plugin-host, adr-0305 |
| 0339 | [호스트는 포화로 버린 요청 수를 다음 요청에 얹어 plugin 에게 알린다](0339-the-host-tells-a-plugin-what-saturation-dropped.md) | Accepted | 2026-09-20 | plugin, host-plugin, wire-protocol, backpressure, resource-bounds, forward-compat, adr-0315 |
| 0340 | [압력 응답은 자리를 따로 세고 시간에는 고정 경계 분포를 단다 — ADR-0305 의 histogram 유보 조항 개정](0340-the-pressure-answer-counts-seats-and-carries-a-fixed-bound-distribution.md) | Accepted | 2026-09-20 | telemetry, ipc, pressure, histogram, saturation, connections, observability, adr-0305, adr-0333 |
| 0360 | [plugin 채널은 큐마다와 합계로 바이트에 묶인다 — 빈 큐는 한 건을 늘 받는다](0360-plugin-channels-are-bounded-in-bytes-per-queue-and-in-total.md) | Accepted | 2026-09-21 | plugin, host-plugin, resource-bounds, backpressure, observability, adr-0315, adr-0339 |
| 0366 | [CLI 는 단발 요청의 응답 대기를 루트 플래그로 자르고, 못 거는 상대와 못 싣는 명령은 거절한다](0366-the-cli-bounds-a-single-request-wait-with-a-root-flag.md) | Accepted | 2026-09-21 | ipc, cli, envelope, response-timeout, capability, compatibility, adr-0312, adr-0328, adr-0365 |
| 0391 | [명령 큐는 쌓인 바이트와 주입 깊이로 입장을 판정한다](0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md) | Accepted | 2026-09-21 | ipc, resource-bounds, admission, backpressure, queue, host-call, webhook, adr-0304, adr-0313, adr-0327 |
| 0392 | [첫 요청 줄에만 idle 기한을 건다](0392-only-the-first-request-line-has-an-idle-deadline.md) | Accepted | 2026-09-21 | ipc, resource-bounds, connection, idle, timeout, compatibility, adr-0304, adr-0327 |
| 0410 | [dispatch 회차는 시간 예산에서도 멈추고, 호출자는 도착 순으로 섬긴다 — ADR-0313 의 회차 길이 조항 개정](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md) | Accepted | 2026-09-21 | ipc, dispatch, backpressure, fairness, headless, measurement, adr-0313, adr-0305 |
| 0411 | [큐에서 기한이 지난 요청은 실행하지 않고 "실행 안 됨" 으로 답한다 — 만료는 취소가 아니다](0411-a-request-whose-deadline-passed-in-the-queue-is-answered-as-not-run.md) | Accepted | 2026-09-21 | ipc, transport, timeout, wire, compatibility, cancellation, reliability, adr-0328, adr-0410 |
| 0412 | [in-flight 는 "시작됐고 호출자가 아직 기다리는" 요청을 센다 — 큐 계측은 한 자리에서 읽는다](0412-in-flight-counts-a-started-request-while-its-caller-still-waits.md) | Accepted | 2026-09-21 | ipc, telemetry, diagnostics, observability, queue, adr-0305, adr-0391, adr-0411 |
| 0413 | [gui 에서 IPC wake 는 루프의 나머지에 차례를 넘기고, 예산에서 멈춘 회차는 루프를 다시 깨운다](0413-in-gui-an-ipc-wake-yields-to-the-rest-of-the-loop-and-a-cut-round-wakes-it-again.md) | Accepted | 2026-09-21 | ipc, dispatch, fairness, gui, winit, timers, measurement, adr-0410, adr-0313 |
| 0435 | [큐 입장 · 큐 꺼냄 · 재시도 판정은 압력 응답에 세 덩어리로 더한다](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md) | Accepted | 2026-09-21 | ipc, cli, telemetry, pressure, admission, dispatch, idempotency, retry, compatibility, adr-0333, adr-0391, adr-0412, adr-0422 |
| 0436 | [IPC 요청은 호스트가 번호를 매기고, 느린 요청은 그 번호로 plugin 대기까지 한 줄에 남긴다](0436-an-ipc-request-is-numbered-by-the-host-and-slow-ones-are-kept-in-a-ring.md) | Accepted | 2026-09-21 | ipc, plugin, telemetry, pressure, correlation, diagnostics, compatibility, adr-0305, adr-0435 |
| 0451 | [호스트 주입도 제 대기 상한을 기한으로 싣는다 — ADR-0411 의 호스트 주입 조항 개정](0451-a-host-injection-carries-its-wait-as-a-deadline.md) | Accepted | 2026-09-21 | ipc, host-injection, timeout, cancellation, reliability, terminal, adr-0411, adr-0391 |
| 0452 | [CLI 의 계약 확인도 같은 응답 대기 상한 안에서 끝난다 — ADR-0366 의 사전 확인 조항 개정](0452-the-cli-capability-check-spends-the-same-response-bound.md) | Accepted | 2026-09-21 | ipc, cli, envelope, response-timeout, capability, compatibility, adr-0366, adr-0411 |
| 0465 | [headless 는 이벤트 채널에 IPC wake 를 하나만 두고, 예산에서 멈춘 회차가 루프를 다시 깨운다 — ADR-0313 의 headless 이월 조항 개정](0465-headless-keeps-one-ipc-wake-in-its-channel-and-a-cut-round-wakes-it-again.md) | Accepted | 2026-09-22 | ipc, dispatch, fairness, headless, timers, plugin, wake, measurement, adr-0313, adr-0410 |
| 0466 | [큐 대기 평균은 대기를 더한 수로 나눈다](0466-the-queue-wait-mean-divides-by-the-waits-it-summed.md) | Accepted | 2026-09-22 | telemetry, pressure, ipc, queue, modulus, measurement, adr-0305, adr-0333 |
| 0467 | [accept 대기는 `connections` 덩어리에 상한으로 싣는다](0467-the-accept-wait-is-reported-as-a-bound-in-the-connection-block.md) | Accepted | 2026-09-22 | telemetry, pressure, ipc, accept, connection, measurement, adr-0333, adr-0340 |
| 0468 | [느린 요청 줄의 호스트 몫은 호출자가 받은 답을 싣는다](0468-the-slow-request-host-part-carries-the-answer-the-caller-got.md) | Accepted | 2026-09-22 | telemetry, pressure, ipc, slow-requests, outcome, measurement, adr-0436, adr-0411 |

## IPC 계약 · 오류 코드 · 멱등 키

미라우팅 응답 0154 · 0163 · 0167 은 오류 코드 `-32015`~`-32017` 각각의 결정이고 소스가 번호로 인용하므로 합치지 않는다(0425 가 file handler 에 적용 — 「파일 핸들러 · 파일 피커」 그룹). 멱등 키는 0306 → 0338 → 0361 → 0423(선언 값 개정) · 0420 · 0421 · 0422 이다.
운영 문서: [dev-guide/api-conventions](../dev-guide/api-conventions.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0154 | [플랫폼이 못 하는 메서드는 "없다" 가 아니라 "여기선 못 한다" 로 답한다](0154-a-platform-gated-dispatch-arm-answers-why-not-what.md) | Accepted | 2026-09-05 | ipc, debug, cross-platform, error-codes, cli, guards, adr-0115 |
| 0163 | [등재된 이름은 "없다" 가 아니라 "부를 수 있는 주체가 다르다" 로 답한다](0163-a-registered-name-answers-who-not-whether.md) | Accepted | 2026-09-05 | ipc, error-codes, plugin, agent-surface, guards, adr-0154, adr-0140, adr-0152 |
| 0167 | [등재된 이름은 "없다" 가 아니라 "이 바이너리에 안 들어 있다" 로 답한다](0167-a-registered-name-answers-whether-it-is-in-this-binary.md) | Accepted | 2026-09-05 | ipc, error-codes, headless, build-combination, guards, adr-0154, adr-0163 |
| 0171 | [호스트가 준 오류 코드는 plugin 경계를 넘어 살아남는다 — ADR-0153 의 "한 겹 감싸짐" 조항 개정](0171-a-host-error-code-survives-the-plugin-boundary.md) | Accepted | 2026-09-05 | ipc, error-codes, plugin, sdk, wire-protocol, partial-amendment, adr-0153, adr-0154, adr-0163, adr-0167 |
| 0306 | [메서드는 "두 번 전달되면 무엇이 남는가" 를 표에 선언한다](0306-a-method-declares-what-a-second-delivery-leaves-behind.md) | Accepted | 2026-09-20 | ipc, method-table, retry, contract |
| 0312 | [서버는 자기 버전이 아니라 협상 가능한 것을 선언한다](0312-the-server-declares-what-it-can-negotiate-not-what-version-it-is.md) | Accepted | 2026-09-20 | ipc, capability, compatibility, method-table, system-info, adr-0306 |
| 0338 | [변경 명령의 재시도는 호출자 키로 구별하고, 그 계약은 부수효과 전에 상대에게 묻는다](0338-a-mutation-retry-is-told-apart-by-a-caller-key-and-the-peer-is-asked-before-the-effect.md) | Accepted | 2026-09-20 | ipc, protocol, idempotency, capability, retry |
| 0361 | [plugin namespace forward 는 멱등 키 계약 밖이라고 선언된다 — 호스트는 정확히 한 번을 약속하지 않는다](0361-a-plugin-namespace-forward-is-declared-outside-the-idempotency-contract.md) | Accepted | 2026-09-21 | ipc, protocol, idempotency, plugin, namespace, retry, adr-0338 |
| 0420 | [멱등 키의 봉투 검사는 진입 게이트에서 한다 — 목적지와 무관한 판정](0420-the-idempotency-key-envelope-is-judged-at-the-admission-gate.md) | Accepted | 2026-09-21 | ipc, protocol, idempotency, envelope, gate, compatibility, adr-0338 |
| 0421 | [App 층도 멱등 키 계약을 지킨다 — 진행 중인 키에 온 재시도는 첫 실행에 합류한다](0421-the-app-layer-keeps-the-idempotency-contract-and-a-running-key-is-joined.md) | Accepted | 2026-09-21 | ipc, idempotency, retry, app-layer, concurrency, deferred-response, headless, adr-0338, adr-0361 |
| 0422 | [재시도 누계는 판정하는 보존소가 센다 — 노출 전 스냅샷까지만](0422-the-retry-counts-are-kept-by-the-store-that-decides.md) | Accepted | 2026-09-21 | ipc, idempotency, telemetry, retry, pressure, observability, adr-0305, adr-0333, adr-0421 |
| 0423 | [메서드마다 멱등 키 계약과 그것을 지키는 판을 선언한다 — ADR-0361 의 선언 값 조항 개정](0423-each-method-declares-its-key-contract-and-the-version-that-keeps-it.md) | Accepted | 2026-09-21 | ipc, protocol, idempotency, capability, method-meta, compatibility, adr-0312, adr-0338, adr-0361, adr-0421 |

## 사건 피드 · 출력 위치

0321(발화 자리) → 0322(위치를 든 링) → 0323(피드) → 0405 · 0406 · 0407 · 0456(용량 단위 개정), 그리고 0321 → 0501(Experimental 등급은 경고, 구독 게이트 아님). 터미널 출력 읽기 0341 · 0365 는 0322 와 같은 "위치로 답하고 못 준 것을 말한다" 규약의 적용처다.
운영 문서: [features/terminal-output](../features/terminal-output/index.md) · [features/agent-collaboration](../features/agent-collaboration/index.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0321 | [agent 사건은 이미 있는 깔때기에서만 발화한다](0321-agent-domain-events-publish-only-at-the-funnel-that-already-exists.md) | Accepted | 2026-09-20 | events, event-bus, agent, task, barrier, plugin-protocol, catalog |
| 0322 | [사건 링은 위치를 들고, 못 준 것을 말한다](0322-the-event-ring-keeps-positions-and-says-what-it-dropped.md) | Accepted | 2026-09-20 | events, event-bus, ring-buffer, offsets, retention, cursor, adr-0321 |
| 0323 | [피드는 위치로 읽고, 서버는 소비자 상태를 안 든다](0323-the-feed-is-read-by-position-and-the-server-keeps-no-consumer-state.md) | Accepted | 2026-09-20 | events, ipc, cli, cursor, long-poll, method-effect, identity, adr-0322 |
| 0341 | [터미널 출력 읽기는 소비자가 든 위치로 답하고, 못 준 것을 말한다](0341-a-terminal-output-read-answers-from-a-position-the-consumer-holds.md) | Accepted | 2026-09-21 | terminal, output, cursor, retention, ipc, method-effect, identity, adr-0307, adr-0322, adr-0323 |
| 0365 | [출력 위치 계약은 이름으로 협상하고, CLI 는 그 이름을 확인한 뒤에만 위치를 싣는다](0365-the-output-cursor-contract-is-negotiated-by-name-before-the-cli-sends-it.md) | Accepted | 2026-09-21 | ipc, capability, compatibility, cli, terminal, output, cursor, adr-0307, adr-0312, adr-0341 |
| 0405 | [피드 끝보다 뒤인 위치는 조용히 기다리지 않고 표지를 단다](0405-a-position-past-the-end-of-the-feed-is-marked-not-waited-on-silently.md) | Accepted | 2026-09-21 | events, event-bus, cursor, offsets, epoch, compatibility, adr-0322, adr-0323 |
| 0406 | [dispatch 에 응답하기 전의 publish 는 호스트가 hop 을 올린다](0406-the-host-raises-the-hop-of-a-publish-made-while-a-dispatch-is-unanswered.md) | Accepted | 2026-09-21 | events, event-bus, plugin, hop, loop-prevention, compatibility, adr-0321 |
| 0407 | [`events follow` 는 재부착을 넘어 세대를 들고 간다](0407-events-follow-carries-the-generation-across-a-reattach.md) | Accepted | 2026-09-21 | events, cli, cursor, epoch, reconnect, compatibility, adr-0323, adr-0405 |
| 0456 | [사건 링은 개수와 바이트 중 먼저 닿는 쪽으로 밀어낸다 — ADR-0322 의 용량 단위 조항 개정](0456-the-event-ring-evicts-by-count-or-bytes-whichever-comes-first.md) | Accepted | 2026-09-21 | events, event-bus, ring-buffer, retention, resource-bounds, plugin, adr-0322, adr-0360 |
| 0501 | [사건의 Experimental 등급은 경고이지 구독 게이트가 아니다](0501-the-experimental-event-grade-is-a-warning-not-a-subscription-gate.md) | Accepted | 2026-09-23 | events, event-bus, plugin, manifest, stability, compatibility, adr-0321 |

## 권한 · 신뢰 경계 · 보안

게이트 순서는 0152 → 0277(중첩 게이트 · Allow 위치 개정)이다.
운영 문서: [dev-guide/plugin-permissions](../dev-guide/plugin-permissions.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0005 | [memory secret 영역은 "안전 보관소" 가 아니다](0005-memory-secret-not-a-vault.md) | Accepted | 2026-06-16 | memory, secret, security, encryption, plugin, trust-boundary |
| 0016 | [Passkey 저장소 — path 수렴 · 파일권한 위임 · 참조 모델](0016-passkey-store-path-convergence.md) | Accepted | 2026-06-19 | passkey, secret, security, file-permission, trust-boundary, remote-profile |
| 0115 | [OS 전역 입력 조작(`surface.raw_key` · `surface.switch_input_source` · `surface.ime_*`)은 debug 표면으로 격리한다](0115-input-reproduction-ipc-debug-isolation.md) | Accepted | 2026-09-04 | ipc, security, input-injection, debug-isolation, macos, ime, focus-independence, method-table, guard-test, adr-0044 |
| 0141 | [호스트 키 namespace `tasty.` 는 raw `memory.*` kv 표면에서 예약한다](0141-host-key-namespace-is-reserved-in-raw-memory-kv.md) | Accepted | 2026-09-05 | security, permissions, memory, ipc, plugin, audit |
| 0152 | [게이트는 라우팅보다 먼저 돈다 — 조기 응답이 검사 자리를 건너뛴다](0152-gates-run-before-routing-not-inside-it.md) | Accepted | 2026-09-05 | security, permissions, ipc, plugin, routing, guards, telemetry, audit |
| 0246 | [텔레메트리에는 옵트아웃 축을 두지 않는다 — 그 권한 토큰은 경계가 아니라 선언이다](0246-telemetry-has-no-opt-out-and-its-token-is-a-declaration.md) | Accepted | 2026-09-08 | telemetry, privacy, permissions, plugin, trust-boundary, cap, non-goal, adr-0141 |
| 0271 | [plugin namespace 는 권한 셋을 가진 모든 caller 에게 그 namespace 의 토큰으로 열린다](0271-a-plugin-namespace-is-invoked-with-its-token-from-every-gated-caller.md) | Accepted | 2026-09-14 | permissions, plugin, ipc, agent, session-token |
| 0277 | [IPC 진입 검사와 허용 관측은 요청마다 한 번 수행한다 — ADR-0152의 중첩 게이트·Allow 위치 개정](0277-ipc-admission-and-observation-run-once.md) | Accepted | 2026-09-15 | ipc, rate-limit, telemetry, permissions, headless |

## 웹훅 · 훅 핸들러

남용차단은 0046 → 0195 → 0196 → 0197 → 0198 → 0199 → 0200 → 0281 이다. 각 ADR 이 다른 상수 · 타입 · 불변식(401 계수 · 출처 키 · 문턱 · 쿨다운 · `Screened` 타입 · body 상한)을 만들어 합치지 않는다. 훅 핸들러는 0047 → 0298 · 0430(병합 순서) · 0498(훅 스텝의 대기 스레드) → 0515(수동 발화도 같은 실행기)이다.
운영 문서: [features/webhook](../features/webhook/index.md) · [features/hooks](../features/hooks/index.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0046 | [인바운드 웹훅 — owner 신뢰 모델 + 단방향 ACK/데이터·흐름 분리 불변식](0046-webhook-owner-trust-one-way-ack.md) | Accepted | 2026-07-11 | webhook, inbound, security, trust-boundary, one-way-ack, data-flow-separation, ipc, owner-trust, cross-platform, adr-0004 |
| 0047 | [훅/웹훅 공유 훅 핸들러 레지스트리 + source(트리거 출처) 게이트](0047-shared-hook-handler-registry-source-gate.md) | Accepted | 2026-07-11 | hook-handler, webhook, hook, registry, source-gate, file-handler-mirror, trigger-source, patch-semantics, ipc-sequence, shell-command |
| 0048 | [웹훅 HTTP 레이어 = tiny_http (blocking, tokio 없음, TLS 는 위임)](0048-webhook-http-tiny-http-blocking.md) | Accepted | 2026-07-11 | webhook, http, tiny-http, blocking, tokio, dependency, tls, request-smuggling, cross-platform, adr-0004, adr-0046 |
| 0195 | [남용차단은 인증 실패(401)도 센다 — ADR-0046 의 남용차단 조항 개정](0195-abuse-counting-includes-rejected-tokens.md) | Accepted | 2026-09-08 | webhook, security, rate-limit, abuse, auth, buckets, adr-0046, adr-0112 |
| 0196 | [남용차단의 문턱값과 출처 키 — 그 값이 지키는 것을 재서 못박는다](0196-abuse-thresholds-and-source-key.md) | Accepted | 2026-09-08 | webhook, security, rate-limit, abuse, thresholds, source-key, measurement, adr-0046, adr-0195 |
| 0197 | [출처 표의 문턱은 상한이 아니라 정리 트리거다 — 그 값이 지키는 것을 재서 정한다](0197-the-source-table-cap-is-a-prune-trigger.md) | Accepted | 2026-09-08 | webhook, security, rate-limit, abuse, resource-bound, measurement, adr-0196, adr-0195 |
| 0198 | [쿨다운의 수명은 진입 시점이 정한다 — 연장 없음, 만료 시 백지](0198-a-cooldown-is-fixed-at-entry.md) | Accepted | 2026-09-08 | webhook, security, rate-limit, abuse, cooldown, time-semantics, adr-0196, adr-0197 |
| 0199 | [차단 판정은 body 를 읽기 전에 끝난다 — 순서를 주석이 아니라 소유권으로 적는다](0199-the-block-is-decided-before-the-body-is-read.md) | Accepted | 2026-09-08 | webhook, security, rate-limit, abuse, resource-bound, type-enforcement, adr-0046, adr-0112 |
| 0200 | [웹훅 body 는 요청당 바이트 상한을 갖는다 — 선언된 길이와 chunked 를 함께 막는다](0200-webhook-body-has-a-per-request-byte-cap.md) | Accepted | 2026-09-08 | webhook, security, resource-bound, dos, body-limit, measurement, adr-0112, adr-0199, adr-0046 |
| 0281 | [웹훅 body 상한의 선행 가정 오류와 미충족 연결 정리를 기록한다](0281-webhook-parser-cap-and-connection-drain.md) | Proposed | 2026-09-15 | webhook, body-limit, tiny-http, connection-drain, resource-bound, adr-0200 |
| 0298 | [훅 핸들러 시퀀스는 CLI 에서 제자리로 고친다 — 지우고 다시 만들지 않는다](0298-a-hook-handler-sequence-is-edited-in-place-from-the-cli.md) | Accepted | 2026-09-20 | hook-handler, cli, ipc, registry, ipc-sequence, local-only, settings, adr-0046, adr-0047 |
| 0430 | [hook handler 병합은 출처 순서(Host → Plugin → User)로 하고 user patch 를 늘 마지막에 둔다 — ADR-0047 의 병합 순서 조항 개정](0430-hook-handler-merge-applies-user-patches-last.md) | Accepted | 2026-09-21 | hook-handler, registry, plugin, settings, boot, headless, patch-semantics, adr-0047, adr-0427 |
| 0498 | [surface 훅의 IpcSequence 는 호스트 명령 큐를 비우는 스레드 밖에서 실행한다](0498-a-surface-hook-sequence-runs-off-the-thread-that-drains-the-queue.md) | Accepted | 2026-09-23 | hooks, hook-handler, ipc, host-injection, main-thread, concurrency, adr-0451 |
| 0515 | [수동 발화한 훅 시퀀스는 surface 훅과 같은 실행기에 줄 선다 — ADR-0498 의 수동 발화 조항 개정](0515-a-manually-dispatched-hook-sequence-joins-the-surface-hook-worker.md) | Accepted | 2026-09-23 | hooks, hook-handler, ipc, concurrency, logging, adr-0498 |

## plugin 시스템 — 경계 · namespace · 수명

namespace 라우팅은 0140 → 0153 → 0171(오류 코드 개정 — 「IPC 계약 · 오류 코드 · 멱등 키」 그룹) · 0173 · 0179 · 0282 · 0311 이다.
프로세스 수명의 메인 스레드 대기는 0457(종료 회수) → 0505(기동의 연결 대기, 같은 형태의 대칭)이다.
운영 문서: [dev-guide/plugin-development](../dev-guide/plugin-development.md) · [concepts/plugins](../concepts/plugins.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0009 | [Plugin sandbox 는 보류 — OS-level opt-in 을 우선 후보로](0009-plugin-sandbox-deferred.md) | Deferred | 2026-06-17 | plugin, sandbox, security, wasm, seccomp, trust-boundary, deferred |
| 0010 | [Plugin marketplace 는 보류 — 로컬 path install 유지](0010-plugin-marketplace-deferred.md) | Deferred | 2026-06-17 | plugin, marketplace, registry, trust, distribution, deferred |
| 0031 | [Lua 스크립트의 tasty 접근은 고정 호스트 API 표면으로만 — state 직접 접근 불가 + 워커 스레드 격리](0031-lua-host-api-only-worker-isolated.md) | Accepted | 2026-07-01 | lua, scripting, host-api, worker-thread, snapshot, command-queue, capability-boundary, sandbox, init-lua-removal, observe-only, adr-0009, adr-0028 |
| 0043 | [convert 시 파일 입력이 필요한 kind 를 capability 로 라우팅](0043-convert-input-popup-capability.md) | Accepted | 2026-07-09 | surface-kind, convert, plugin, de-pluginize, capability, popup, adr-0028, adr-0042 |
| 0132 | [매니페스트가 선언한 인자 타입은 파서가 강제한다 — 변환 실패는 값을 버리지 않고 거부한다](0132-declared-arg-types-are-enforced-not-documentation.md) | Accepted | 2026-09-04 | plugin, cli, manifest, argument-parsing, error-handling, agent-facing, silent-failure |
| 0140 | [호스트 IPC prefix 는 집행할 수 있는 자리에서 예약한다 — 파생이 아니라 고정으로](0140-host-ipc-prefixes-are-reserved-where-they-can-be-enforced.md) | Accepted | 2026-09-05 | plugin, ipc, manifest, namespace, guards, compatibility, adr-0133 |
| 0153 | [번들 plugin 이 점유한 namespace 아래의 host 메서드는 그 plugin 이 되돌려 준다](0153-a-bundled-namespace-hands-host-methods-back.md) | Accepted | 2026-09-05 | plugin, ipc, namespace, routing, guards, identity-principle-2, adr-0140, adr-0143 |
| 0158 | [plugin CLI 이름과 호스트 명령의 충돌은 등록 시점에서만 판정한다](0158-cli-name-collisions-are-judged-at-registration-not-in-the-manifest.md) | Accepted | 2026-09-05 | plugin, cli, manifest, guards, layering, adr-0140 |
| 0172 | [뒤에 로컬 정리가 있는 훅 핸들러는 host 호출 실패를 전파하지 않는다](0172-a-hook-handler-that-cleans-up-locally-does-not-propagate.md) | Accepted | 2026-09-05 | plugin, error-handling, hook, agent-integration, adr-0075, adr-0092 |
| 0173 | [namespace 해소는 프로세스 표가 아니라 매니페스트를 읽는다](0173-namespace-resolution-reads-the-manifest-not-the-process-table.md) | Accepted | 2026-09-05 | plugin, ipc, routing, headless, namespace, error-codes, adr-0167, adr-0136, adr-0171 |
| 0178 | [필요성이 트리거와 무관한 일은 부팅에 걸고, 기동만 지연에 둔다](0178-a-job-whose-need-is-independent-of-the-trigger-is-anchored-at-boot.md) | Accepted | 2026-09-05 | lifecycle, boot, plugin, lazy-init, source-guard, adr-0050, adr-0136, adr-0173 |
| 0179 | [해소하는 crate 에 표를 넘긴다 — 주입하는 것은 함수가 아니라 데이터다](0179-the-resolver-is-handed-the-table-not-a-callback.md) | Accepted | 2026-09-06 | plugins, ipc, derived-state, encapsulation, global-state, adr-0173, adr-0178 |
| 0182 | [테스트 인스턴스는 기본적으로 번들 plugin 을 스테이징하지 않는다](0182-test-instances-do-not-stage-bundled-plugins-by-default.md) | Accepted | 2026-09-06 | testing, harness, plugin, performance, disk-io |
| 0191 | [로컬에 둘 다 있는 파일은 해시하지 않고 바이트로 비교한다](0191-two-local-files-are-compared-bytewise-not-hashed.md) | Accepted | 2026-09-07 | plugin, install, performance, measurement, hashing, cold-cache, adr-0182, adr-0139 |
| 0259 | [헤드리스도 plugin surface kind 를 등록하고, 그 kind 를 지목한 생성 요청이 **소유자 하나**를 띄운다](0259-a-kind-request-starts-the-owner-that-declares-it.md) | Accepted | 2026-09-10 | headless, plugin, surface-kind, lazy-start, attach, markdown, trust-boundary, adr-0136, adr-0173, adr-0255 |
| 0282 | [Namespace 호출은 owner와 필요한 활성 IPC hook extension만 시작한다](0282-namespace-invocation-starts-only-its-owner-and-matching-extension.md) | Accepted | 2026-09-15 | ipc, plugins, lifecycle, headless |
| 0311 | [namespace 호출의 만료는 fail-open 이 아니라 caller 에 대한 오류다](0311-a-namespace-call-expires-into-an-error-not-a-fail-open.md) | Accepted | 2026-09-20 | plugin, ipc, timeout, host-plugin, error-handling, adr-0078 |
| 0457 | [단건 plugin 종료는 메인 스레드 밖에서 회수하고, 새 프로세스는 옛 것이 빠진 뒤에 뜬다](0457-a-single-plugin-shutdown-is-reaped-off-the-main-thread.md) | Accepted | 2026-09-21 | plugin, host-plugin, lifecycle, shutdown, main-thread, healthcheck, restart, concurrency |
| 0505 | [plugin 기동은 연결을 메인 스레드 밖에서 기다리고, 연결 전의 요청은 쌓았다가 보낸다](0505-a-plugin-start-waits-for-its-connection-off-the-main-thread.md) | Accepted | 2026-09-23 | plugin, host-plugin, lifecycle, startup, handshake, main-thread, concurrency, adr-0457 |

## plugin 렌더 채널 · webview

렌더 채널은 0028 → 0030(image) · 0041 · 0065(markdown webview) → 0067 이다. webview 플랫폼 결정(0159 · 0248 · 0250 · 0301)과 호스트 계약(0320 → 0385)이 뒤따른다.
운영 문서: [dev-guide/egui-mesh-channel](../dev-guide/egui-mesh-channel.md) · [dev-guide/linux](../dev-guide/linux.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0028 | [Plugin 이 자기 프로세스에서 egui 를 tessellate 한 mesh 를 host 가 합성하는 out-of-process 렌더 채널 도입](0028-plugin-egui-mesh-render-channel.md) | Accepted | 2026-06-29 | plugin, render-channel, egui, epaint, mesh, ipc, shared-memory, surface-kind, popup, banner, host-rendered-removal, bundled-only, adr-0008, adr-0009 |
| 0030 | [image surface 는 mesh-only(비트맵=egui 텍스처) 로 전환 — ADR-0028 의 image Canvas-하이브리드 조항 개정](0030-image-egui-mesh-bitmap-texture.md) | Accepted | 2026-07-01 | plugin, render-channel, egui, epaint, mesh, image, surface-kind, bitmap, texture, host-rendered-removal, adr-0028 |
| 0041 | [체인 단절 유실 frame 의 stale atlas 는 host 단독 "매 tick full 재무장"으로 복구한다](0041-egui-mesh-stale-frame-recovery.md) | Accepted | 2026-07-09 | plugin, render-channel, egui, epaint, mesh, texture-atlas, font, stale-frame, host-only, recovery, adr-0028, adr-0030 |
| 0065 | [markdown surface 는 EguiMesh 대신 Webview(HTML+CSS) 로 렌더한다 — ADR-0028 의 markdown B1 선례 조항 개정](0065-markdown-webview-render-channel.md) | Accepted | 2026-08-10 | plugin, render-channel, webview, html, markdown, egui-mesh, typography, mermaid, surface-kind, host-rendered-removal, adr-0028 |
| 0067 | [markdown webview 전환(ADR-0065) Stage B 구현은 mermaid 렌더링을 포함하지 않는다 — 스코프 정정](0067-markdown-webview-stage-b-scope-correction.md) | Accepted | 2026-08-10 | plugin, render-channel, webview, html, markdown, mermaid, sanitize, adr-0065, scope-correction |
| 0095 | [plugin 리스트는 `show_rows` 로 virtualize 하고, 가로 폭은 한 번 재서 고정한다](0095-plugin-list-virtualization-and-fixed-content-width.md) | Accepted | 2026-09-02 | plugin, egui-mesh, git-viewer, scroll, virtualization, performance, layout |
| 0097 | [plugin self-repaint 지연 알림은 프로세스 상주 타이머 스레드 1 개로 처리한다](0097-plugin-self-repaint-resident-timer.md) | Accepted | 2026-09-02 | plugin-sdk, egui-mesh, threading, self-repaint |
| 0102 | [webview 자식 창이 잡은 키는 host 로 포워딩한다 — 페이지 소유 범위는 `KeybindingSettings` + plugin 명령 레지스트리에서 도출한다](0102-webview-key-forwarding.md) | Accepted | 2026-09-03 | webview, keyboard, shortcuts, keybindings, focus, cross-platform, markdown, html |
| 0108 | [스크롤은 한 pass 에 전량 전달한다 — egui-mesh 는 휠 델타를 쪼개 넣고, 스크롤 애니메이션은 끈다](0108-egui-mesh-scroll-delivered-in-one-pass.md) | Accepted | 2026-09-03 | egui-mesh, plugin-sdk, scroll, self-repaint, performance, theme, animation |
| 0159 | [NULL GdkWindow 은 크래시가 아니라 값이다 — 그리고 연결이 둘이면 왕복해야 한다](0159-a-null-gdk-window-is-a-value-not-a-crash.md) | Accepted | 2026-09-05 | linux, x11, gdk, webview, ffi, crash-safety, guards, adr-0117 |
| 0248 | [webview 정리는 GDK 를 먼저 끝낸 뒤 X 창을 지운다 — 남는 경합은 에러 트랩이 값으로 받는다](0248-webview-teardown-lets-gdk-finish-before-the-x-window-is-destroyed.md) | Accepted | 2026-09-08 | linux, x11, gdk, gtk, webview, crash-safety, teardown, ordering, adr-0159 |
| 0250 | [Linux 도 원격 서브리소스를 막는다 — macOS 와 같은 규칙을 WebKit content filter 로 컴파일해서](0250-linux-blocks-remote-subresources-with-a-webkit-content-filter.md) | Accepted | 2026-09-08 | webview, linux, webkitgtk, security, remote-content, cross-platform, ffi, adr-0249 |
| 0263 | [plugin 이 그린 IME 커서 영역은 mesh frame 알림에 실려 host 로 돌아온다](0263-plugin-drawn-ime-cursor-rides-the-mesh-frame-notice.md) | Accepted | 2026-09-11 | ime, egui-mesh, plugin-protocol, typed-length, candidate-window, popup, surface |
| 0301 | [세 webview backend 는 크기를 서로 다른 수단으로 전파한다 — Linux 는 allocation 을 직접 준다](0301-three-webview-backends-propagate-size-by-different-means.md) | Accepted | 2026-09-20 | linux, x11, gtk, webkitgtk, webview, layout, cross-platform, adr-0159 |
| 0320 | [webview 백엔드 셋은 trait 이 아니라 공유 호출부가 묶는다](0320-the-webview-backends-are-held-together-by-shared-call-sites-not-a-trait.md) | Accepted | 2026-09-20 | architecture, webview, host-api, cross-platform, trait, cfg, contract |
| 0385 | [webview 백엔드는 호스트 계약을 주입받는다 — 탐색 상태는 도메인 모델, 키 정책은 콤보 목록, 키 접점은 trait](0385-webview-backends-receive-their-host-contract-by-injection.md) | Accepted | 2026-09-21 | architecture, webview, host-api, keybindings, layering, injection, cross-platform, adr-0102, adr-0320 |

## 번들 plugin 기능 — markdown · git-viewer · explorer · image · clipboard

운영 문서: [features/explorer](../features/explorer/index.md) · [features/clipboard](../features/clipboard/index.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0026 | [클립보드 히스토리 백엔드 제거 + 뷰어는 plugin 직접-read](0026-clipboard-history-removal-plugin-direct-read.md) | Accepted | 2026-06-28 | clipboard, plugin, removal, scope, sandbox, user-agent-separation, semver, breaking, adr-0009 |
| 0074 | [Explorer root 는 항상 절대경로 — 상대 경로는 채택하지 않고 홈으로 폴백한다](0074-explorer-root-always-absolute.md) | Accepted | 2026-08-18 | explorer, surface-cwd, invariant, fallback, attach, path |
| 0099 | [git-viewer 는 활성 worktree 의 repo 핸들을 하나만 들고, worktree 중복 검사는 미리 잰 정규화 경로로 한다](0099-git-viewer-repo-handle-cache-and-canonical-dedup.md) | Accepted | 2026-09-03 | plugin, git-viewer, git2, cache, invalidation, performance, worktree |
| 0245 | [image 의 편집은 임시다 — 미저장 편집은 복원하지도, 알리지도 않는다](0245-an-image-surface-edit-is-temporary-and-is-not-restored.md) | Accepted | 2026-09-08 | image, plugin, persistence, restore, snapshot, identity, non-goal, adr-0030 |
| 0249 | [markdown 의 로컬 이미지는 렌더러가 문서 안에 싣는다 — 그 자리가 읽기 범위이기도 하다](0249-markdown-local-images-are-inlined-by-the-renderer.md) | Accepted | 2026-09-08 | markdown, plugin, webview, images, sanitizer, security, scope, cross-platform, adr-0065 |
| 0289 | [markdown 문서는 `<base href>` 를 싣지 않는다 — 그것이 문서 안 앵커를 문서 밖으로 보낸다](0289-the-markdown-document-carries-no-base-href.md) | Accepted | 2026-09-19 | markdown, plugin, webview, navigation, anchors, toc, footnotes, cross-platform, adr-0249, adr-0065 |

## 파일 핸들러 · 파일 피커

0272 → 0279 → 0302(포커스 조항 개정)(탭 생성 갈래는 0502 — 「창 · 워크스페이스 · 포커스 · 수명주기」 그룹)(toast 축은 0503 — 「UI · 테마 · 디자인 토큰 · 갤러리」 그룹) → 0425 · 0426 · 0427. 파일 피커는 0042 → 0162(에이전트 표면에서 제외, 0042 대체)이다.
운영 문서: [features/file-handler](../features/file-handler/index.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0042 | [native 파일 선택 다이얼로그는 host `fs.pick_file`(FsRead)로 위임한다](0042-fs-pick-file-native-dialog-host-delegation.md) | Superseded by 0162 | 2026-07-09 | plugin, ipc, fs, native-dialog, rfd, permission, fs-read, host-delegation, markdown, focus-independence, adr-0028 |
| 0162 | [호스트를 막는 네이티브 다이얼로그는 에이전트 표면이 아니다 — `fs.pick_file` 을 뺀다](0162-a-host-blocking-native-dialog-is-not-an-agent-surface.md) | Accepted | 2026-09-05 | ipc, agent-surface, gui, blocking, rfd, portal, identity-principle, adr-0042, adr-0058, adr-0091 |
| 0272 | [URL 대상은 핸들러 picker 와 실행 계층에만 들어가고 형식 식별에는 들어가지 않는다](0272-url-targets-enter-the-handler-picker-not-identify.md) | Accepted | 2026-09-14 | file-handler, terminal-link, dispatch, url |
| 0279 | [File dispatch retains its origin through completion](0279-file-dispatch-retains-origin-through-completion.md) | Accepted | 2026-09-15 | file-handler, focus, routing, lifecycle |
| 0302 | [A user file open selects its result tab — amends the focus clause of ADR-0279](0302-a-user-file-open-selects-its-result-tab.md) | Accepted | 2026-09-20 | file-handler, focus, explorer, user-action, adr-0279 |
| 0425 | [헤드리스의 `file_handler.dispatch` 는 수락하지 않고 "이 빌드에 없다" 로 답한다](0425-headless-file-dispatch-answers-that-this-build-cannot-open-files.md) | Accepted | 2026-09-21 | ipc, headless, file-handler, agent-facing, build-combination, error-code |
| 0426 | [`file_handler.reload` 는 적용되지 않은 user 항목을 `rejected` 필드로 알린다](0426-file-handler-reload-reports-the-entries-it-dropped.md) | Accepted | 2026-09-21 | ipc, cli, file-handler, agent-facing, compatibility, settings |
| 0427 | [file handler 병합은 출처 순서(Host → Plugin → User)로 하고 user patch 를 늘 마지막에 둔다](0427-file-handler-merge-applies-user-patches-last.md) | Accepted | 2026-09-21 | file-handler, registry, plugin, settings, boot, patch-semantics |

## 에이전트 통합 · 협업

child 상태는 0072 → 0266 → 0288 → 0291(0288 대체), 완료 알림 로그는 0330 → 0344 → 0415 · 0416 이다. hook 실패 기록은 0075 → 0164(언어 조항 개정)이다. task-graph 는 0066 → 0073(0066 대체)이다.
운영 문서: [dev-guide/external-interaction/child-completion-notify-log](../dev-guide/external-interaction/child-completion-notify-log.md) · [features/child-terminal](../features/child-terminal/index.md) · [dev-guide/agent-runner](../dev-guide/agent-runner.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0066 | [task-graph 실시간 화면은 보류한다 (task runner 안정화 전까지)](0066-task-graph-view-deferred.md) | Superseded by 0073 | 2026-08-10 | agent-collaboration, task-graph, ui, scope, deferred, superseded |
| 0072 | [child terminal 상태를 hook push 캐시 단독에서 hook+관측 융합 판정으로 바꾼다](0072-child-state-hook-observation-fusion.md) | Accepted | 2026-08-18 | child-terminal, agent-collaboration, liveness, staleness, hook, observation, heuristic, self-heal, ipc |
| 0073 | [task-graph 화면 보류를 해제하고 host builtin surface + workspace popup 두 표면으로 만든다](0073-task-graph-view-unblock.md) | Accepted | 2026-08-19 | agent-collaboration, task-graph, dag, ui, surface, popup, host-builtin, egui-mesh, adr-0066 |
| 0075 | [agent hook 전달 실패를 CLI 로컬 파일에 기록하고, exit code 는 노출하지 않는다](0075-agent-hook-delivery-failure-record.md) | Accepted | 2026-08-20 | agent-hooks, observability, cli, error-handling |
| 0083 | [Stop-훅 게이트를 이름으로 등록하는 레지스트리로 일반화한다](0083-stop-gate-named-registry.md) | Accepted | 2026-08-24 | claude-plugin, registry, stop-hook, gate, session-profile, marker, cli, ipc, i18n |
| 0088 | [Stop-훅 게이트는 세션 goal 을 근거로만 자율 계속-진행을 지시한다](0088-stop-gate-goal-aware-continuation.md) | Accepted | 2026-08-30 | claude-plugin, stop-hook, gate, memory, goal, i18n, autonomy |
| 0093 | [에이전트 응답 중계는 화면이 아니라 transcript JSONL 을 읽고, at-least-once 로 전달한다](0093-agent-response-relay-reads-transcript-jsonl.md) | Accepted | 2026-09-02 | agent-stream, plugin, transcript, tail, relay, at-least-once, claude-plugin, focus-independence |
| 0100 | [agent-stream 은 자기 프로세스에서 SSE 서버를 열고, loopback 기본 · 명시 포트 · 광역 bind 시 토큰 필수로 노출을 좁힌다](0100-agent-stream-sse-endpoint-exposure.md) | Accepted | 2026-09-03 | agent-stream, plugin, sse, http, tiny-http, exposure, authentication, backpressure, resume, adr-0046, adr-0048, adr-0093 |
| 0112 | [agent-stream 의 턴 correlation 은 요청자 제공 `request_id` 로 하고, 턴 경계는 transcript 의 turn_end 를 그대로 쓴다](0112-agent-stream-turn-correlation.md) | Accepted | 2026-09-04 | agent-stream, plugin, webhook, correlation, turn, sse, inbound, adr-0046, adr-0093, adr-0100 |
| 0119 | [세마포어 한도는 원자적으로 조정하고, 홀더 만료는 opt-in 으로 둔다](0119-agent-semaphore-resize-and-holder-expiry.md) | Accepted | 2026-09-04 | agent-collaboration, semaphore, lease, concurrency, operability |
| 0164 | [hook 실패 기록의 로케일 무관성은 산문이 아니라 좌표 필드가 진다 — ADR-0075 의 언어 조항 개정](0164-hook-failure-locale-invariance-rests-on-fields.md) | Accepted | 2026-09-05 | cli, agent-hooks, diagnostics, i18n, plugin, partial-amendment, adr-0075 |
| 0262 | [Codex 승인 대기는 `PermissionRequest` 훅으로 관측하고, `idle` 전이가 대기를 내린다](0262-codex-approval-wait-is-observed-via-permission-request.md) | Accepted | 2026-09-11 | plugin, codex, agent-state, attention, hooks |
| 0265 | [자식 Claude 의 승인 정책은 호출자가 고르고, 아무도 안 고르면 사용자 자신의 설정이 남는다](0265-child-approval-policy-is-the-callers-choice.md) | Accepted | 2026-09-12 | plugin, claude, codex, permissions, cli, ipc, defaults, safety, profile |
| 0266 | [관측으로 파생된 정지(`stale`)는 조회뿐 아니라 push 알림에도 도달한다](0266-derived-stale-must-reach-the-push-channel.md) | Accepted | 2026-09-12 | plugin, claude, child-terminal, agent-state, hooks, notification, stall, adr-0072 |
| 0288 | [Codex 부모에게 child 상태를 App Server 도구 결과로 전달한다 — ADR-0266의 Codex 부모 push 주체 개정](0288-codex-parent-tool-output-completion.md) | Superseded by 0291 | 2026-09-16 | codex, app-server, completion, outbox, lifecycle, plugin |
| 0291 | [Codex 부모의 App Server 완료 전달을 제거한다 — 완료 채널을 로그 하나로 되돌린다](0291-remove-the-codex-app-server-completion-channel.md) | Accepted | 2026-09-20 | codex, app-server, completion, outbox, removal, lifecycle, plugin |
| 0330 | [완료 알림 한 줄은 한 번의 write 다](0330-one-completion-line-is-one-write.md) | Accepted | 2026-09-20 | notify, concurrency, plugin, logging |
| 0344 | [완료 알림 로그는 호스트 세대 하나를 들고, 버린 양을 말한다](0344-the-completion-log-keeps-one-host-generation-and-says-what-it-threw-away.md) | Accepted | 2026-09-20 | notify, retention, logging, plugin, instance-identity, adr-0330 |
| 0415 | [재개하는 완료 로그 reader 는 옆 메타 파일에서 잃은 양을 안다](0415-a-resuming-completion-log-reader-learns-what-it-lost-from-a-sidecar.md) | Accepted | 2026-09-21 | notify, retention, reader-recovery, offset, plugin, compatibility, adr-0330, adr-0344 |
| 0416 | [부팅 청소는 포트 파일과 같은 뿌리일 때만 돈다 — ADR-0344 의 "안 고친 것" 해소](0416-the-boot-cleanup-follows-the-port-file-root.md) | Accepted | 2026-09-21 | notify, retention, instance-identity, port-file, boot, adr-0344 |
| 0610 | [에이전트 primitive 의 이름은 memory 키가 되기 전에 호출자 값 기준으로 판정한다](0610-an-agent-primitive-name-is-judged-before-it-becomes-a-memory-key.md) | Accepted | 2026-09-23 | agent, ipc, error-code, memory, key, semaphore, barrier, rate-limit, task |

## 저장소 · 메모리 DB

pragma 보고는 0316 → 0376(보고 채널 개정), 못 연 `memory.db` 의 in-memory 대체는 0485 → 0611(쓰기 응답 조항의 적용 범위를 `memory.*` 밖 이름공간까지 개정)이다.
운영 문서: [design/systems/storage](../design/systems/storage.md) · [design/systems/memory](../design/systems/memory.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0085 | [IPC 관측 로그는 무한 보존하지 않는다 — 상한을 한 곳에서 관리하고, audit 은 deny 만 남긴다](0085-ipc-log-retention-bounded.md) | Accepted | 2026-08-25 | audit, telemetry, memory-db, retention, observability, cpu |
| 0260 | [튜토리얼 진행은 사용자 상태 DB에 주제별로 저장한다](0260-tutorial-progress-belongs-to-user-state.md) | Accepted | 2026-09-09 | tutorial, persistence, concurrency |
| 0275 | [최근 목록 캐시는 state.db 수명에 귀속한다](0275-recent-cache-belongs-to-the-state-database.md) | Accepted | 2026-09-15 | storage, recent-files, focus, multi-window |
| 0316 | [DB 는 요청한 pragma 가 아니라 **적용된** pragma 를 보고한다](0316-a-database-reports-the-pragma-that-took-not-the-one-requested.md) | Accepted | 2026-09-20 | sqlite, storage, memory, pragma, observability, wal |
| 0335 | [`state.db` 는 GUI 부팅만 열고, 접근자의 `None` 은 뜻이 하나다](0335-the-state-database-is-opened-by-gui-boot-alone.md) | Accepted | 2026-09-20 | storage, headless, ownership, naming |
| 0376 | [요청한 pragma 가 안 선 DB 는 치명이 아니라 degraded 이고, 그 상태는 값으로 밖에 나간다 — ADR-0316 의 보고 채널 조항 개정](0376-a-database-that-opened-with-pragmas-that-did-not-take-is-degraded-not-fatal.md) | Accepted | 2026-09-21 | sqlite, storage, memory, pragma, observability, ipc, cli, adr-0316 |
| 0377 | [실패한 메모리 쓰기는 초기화와 같은 표로 원인을 말하고, 메모리 쪽 상태는 실패 전 그대로다](0377-a-failed-memory-write-names-its-cause-with-the-same-table-as-init.md) | Accepted | 2026-09-21 | sqlite, storage, memory, error-handling, ipc, compatibility |
| 0378 | [memory store 락의 poison 보고 좌표는 store 의 port 에 둔다](0378-the-poison-coordinate-of-the-memory-store-lives-at-its-port.md) | Accepted | 2026-09-21 | memory, storage, poison, boundary, output-observer |
| 0485 | [못 연 `memory.db` 는 in-memory 대체로 계속 뜨되, 그 사실을 진단과 쓰기 응답이 말한다](0485-a-memory-db-that-failed-to-open-falls-back-in-memory-and-says-so.md) | Accepted | 2026-09-22 | sqlite, storage, memory, degraded, durability, ipc, cli, fallback |
| 0611 | [`memory.db` 에 쓰는 IPC 는 이름공간과 무관하게 durable 이 아님을 말한다 — ADR-0485 의 적용 범위 조항 개정](0611-every-ipc-write-to-memory-db-says-when-it-is-not-durable.md) | Accepted | 2026-09-23 | sqlite, storage, memory, degraded, durability, ipc, agent, approval, telemetry, session |

## 아키텍처 · 헤드리스 · 크레이트 경계

구조 op 는 0337 → 0395(결정 2 개정) → 0440, headless Intent 는 0111 → 0346 이다.
운영 문서: [architecture](../architecture/index.md) · [dev-guide/headless-build-boundaries](../dev-guide/headless-build-boundaries.md) · [dev-guide/app-state-ownership](../dev-guide/app-state-ownership.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0089 | [크레이트 분리 기준은 줄 수보다 의존 방향이 우선한다](0089-crate-split-follows-dependency-direction.md) | Accepted | 2026-08-30 | build, crate-layout, dependency-direction, remote-attach |
| 0111 | [headless 는 Intent 큐를 스스로 drain 해 engine 에 적용한다](0111-headless-drains-the-intent-queue.md) | Accepted | 2026-09-04 | headless, intent, dispatch, cascade, ipc, queue, agent-surface |
| 0134 | [headless 는 host event 큐를 비우되 비-bus 소비자가 있는 종류만 적용한다](0134-headless-drains-host-events-but-applies-only-hook-fired.md) | Accepted | 2026-09-04 | headless, host-event, plugin-event-bus, agent-runner, hooks, queue, agent-surface |
| 0136 | [조회는 자기가 관측하는 것을 만들지 않는다 — headless plugin 조회 표면](0136-a-query-does-not-create-what-it-observes.md) | Accepted | 2026-09-05 | headless, plugin, ipc, identity-principle-2, agent-surface, observability |
| 0143 | [헤드리스도 지목한 대상을 확인한다 — 예약 prefix 에 한정해 engine handler 앞에서](0143-a-named-target-is-checked-before-the-engine-in-headless.md) | Accepted | 2026-09-05 | headless, ipc, routing, plugin-namespace, identity-principle-3, adr-0140, adr-0136 |
| 0253 | [수명주기 토글 둘은 `App` 이분을 기다리지 않는다 — cascade 전체가 아니라 그 둘이 내는 이벤트만 헤드리스 형태로 대체한다](0253-the-lifecycle-toggles-do-not-wait-for-the-app-split.md) | Accepted | 2026-09-09 | ipc, headless, plugin, lifecycle, routing, agent-surface, adr-0127, adr-0173 |
| 0308 | [형식 레지스트리의 port impl 은 타입을 소유한 크레이트에 남고, 그 의존이 layer 예외다](0308-the-format-registry-port-impl-stays-with-the-type.md) | Accepted | 2026-09-20 | architecture, crates, layering, plugin-protocol, orphan-rule, file-format, guards, adr-0089 |
| 0318 | [핸들러 레지스트리는 크레이트로 내려가고, 번들 기본값은 상대 경로가 아니라 크레이트 상수가 된다](0318-bundled-handler-defaults-become-a-crate-constant.md) | Accepted | 2026-09-20 | architecture, crates, layering, file-handler, include-str, plugin-protocol, orphan-rule, adr-0308 |
| 0319 | [선택 모델은 낡은 gui 게이트를 크레이트 feature 로 옮기지 않고 버린다](0319-the-selection-model-drops-the-stale-gui-gate-instead-of-carrying-it.md) | Accepted | 2026-09-20 | architecture, crates, layering, selection, cell-width, headless, feature-gate, adr-0308 |
| 0324 | [링크는 검출과 여는 것을 부수효과로 가른다](0324-link-detection-and-link-opening-split-by-side-effect.md) | Accepted | 2026-09-20 | architecture, crates, layering, terminal-link, headless, feature-gate, side-effect, adr-0319 |
| 0325 | [루트 패키지를 lib 와 bin 으로 가른다 — 경계를 만드는 것이 아니라 잴 좌변을 만드는 것이다](0325-the-root-package-splits-into-a-lib-and-a-bin.md) | Accepted | 2026-09-20 | architecture, cargo, targets, testing, public-api, headless |
| 0326 | [egui 는 링크하는 크레이트가 켜는 것이지 기본값으로 따라오는 것이 아니다](0326-egui-is-opt-in-for-the-crates-that-link-it.md) | Accepted | 2026-09-20 | build, cargo, features, headless, type-appearance, dependency-graph |
| 0331 | [platform 은 OS 호출을 들고, 그 신호가 App 에서 무엇이 되는지는 안 정한다](0331-the-platform-folder-holds-the-os-call-not-the-app-meaning.md) | Accepted | 2026-09-20 | architecture, platform, layering, app-event, callback, windows, macos, cross-platform |
| 0336 | [`tasty-font` 은 device 경계에서 갈린다 — wgpu 는 `gpu` feature 뒤로](0336-the-font-crate-splits-at-the-device-boundary.md) | Accepted | 2026-09-20 | build, cargo, features, headless, font, dependency-graph, wgpu, adr-0326 |
| 0337 | [구조 op 실행은 도메인 값으로 답하고, 자원 회수는 cascade 한 자리가 소유한다](0337-structural-execution-answers-with-domain-values.md) | Accepted | 2026-09-20 | architecture, boundary, close, attach, cascade, resource-reclamation, hexagonal |
| 0342 | [셀 렌더러는 크레이트를 직접 부르고, 본체 재수출을 거치지 않는다](0342-the-cell-renderer-names-the-crates-not-the-host-re-exports.md) | Accepted | 2026-09-20 | architecture, layering, renderer, selection, terminal-link, naming, headless, adr-0319, adr-0324 |
| 0343 | [OS 경계는 폴더가 아니라 크레이트다 — 본체는 별칭으로 부른다](0343-the-os-boundary-is-a-crate.md) | Accepted | 2026-09-20 | architecture, platform, crate-split, layering, features, headless, cross-platform, adr-0331 |
| 0346 | [headless 는 자기가 닿는 정의만 컴파일한다 — ADR-0111 의 non-Domain 보존 조항 개정](0346-headless-compiles-only-what-it-reaches.md) | Accepted | 2026-09-21 | headless, intent, feature, dead-code, adr-0111 |
| 0350 | [스트림 허브는 IPC 크레이트에 산다 — core 는 adapter 를 거치지 않는다](0350-the-stream-hub-lives-in-the-ipc-crate.md) | Accepted | 2026-09-21 | architecture, layering, ipc, attach, stream, crate-split, hexagonal |
| 0355 | [AppState 의 소유권은 둘째 struct 가 아니라 gui 경계로 가른다](0355-app-state-ownership-is-split-by-the-gui-boundary-not-by-a-second-struct.md) | Accepted | 2026-09-21 | headless, app-state, ownership, popup, feature, adr-0346 |
| 0381 | [셀 렌더러의 잎 크레이트 셋은 dev 에서도 최적화한다](0381-the-cell-renderer-leaf-crates-are-optimized-in-dev.md) | Accepted | 2026-09-21 | build, dev-profile, opt-level, renderer, selection, terminal-link, cell-width, measurement, adr-0342 |
| 0395 | [구조 변경 실행과 그 cascade 는 도메인 계층에 산다 — ADR-0337 의 결정 2 와 핸들러 재사용 조항 개정](0395-structural-execution-and-its-cascades-live-in-the-domain-layer.md) | Accepted | 2026-09-21 | architecture, boundary, hexagonal, attach, cascade, close, headless, adr-0337 |
| 0440 | [도메인 경계는 크레이트가 아니라 모듈 경계와 가드로 세운다](0440-the-domain-boundary-is-a-module-boundary-with-a-guard-not-a-crate.md) | Accepted | 2026-09-21 | headless, domain, core, layering, crate-split, ports, app-state, guard, adr-0355, adr-0346 |
| 0470 | [IPC 핸들러는 창 상태를 읽을 때만 `AppState` 를 받는다](0470-an-ipc-handler-takes-window-state-only-when-it-reads-it.md) | Accepted | 2026-09-22 | ipc, handler, app-state, ownership, headless, signature, adr-0355, adr-0440 |
| 0471 | [IPC 엔진 핸들러는 창에 포트와 intent 출구로만 닿는다](0471-ipc-engine-handlers-reach-the-window-through-a-port.md) | Accepted | 2026-09-22 | ipc, handler, app-state, ownership, port, intent, focus, headless, adr-0355, adr-0470 |
| 0490 | [경계 가드의 세 빈자리를 판정기를 넓혀 닫는다 — 변이로 찾은 것](0490-boundary-guards-close-three-holes-found-by-mutation.md) | Accepted | 2026-09-22 | guard, domain, layering, mutation, gui, focus, automation, webhook, hook-handler, adr-0440 |

## CLI · 로깅 · 에이전트 표면

파이프 조기 종료는 0101(stdout, 종료 코드 0) → 0513(stderr, 종료 코드 유지) — 결론이 반대라 합치지 않는다. 호스트 오류 출력은 0512.
운영 문서: [dev-guide/cli-ipc-surface](../dev-guide/cli-ipc-surface.md) · [dev-guide/error-handling](../dev-guide/error-handling.md) · [dev-guide/cli-structure](../dev-guide/cli-structure.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0092 | [공유 로그 파일은 host 프로세스만 연다 — CLI 클라이언트는 stderr 전용](0092-file-log-host-process-only.md) | Accepted | 2026-08-30 | logging, tracing, diagnostics, cli, boot, crash-report |
| 0101 | [CLI 클라이언트의 stdout 파이프 조기 종료(EPIPE)는 종료 코드 0 으로 조용히 끝낸다 — SIGPIPE 복원은 채택하지 않는다](0101-cli-stdout-broken-pipe-exit-zero.md) | Accepted | 2026-09-03 | cli, stdout, epipe, sigpipe, exit-code, crash-report, cross-platform, error-handling |
| 0160 | [IPC 메서드는 CLI 로 닿거나, 왜 못 닿는지의 근거를 든다](0160-every-ipc-method-is-cli-reachable-or-carries-a-reason.md) | Accepted | 2026-09-05 | ipc, cli, agent-surface, guard, identity-principle |
| 0512 | [CLI 는 IPC 오류의 `error.data` 를 stderr 둘째 줄에 원형 그대로 싣는다](0512-the-cli-relays-ipc-error-data-on-a-second-stderr-line.md) | Accepted | 2026-09-23 | cli, ipc, error-message, parity, wire-format, stderr |
| 0513 | [CLI 의 stderr 쓰기 실패는 버리고 명령의 종료 코드를 그대로 둔다](0513-cli-stderr-broken-pipe-keeps-the-exit-code.md) | Accepted | 2026-09-23 | cli, stderr, epipe, exit-code, crash-report, error-handling, adr-0101 |
| 0514 | [`tasty new workspace` 는 창을 서피스로 지목하고, 생략값을 `TASTY_SURFACE_ID` 로 채우지 않는다](0514-new-workspace-names-its-window-by-a-surface-and-keeps-no-env-default.md) | Accepted | 2026-09-23 | cli, ipc, workspace, window, multi-window, focus, parity, routing |

## 빌드 · 배포 · 버전

고지 세트는 0317 → 0370(이행 순서 개정), plugin 버전 게이트는 0137 → 0166 이다.
운영 문서: [dev-guide/release](../dev-guide/release.md) · [dev-guide/dist-build](../dev-guide/dist-build.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0021 | [자체 업데이트 확인 기능(update-check) 전면 제거](0021-remove-update-check-feature.md) | Accepted | 2026-06-25 | update, auto-update, scope, distribution, removal, maintenance, cli, plugin |
| 0051 | [release plugin 서명키를 영구 신뢰 루트가 아닌 매 빌드 로컬 자동생성으로 전환](0051-ephemeral-release-signing-key.md) | Accepted | 2026-07-14 | plugin-signing, release-ci, security, ed25519, trust-store, self-hosted-runner |
| 0137 | [plugin 패치 bump 의무는 파일 수 문턱이 아니라 정규화된 내용으로 판정하고, 자동 채널을 세운다](0137-plugin-version-bump-is-judged-by-content-not-file-count.md) | Accepted | 2026-09-05 | plugin, versioning, ci-gates, guard, measurement, adr-0037 |
| 0166 | [plugin 버전 게이트는 디렉토리가 아니라 산출물을 판정한다](0166-the-plugin-version-gate-judges-the-artifact-not-the-directory.md) | Accepted | 2026-09-05 | plugin, versioning, guards, ci-gates, measurement, adr-0137, adr-0165, adr-0138 |
| 0317 | [고지 세트는 생성하지 않고 저장소의 고지 파일을 산출물마다 스테이징한다](0317-the-notice-set-is-staged-not-generated.md) | Accepted | 2026-09-20 | release, packaging, licensing, third-party, linux, appimage, deb, rpm, ci |
| 0370 | [macOS·Windows 산출물도 관측 전에 고지 세트를 배선한다 — ADR-0317 의 이행 순서 조항 개정](0370-macos-and-windows-artifacts-carry-the-notice-set-before-it-is-observed.md) | Accepted | 2026-09-21 | release, packaging, licensing, third-party, macos, dmg, windows, msi, zip, wix, adr-0317 |

## 테스트 · flake · 하네스

flake 처방은 0129 → 0155, e2e 하네스는 0090 → 0127 → 0170 → 0297 이다.
운영 문서: [dev-guide/e2e-tests](../dev-guide/e2e-tests.md) · [dev-guide/unit-test-isolation](../dev-guide/unit-test-isolation.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0090 | [e2e 테스트 격리 단위는 프로세스가 아니라 workspace 다](0090-test-isolation-by-workspace-not-process.md) | Accepted | 2026-08-30 | testing, e2e, harness, isolation, workspace, attach, ci |
| 0096 | [유닛 테스트는 사용자 환경을 읽지 않는다 — 설정은 주입, env 는 RAII 복원](0096-unit-tests-isolated-from-user-environment.md) | Accepted | 2026-09-02 | testing, isolation, settings, env, harness, ci, regression-detection |
| 0127 | [e2e 하네스가 띄울 바이너리는 한 곳에서 정한다 — 기본 조합의 GPU 종속은 `App` 이분이 선행이다](0127-e2e-harness-binary-selection.md) | Accepted | 2026-09-04 | testing, e2e, headless, gpu, feature-flags, cargo, harness, adr-0090 |
| 0129 | [확률적 테스트 실패(flake)의 부류별 표준 처방](0129-flaky-test-classes-and-standard-fixes.md) | Accepted | 2026-09-04 | testing, flaky-tests, ci, concurrency, test-isolation, guards, cfg-feature-gate, source-scan-guard, adr-0128 |
| 0155 | [전역 상태 경합 flake 의 두 갈래와 처방을 "인자화됐는가" 에 건다](0155-global-state-race-prescription-by-parameterization.md) | Accepted | 2026-09-05 | testing, flaky-tests, concurrency, test-isolation, env, adr-0129 |
| 0170 | [e2e 데몬은 스위트 단위로 고르고, 배선은 만들되 기본으로 켜지 않는다](0170-e2e-daemon-is-chosen-per-suite-and-off-by-default.md) | Accepted | 2026-09-05 | testing, e2e, harness, gpu, headless, build |
| 0181 | [지연 단정은 부하에는 반응하고 코드에는 반응하지 않는 대조군을 함께 싣는다](0181-a-latency-assertion-must-carry-a-control-that-load-moves-and-code-does-not.md) | Accepted | 2026-09-06 | testing, flake, harness, assertions, diagnostics, adr-0129, adr-0139 |
| 0211 | [PTY 종료 대기의 상한은 경주 예산이 아니라 안전망이다 — 실패문이 두 사건을 스스로 가른다](0211-the-pty-exit-budget-is-a-safety-net-not-a-race-budget.md) | Accepted | 2026-09-09 | testing, flaky, pty, ci, macos, diagnostics, measurement, adr-0129, adr-0181, adr-0217, adr-0206 |
| 0217 | [검증의 전제는 산문이 아니라 실패 문구에 산다](0217-a-precondition-lives-in-the-failure-text-not-in-prose.md) | Accepted | 2026-09-08 | testing, e2e, diagnostics, failure-messages, preconditions, guards, measurement, adr-0139, adr-0142, adr-0206, adr-0211 |
| 0274 | [하네스 락은 선언 위치가 아니라 획득 형태를 검사한다](0274-harness-locks-are-checked-at-acquisition.md) | Accepted | 2026-09-15 | tests, mutex, poison, acquisition, scope |
| 0284 | [Self-attach 거절은 RTT가 아니라 connector 진입 사건으로 판정한다](0284-self-attach-is-judged-by-connector-entry.md) | Accepted | 2026-09-15 | testing, attach, flake, diagnostics, connector, adr-0181 |
| 0297 | [e2e 하네스는 디스플레이를 격리하지 않고 이름을 요구한다](0297-the-e2e-harness-names-a-display-instead-of-isolating-it.md) | Accepted | 2026-09-20 | testing, e2e, harness, isolation, linux, adr-0127, adr-0090 |
| 0511 | [debug 스위치 아래에서 OS 열기는 띄우지 않고 기록한다](0511-os-open-is-recorded-not-launched-under-a-debug-switch.md) | Accepted | 2026-09-23 | debug, verification, self-verification, e2e, isolation, os-open, browser, user-agent-separation, identity |

## CI 게이트 · 복잡도

파일 SLOC 게이트는 0037 → 0131(트리거) → 0165(출하 줄) → 0168(임계) → 0205(총합 저울) → 0258(계측 사본) → 0345(시험 전체 파일)이다. 각각 다른 결정이라 합치지 않는다.
운영 문서: [dev-guide/complexity-gate](../dev-guide/complexity-gate.md) · [dev-guide/ci-gates](../dev-guide/ci-gates.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0037 | [복잡도 게이트 — clippy cognitive(deny) + tokei 파일 SLOC, baseline 은 위치 단위 동결](0037-complexity-gate.md) | Accepted | 2026-07-06 | lint, complexity, ci, quality-gate, clippy, cognitive-complexity, tokei, file-size, maintainability, clippy-policy, ratchet |
| 0131 | [파일 SLOC 게이트는 발화하는 트리거를 갖는다 — 채널 없이 쌓인 26 건은 부채로 동결하고 래칫으로 갚는다](0131-file-sloc-gate-needs-a-firing-trigger.md) | Accepted | 2026-09-04 | ci, quality-gate, complexity, tokei, file-size, ratchet, trigger, drift, adr-0037 |
| 0142 | [채널 주장은 작업 트리 기준으로 쓴다 — 원격 층은 오프라인으로 못 고정한다](0142-channel-claims-are-written-against-the-working-tree.md) | Accepted | 2026-09-05 | ci, docs, guards, remote, push, observability, adr-0133, adr-0138, adr-0139 |
| 0165 | [파일 SLOC 게이트는 출하되는 줄을 잰다 — 파일 전체가 아니라](0165-the-file-sloc-gate-measures-shipped-lines.md) | Accepted | 2026-09-05 | complexity-gate, guards, measurement, adr-0037, adr-0131, adr-0150 |
| 0168 | [파일 SLOC 임계 1000 은 유도되지 않는다 — 유지의 근거는 발화율 곡선이고, 동결은 한 방향만 잠겨 있다](0168-the-file-sloc-threshold-is-not-derived-and-the-freeze-ratchets-one-way.md) | Accepted | 2026-09-05 | complexity, quality-gate, file-size, tokei, ratchet, threshold, measurement, complexity-gate, adr-0037, adr-0131, adr-0165 |
| 0188 | [빨강은 재실행 대조가 답하기 전까지 귀속하지 않는다](0188-a-red-is-not-attributed-until-a-rerun-control-answers.md) | Accepted | 2026-09-06 | ci, measurement, control, attribution, flaky, rerun, bias, wall-clock, adr-0183, adr-0139 |
| 0192 | [레포 전체를 세는 래칫은 lane 단위로 예측되지 않는다 — 판정은 병합 트리에서 한다](0192-a-repo-wide-ratchet-is-judged-at-the-merge-tree-not-per-lane.md) | Accepted | 2026-09-07 | ci, guards, ratchet, merge-tree, per-lane, split-landing, cap, adr-0139, adr-0137, adr-0183 |
| 0205 | [동결 총합은 저울 하나로 둔다 — 항목 수준으로 가르면 안 보는 구간이 커진다](0205-the-frozen-sum-stays-one-scale.md) | Accepted | 2026-09-08 | complexity-gate, ratchet, frozen-sum, measurement, granularity, adr-0168, adr-0139 |
| 0230 | [CI 판독은 세 층이고, 각 층은 자기 위층의 결론에 안 나온다](0230-ci-readout-has-three-layers-and-each-hides-in-the-one-above.md) | Accepted | 2026-09-08 | ci, readout, coverage, unmeasured, workflow, steps, guard, adr-0142, adr-0139 |
| 0258 | [계측용 사본은 계측기가 읽을 수 있는 형태로 넘긴다 — 그리고 그 교정이 여는 예산 하향 갈래](0258-the-measured-copy-is-neutralized-for-the-counter.md) | Accepted | 2026-09-09 | complexity-gate, sloc, tokei, measurement, judge-vs-counter, ratchet, budget, adr-0165, adr-0168, adr-0205 |
| 0295 | [셸 자산은 warning 이상에서 잔여 0 으로 판정한다 — 그 아래는 안 센다](0295-shell-assets-are-judged-at-warning-and-above.md) | Accepted | 2026-09-20 | shell, gates, ci, git-hooks, shellcheck, ratchet, adr-0183 |
| 0345 | [파일 SLOC 게이트는 파일 전체가 시험인 것도 지운다 — 출하 줄이라는 이름의 근거가 그것이다](0345-the-file-sloc-gate-erases-whole-test-only-files.md) | Accepted | 2026-09-21 | complexity, quality-gate, file-size, tokei, shipping-scope, measurement, complexity-gate, adr-0166, adr-0168 |

## 가드 설계 · 측정 규율

집행 등급은 0186 → 0190(한 체계의 두 축), 하한 선언은 0224 → 0225 · 0226(사실 · 재현 불가 · 여유)이다. 둘 다 합치지 않는다.
운영 문서: [dev-guide/guard-population](../dev-guide/guard-population.md) · [dev-guide/guard-verification](../dev-guide/guard-verification.md) · [dev-guide/self-verification](../dev-guide/self-verification.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0123 | [계층 가드는 `#[cfg(test)]` 전용 모듈을 범위 밖으로 둔다](0123-layering-guard-excludes-cfg-test-modules.md) | Accepted | 2026-09-05 | layering, guards, testing, tasty-cli, adr-0105 |
| 0133 | [소스 스캔 가드의 모수는 열거하지 말고 고정한다](0133-guard-scan-population-is-pinned-not-enumerated.md) | Accepted | 2026-09-04 | guards, design-tokens, testing, ci, adr-0126, adr-0128, adr-0033 |
| 0138 | [문서를 읽는 가드는 의존 0 크레이트에 산다 — 잡이 싸야 경로 필터를 뗄 수 있다](0138-doc-guards-live-in-a-dependency-free-crate.md) | Accepted | 2026-09-05 | ci, guards, docs, workspace, paths-ignore, build-cost, adr-0133 |
| 0144 | [면제는 모호하면 성립하지 않는다 — 한 서술이 자동 실행과 부재를 함께 말하면 모순이다](0144-an-exemption-must-be-unambiguous-to-hold.md) | Accepted | 2026-09-05 | guards, ci-gates, docs, false-negative, mutation-testing, adr-0139, adr-0142 |
| 0146 | [스캔 가드는 빌드 디렉토리를 표식으로 가지치기한다 — 이름은 성질이 아니다](0146-build-dirs-are-pruned-by-their-tag-not-by-their-name.md) | Accepted | 2026-09-05 | guards, scan-population, build-artifacts, measurement, adr-0133, adr-0138, adr-0139 |
| 0147 | [같은 집합이 여러 곳에 적혀 있으면 합치지 않고 판정기로 잇는다](0147-multiple-carriers-are-joined-by-a-check-not-merged.md) | Accepted | 2026-09-05 | docs, guards, single-source-of-truth, drift |
| 0149 | [부류를 부른 것도 지목이다 — 이름만 세는 추출기는 그 서술을 아무 축도 판정하지 않는다](0149-a-class-citation-is-a-citation.md) | Accepted | 2026-09-05 | guards, ci-gates, docs, false-negative, census, adr-0139, adr-0142, adr-0144 |
| 0150 | [차집합이 0 인 면제는 죽은 것이 아니다 — 죽음과 잠복은 사유가 가른다](0150-a-zero-difference-exemption-is-not-dead.md) | Accepted | 2026-09-05 | guards, exemptions, measurement, adr-0133, adr-0146 |
| 0177 | [poison 복구가 금지되는 락은 이름이 아니라 프레임 경계 타입으로 판정한다](0177-recovery-forbidden-locks-are-judged-by-frame-boundary-type.md) | Accepted | 2026-09-05 | poison, locks, error-handling, guards, measurement, false-negative, adr-0129, adr-0155 |
| 0180 | [소스 스캔 가드의 세 판정 물음에 이름과 집을 준다 — "출하되는가" 의 정본은 `shipping_scope` 다](0180-test-only-files-is-the-canonical-shipping-judge.md) | Accepted | 2026-09-05 | guards, shipping-scope, cfg-predicate, test-gate, canonical-judge, layering, cargo-layout, scan-target, adr-0129, adr-0165, adr-0166 |
| 0183 | [가드·측정의 초록은 증거가 아니다 — 대조가 있어야 측정이다](0183-a-green-check-is-not-evidence-without-a-control.md) | Accepted | 2026-09-06 | guards, measurement, control, testing, mutation, false-green, adr-0129, adr-0139, adr-0180, adr-0181 |
| 0184 | [새것에 조립은 따라오고 판정은 따라오지 않는다](0184-assembly-follows-a-new-artifact-but-judgment-does-not.md) | Accepted | 2026-09-06 | guards, ci, new-artifacts, scan-population, mirror, pre-commit, shebang, move, adr-0138, adr-0180, adr-0183 |
| 0186 | [불가침 원칙에도 집행 등급이 있다 — 구두 원칙은 집행이 0이다](0186-an-inviolable-principle-needs-an-enforcer.md) | Accepted | 2026-09-06 | guards, principles, enforcement, gallery, gallery-first, adr-0510, adr-0180, adr-0183, adr-0184, adr-0185, adr-0190 |
| 0187 | [줄어든 수는 잃은 것이 아니다 — 인구조사의 감소는 모수를 넓혀 다시 잰다](0187-a-decreased-census-is-not-a-loss-remeasure-by-widening-the-population.md) | Accepted | 2026-09-06 | measurement, census, population, attribution, migration, adr-0139, adr-0183 |
| 0189 | [목록의 원소는 조각으로 박는다 — 문서가 등급을 매긴 판단은 그것을 지키는 것이 있어야 주장이 된다](0189-a-list-element-is-pinned-by-a-literal-snippet.md) | Accepted | 2026-09-06 | guards, mutation-testing, census, positive-control, adr-0139, adr-0183 |
| 0190 | [집행 등급은 채널 존재와 효과 둘로 갈린다 — 가드가 있다는 것이 막는다는 뜻은 아니다](0190-a-guard-that-exists-is-not-one-that-blocks.md) | Accepted | 2026-09-06 | guards, principles, enforcement, effect, mirror, polarity, blockage, vocabulary, serialization, modulus, adr-0186, adr-0184, adr-0185, adr-0180, adr-0183 |
| 0206 | [거절은 종료 코드가 아니라 자기 문구로 구별된다 — 그 축을 기계가 세고 래칫으로 든다](0206-a-refusal-is-told-apart-by-its-own-words.md) | Accepted | 2026-09-08 | guards, ci-gates, mutation-testing, exit-code, ratchet, census, adr-0139, adr-0205, adr-0217, adr-0211 |
| 0224 | [하한 선언은 사실과 판단을 함께 담는다 — 사실은 모수마다 하나, 판단은 소비자마다](0224-a-floor-declares-a-fact-and-a-judgement.md) | Accepted | 2026-09-08 | guards, floored-walk, measurement, staleness, single-source, census, adr-0139, adr-0180 |
| 0225 | [커밋에서 재현 안 되는 좌변은 값으로 덮지 않고 그 사실을 적는다](0225-a-left-side-that-a-commit-cannot-reproduce-is-described-not-pinned.md) | Accepted | 2026-09-08 | guards, floored-walk, measurement, reproducibility, working-tree, adr-0139, adr-0142, adr-0224 |
| 0226 | [하한의 여유는 폭이 아니라 「움직임의 단위 × 몇 번」으로 정한다](0226-a-floors-gap-is-derived-from-a-unit-of-motion.md) | Accepted | 2026-09-08 | guards, floored-walk, measurement, justification, ratchet, census, adr-0139, adr-0224, adr-0225 |
| 0242 | [아무도 안 읽는 문서의 수는 판정 대상이 아니다 — 셀 수는 있고 시킬 것이 없다](0242-a-number-nobody-reads-is-not-a-guard-target.md) | Accepted | 2026-09-08 | guards, docs, census, measurement, decidability, false-prescription, adr-0139 |
| 0243 | [「안 짓는다」는 좌변부터 묻지 않는다 — 좌변은 마지막 물음이다](0243-not-building-a-judge-has-three-reasons-and-the-left-side-is-the-last-one.md) | Accepted | 2026-09-08 | guards, judgement, prescription, channels, left-side, discipline, census, positive-control, adr-0139, adr-0242 |
| 0270 | [좌변이 그 사실을 재는가에는 자동 채널을 안 붙인다 — 기계가 읽는 두 모양만 가드가 본다](0270-whether-a-left-side-measures-the-fact-has-no-automatic-channel.md) | Accepted | 2026-09-14 | docs, guards, channels, mutation-testing, false-positive, observability, adr-0142, adr-0151, adr-0220 |

## 문서 · ADR 규약

재검토 조건은 0220 → 0244(표기 규격 — 운영 규칙은 template), 앵커 판정은 0201 → 0247(0201 대체)이다. 중복 기록은 0506(처리) · 0507(작성 전 탐색) · 0508(착지 대조)가 3 층을 이룬다.
운영 문서: [documentation-model](../documentation-model.md) · [adr/template](template.md) · [dev-guide/adr-landing](../dev-guide/adr-landing.md)

| # | Title | Status | Date | Tags |
|---|-------|--------|------|------|
| 0006 | [문서 분류체계 — 동작 우선(behavior-first), 화면 종속](0006-docs-taxonomy-behavior-first.md) | Accepted | 2026-06-16 | docs, taxonomy, headless, screen-spec, design-system, behavior-first |
| 0105 | [추적되는 파일에는 git 에 없는 경로를 적지 않는다 — 로컬 작업 폴더의 위치는 로컬 지침이 정한다](0105-no-nongit-path-refs-in-tracked-sources.md) | Accepted | 2026-09-04 | docs, conventions, hygiene, dead-reference, gitignore, local-workspace, guard-test, adr-0096 |
| 0139 | [문서에 적는 수는 계보로 분류하고, 빨리 낡는 수는 세 형태 중 하나로 바꾼다](0139-numbers-in-docs-are-classified-by-lineage-not-by-name.md) | Accepted | 2026-09-05 | documentation, guards, staleness, measurement, ci-gates, adr-0037, adr-0131, adr-0138 |
| 0151 | [문서의 좌표 인용은 리터럴로만 판정하고, 오탐은 예외 목록이 아니라 인용 형태를 고쳐 없앤다](0151-cited-coordinates-are-judged-as-literals-not-by-context.md) | Accepted | 2026-09-05 | documentation, guards, citation, false-positive, allowlist, detector-design, adr-0105, adr-0133, adr-0138, adr-0139 |
| 0185 | [적을 수 없는 값은 재는 법으로 적는다 — 시제가 값과 명령을 가른다](0185-an-unwritable-value-is-written-as-the-way-to-measure-it.md) | Accepted | 2026-09-06 | docs, guards, measurement, freshness, declaration, tense, adr-0139, adr-0180, adr-0183, adr-0184 |
| 0194 | [코드 인용은 줄 번호가 아니라 심볼 이름으로 한다](0194-code-citations-name-symbols-not-line-numbers.md) | Accepted | 2026-09-07 | documentation, adr-conventions, citations, guards, ratchet, adr-0139, adr-0183, adr-0190, adr-0105 |
| 0201 | [슬러그 규칙은 그것을 렌더하는 트리가 정한다 — 통일하지 않는다](0201-slug-rules-are-scoped-by-the-tree-that-renders-them.md) | Superseded by ADR-0247 | 2026-09-09 | documentation, anchors, slug, guards, site, github, two-judges, adr-0139, adr-0142 |
| 0220 | [재검토 조건은 관측 가능성으로 가른다 — 채널은 전수가 아니라 우선순위로 짓는다](0220-reconsideration-triggers-are-split-by-observability.md) | Accepted | 2026-09-08 | adr, reconsideration-triggers, guards, channels, observability, census, adr-0139, adr-0142 |
| 0239 | [안 쓴 ADR 번호도 재사용하지 않는다 — 비어 있음의 원인을 다시 묻지 않기 위해](0239-an-unused-adr-number-is-retired-not-recycled.md) | Accepted | 2026-09-08 | adr, adr-numbering, identifiers, guards, census, measurement, adr-0138, adr-0139 |
| 0244 | [재검토 조건의 갈래는 소제목 둘로 표시한다 — 표지가 문면에 없으면 좌변이 없다](0244-the-trigger-split-is-marked-by-two-subheadings.md) | Accepted | 2026-09-08 | adr-conventions, documentation, reconsideration-triggers, observability, guards, left-side, adr-0220, adr-0139 |
| 0247 | [`site/content/` 의 앵커는 산출물을 읽는 판사에게 넘긴다 — ADR-0201 대체](0247-site-anchors-are-judged-by-the-artifact-not-a-copy-of-the-rule.md) | Accepted | 2026-09-08 | documentation, anchors, slug, guards, site, astro, two-judges, adr-0201, adr-0139 |
| 0296 | [도달 불가가 된 커밋 좌표는 지우지 않고 그 자리에 적는다](0296-an-unreachable-commit-coordinate-is-annotated-not-deleted.md) | Accepted | 2026-09-20 | documentation, guards, citations, git-history, floored-walk, adr-0105, adr-0139 |
| 0506 | [같은 결정이 서로 모르는 두 ADR 에 적혔으면 겹침의 폭으로 먼저 가른다 — 조항이면 새 ADR 로 모으고, 결정 전체면 한쪽을 Supersede 한다](0506-a-decision-recorded-twice-is-split-by-the-width-of-the-overlap.md) | Accepted | 2026-09-23 | adr-conventions, adr-duplication, documentation, supersede, adr-0239, adr-0030 |
| 0507 | [새 ADR 은 결정 대상의 심볼로 기존 ADR 을 찾은 뒤에, 대안 사이의 선택일 때만 쓴다](0507-an-adr-is-written-after-a-symbol-search-and-only-for-a-choice.md) | Accepted | 2026-09-23 | adr-conventions, adr-duplication, documentation, search, adr-0506, adr-0243, adr-0244 |
| 0508 | [착지는 커밋 전에 들어오는 ADR 을 나란히 놓는다 — 도구는 보고만 하고 판정은 사람이 한다](0508-landing-lines-up-incoming-adrs-before-the-commit.md) | Accepted | 2026-09-23 | adr-conventions, adr-duplication, landing, parallel-lanes, adr-0506, adr-0507, adr-0243, adr-0239 |
