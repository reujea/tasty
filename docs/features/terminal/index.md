# 터미널 (Terminal)

- **Status**: Implemented
- **주체**: 로컬 사용자 · AI Agent(입력 주입은 [terminal-output](../terminal-output/index.md)/`surface.send*`) · 원격(mirror)
- **ADR**: [ADR-0002](../../adr/0002-vte-parsing-off-input-thread.md)(파서 스레드) · [ADR-0008](../../adr/0008-inline-graphics-protocols-deferred.md)(인라인 그래픽 보류) · [ADR-0017](../../adr/0017-windows-suspend-resume-pty-recovery.md)(Windows 절전 PTY 복구)
- **코드**: `crates/tasty-terminal/` (PTY·VTE·grid·scrollback), 렌더 `src/gfx/`
- **화면**: GPU 렌더링 셀 그리드 (egui 아님)

## 목적

`terminal` surface kind 의 본체 — PTY 셸 세션을 VTE 파싱해 셀 그리드로 에뮬레이트하고 GPU 로 그린다. host 내장 surface([work-area](../work-area/index.md)의 Surface 종류).

## 내부 동작

### PTY 셸

ConPTY(Windows) / Unix PTY 로 네이티브 셸 실행(`TERM=xterm-256color`). 윈도우 리사이즈 시 자식에 새 크기 전파 — rows 축소 시 커서 아래 빈 행 먼저 제거 후 부족분은 위쪽 행을 scrollback 으로 캡처(커서-콘텐츠 관계 보존), 확대 시 scrollback 에서 복원.

**작업 디렉토리 상속**: 새 surface 생성 시 소스의 현재 cwd 를 상속(`general.inherit_cwd`, 기본 on). macOS/Linux 는 셸 PID 로 OS 직접 조회(`proc_pidinfo` / `/proc/<pid>/cwd`, OSC 7 캐시 우선), Windows 는 타 프로세스 cwd API 부재로 셸이 내보내는 OSC 7 캐시에만 의존(합성 rcfile 로 OSC 7 emit 강제). 합성 rcfile 은 OSC 0(cwd 기반 탭 제목, `__tasty_title`)도 함께 발화하며, 빌트인 블록에 버전 스탬프(`# tasty-bashrc-v<N>`)를 심어 스탬프가 다른 기존 `~/.tasty/bashrc`·`bashrc.default` 를 셸 spawn 시 자동 재생성한다(`ensure_compiled_bashrc_in` — 사용자 편집 영역 `bashrc.user` 는 보존). carry 규칙은 [surface-cwd invariant](../../design/policies/cwd.md#surface-cwd-invariant).

### 프로세스 종료 / 절전 복귀

자식 프로세스가 종료하면 `ProcessExited` 이벤트가 한 번 발화되고, cascade 가 해당 surface 를 자동 정리한다(hook 발화 → host event → surface close). 종료 판정은 호스트가 그 terminal 을 깨워 처리할 때 도는 `try_wait` 이고(`ALIVE_CHECK_INTERVAL` 500 ms 로 레이트 리밋, PTY EOF 뒤에는 리밋 없이 즉시), 깨우는 것은 파서 스레드다. PTY EOF 는 자식 종료보다 먼저 올 수 있어(커널이 자식의 fd 를 먼저 닫는다), 파서 스레드는 EOF 뒤 한 번 깨우고 끝나지 않고 종료가 판정되거나 terminal 이 닫힐 때까지 간격을 10 ms 부터 두 배씩 500 ms 까지 늘려 계속 깨운다 — 안 그러면 조용해진 PTY 의 종료를 영영 못 보고 죽은 surface 가 남는다([ADR-0523](../../adr/0523-pty-eof-keeps-waking-until-the-exit-is-settled.md)).

**Windows 절전(suspend/resume) 복구**(Windows 전용, [ADR-0017](../../adr/0017-windows-suspend-resume-pty-recovery.md)): ConPTY 는 `conhost.exe` + named pipe 기반이라, OS 절전(특히 modern standby/hibernate) 복귀 후 자식이 stdin 을 읽지 않고 멈출(hang) 수 있다. 메인 윈도우에 `WM_POWERBROADCAST` 서브클래스를 붙여 resume 를 감지하고 헬스 패스를 돈다 — (1) 죽은 자식은 즉시 `ProcessExited` cascade 로 정리, (2) 살아있는 자식은 현재 크기로 ConPTY resize 를 재발행해 wake nudge, (3) wake 로도 깨어나지 못할 수 있는(자식 TUI 가 도는) surface 는 알림으로 가시화. Unix PTY(macOS/Linux)는 sleep 이 프로세스를 freeze→thaw 하며 fd/파이프를 보존해 hang 이 생기지 않으므로 이 경로는 적용하지 않는다(`#[cfg(windows)]`). hang 은 idle 과 구분 불가해 자동 *완전* 복구는 보장하지 않으며, 최종 수단은 사용자 재시작이다.

### VTE 에뮬레이션

termwiz `Parser`/`Surface` 로 VT 시퀀스를 파싱·grid 갱신. 지원: 텍스트·제어코드(LF/CR/BS/Bell · **HT** 는 탭스톱(기본 8칸, HTS `ESC H`/CTC `CSI W` 로 설정, TBC `CSI g`/`CSI 3 g` 로 해제, RIS·리사이즈 시 기본값 재구성)으로 전진하며 termwiz 의 1칸 리터럴 탭 동작을 대체) · SGR(intensity/underline/italic/blink/inverse/strikethrough + fg/bg, **SGR 2 dim** = bg 50:50 블렌딩) · 커서 이동(CUP/CHA/VPA/CNL/CPL/CHT `CSI I`/CBT `CSI Z`/save·restore) · 커서 모양(DECSCUSR `CSI Ps SP q` — 0/기본·블록·언더라인·바 + blink 플래그, `cursor_shape()` 로 노출하고 **렌더러가 실제로 블록/바(`▏`)/언더라인(`▁`)으로 그림** — blink 변형은 정적 모양으로 매핑(콘텐츠 애니메이션 0ms 정책), vim/vi-mode insert 바 커서 표시) · 화면 편집(ED/EL/SU/SD/DCH/ICH/DL/IL/ECH — DCH/ICH 는 전각 2셀 처리 · **REP `CSI b`** 마지막 출력 문자 n회 반복) · ESC(DECSC/DECRC/IND/RI/**NEL `ESC E`**/**HTS `ESC H`**/**DECALN `ESC#8`**(화면을 'E' 로 채워 정렬 테스트)/RIS) · **DEC 라인드로잉 charset**(`ESC(0`/`ESC)0` G0/G1 지정 + SO/SI 로 GL 전환 — 활성 시 출력 ASCII `0x60–0x7e` 를 박스드로잉 글리프(`┌─┐│` 등)로 치환, ncurses/dialog/mc 류 테두리 호환; UK charset 은 ASCII 취급; RIS 로 리셋) · **DECSTR**(소프트 리셋 `CSI !p` — 스크롤 마진·저장 커서·SGR·앱 커서 키·IRM 삽입모드·커서 가시성을 기본값으로 되돌리되 RIS 와 달리 화면 내용·대체 화면은 보존, 무응답) · 스크롤 리전(DECSTBM) · 디바이스 응답(DSR/CPR · DA1 `CSI c`→`CSI ?1;2c` · DA2 `CSI >c`→`CSI >0;10;0c` · DA3 `CSI =c`→`DCS !|54415354 ST` · XTVERSION `CSI >q`→`DCS >|tasty(<ver>) ST`, 이름 `tasty`·버전은 tasty-terminal 크레이트 버전 · **XtGetTcap** `DCS +q <hexcap> ST`→ 각 질의 cap 마다 `DCS 0+r <hexcap> ST`(status 0, 요청 hex 그대로 echo) — termcap/terminfo 능력 DB 가 아직 없어 **현재 미지원**임을 알려 앱이 침묵 timeout 에 빠지지 않게 한다(영구 비지원 아님; 추후 status 1 `DCS 1+r <cap>=<hexvalue> ST` 실제 응답 추가 가능)). **DECSET/DECRST 모드**: DECCKM(1, 앱 커서 키) · DECTCEM(25, 커서 가시성) · 대체 화면(47/1047/1049) · 마우스 트래킹(1000/1002/1003 — **버튼 press/release·드래그·휠을 앱에 보고**; 드래그 motion 은 **버튼별로** 보고한다 — 좌 `32`(`0|32`) / 미들 `33`(`1|32`) / 우 `34`(`2|32`) 로 cb 하위 비트에 실제 눌린 버튼이 실린다. 여러 버튼을 겹쳐 누르면 **가장 최근 press** 가 motion 의 주인이고, 그 버튼을 떼면 아직 눌려 있는 버튼이 다시 주인이 된다. motion 은 press 를 **실제로 앱에 보고한** 버튼만 내보내므로 Shift+우클릭(메뉴 우회)·Shift+좌클릭(로컬 선택 우회)·블랙리스트 격하로 press 가 로컬 처리된 드래그는 중간 motion 도 나가지 않고, 대상 surface 는 press 시점에 고정된다; **1003 은 버튼 없는 hover 도 보고한다** — 셀이 바뀔 때마다 `ESC[<35;col;rowM`(버튼 코드 3=버튼 없음 + motion 32), 단 **focused surface 한정**이고 divider 밴드·OS 리사이즈 가장자리·divider 드래그 중에는 보내지 않는다([ADR-0081](../../adr/0081-hover-motion-focused-surface-only.md)); 1002 는 hover 를 보내지 않는다(드래그 motion 은 1002·1003 동일); 세 모드는 xterm 과 같이 **서로 독립된 on/off 레지스터**라 하나를 꺼도 나머지는 그대로 살아 있고, 실효 동작은 켜진 것 중 가장 넓은 것(1003 ⊃ 1002 ⊃ 1000)으로 정해진다 — 앱이 `1003h` 뒤에 `1002l` 을 보내도 1003 트래킹이 유지된다. RIS 는 세 레지스터를 모두 지우고, DECSTR 은 xterm 과 마찬가지로 건드리지 않는다; 트래킹 ON 이면 로컬 텍스트 선택·우클릭 컨텍스트 메뉴 대신 마우스를 앱에 전면 위임 [ADR-0019](../../adr/0019-mouse-button-reporting-app-delegation.md); 단 **Shift+우클릭은 앱에 보고하지 않고 tasty 컨텍스트 메뉴로 우회**하고, 마찬가지로 **Shift+좌클릭 드래그는 앱에 보고하지 않고 로컬 텍스트 선택으로 우회**한다(앱 위임을 깨지 않는 opt-in modifier 우회, xterm/iTerm2 표준 관례 — Shift 여부는 press 시점 1회 판정해 release 까지 유지하므로 드래그 중 Shift 를 떼도 선택 유지, Shift+더블/트리플클릭은 word/line) [ADR-0022](../../adr/0022-shift-rightclick-context-menu-bypass.md); 트래킹 진입 후 첫 (일반) 좌/우 클릭 시 "마우스 캡처 중 — 텍스트 선택은 Shift+드래그, 메뉴는 Shift+우클릭" 안내 배너(banner, surface 스코프 persistent)를 **트래킹 세션당 1회** 표시(좌·우 보고 경로가 같은 무장 플래그를 공유해 먼저 발생한 상호작용만 띄움; 설정 `general.mouse_capture_hint`, 기본 ON; 트래킹 `None→ON` 엣지·RIS 에서 재무장/해제); **마우스 캡처 비활성화 블랙리스트**(설정 `general.mouse_capture_blacklist`, 기본 빈 목록): 등록된 프로세스 이름 패턴(`.exe` 제거·소문자화 후 substring 또는 `*` glob, 1Hz busy 폴이 resolve 한 surface 별 foreground 로 판정·캐시)에 매칭되는 프로세스가 foreground 인 surface 는 트래킹 ON 이어도 **클릭/드래그/버튼을 앱에 보고하지 않고 로컬 처리**(좌클릭 선택·우클릭 tasty 메뉴)한다 — 단 **휠은 예외로 계속 앱에 보고**(클릭만 끔), 클릭이 로컬로 빠지므로 캡처 안내 배너도 뜨지 않는다; 미들클릭 보고는 macOS 가시 동작 검증 미완); **마우스 캡처 안내 배너 억제 블랙리스트**(설정 `general.mouse_capture_banner_blacklist`, 기본 빈 목록, [ADR-0055](../../adr/0055-mouse-capture-banner-suppress-list.md)): 위 캡처 비활성화 블랙리스트와 완전히 독립된 별도 축 — 등록된 프로세스 이름 패턴(동일 매칭 규칙)에 매칭되는 프로세스가 foreground 인 surface 는 캡처(클릭/드래그 앱 위임)는 그대로 유지하되 "마우스 캡처 중..." 안내 배너만 표시하지 않는다. 억제 대상 surface 에서는 armed 플래그(`take_mouse_capture_hint`)도 소모하지 않아, 같은 트래킹 세션 도중 foreground 가 비억제 앱으로 바뀌면 그 시점에 배너를 정상적으로 띄울 수 있다; **"더보기"(⋯) 퀵 엔트리**([ADR-0061](../../adr/0061-mouse-capture-banner-more-menu-quick-entry.md)): 위 두 블랙리스트에 Settings 를 거치지 않고 배너에서 바로 진입하는 경로 — 배너 hover 시 X 왼쪽에 ⋯ 트리거가 나타나고(메뉴가 열려 있는 동안은 hover 와 무관하게 계속 표시), 클릭하면 트리거 아래에 headless 컨텍스트 메뉴가 열려 "{app}에 대해 이 알림 끄기"(배너 억제 블랙리스트에 추가 + 배너 즉시 닫힘)와 "{app}에 대해 마우스 캡처 비활성화"(캡처 블랙리스트에 추가, 배너는 유지) 두 항목을 제공한다 — 저장/매칭 로직은 Settings 경로와 완전히 동일(같은 두 필드를 공유); **안내 배너 자동 닫힘**: 배너를 띄운 시점의 surface 별 foreground "인스턴스" generation(이름이 바뀔 때마다 +1, `CoreState::foreground_generation`)을 배너가 함께 기억해두고(`BannerState::origin_generation`), 1Hz BusyPoll 마다 그 값이 현재 generation 과 달라지면(=그 TUI 가 종료돼 쉘로 돌아왔든, 쉘을 거치지 않고 곧바로 다른 TUI 로 넘어갔든) 최대 1 초 이내에 자동으로 닫는다(`App::poll_busy_states`) — X 버튼 수동 닫기는 그대로 유지되는 별도 경로. 같은 스코프에 다른 배너(예: 셸 통합 미설치 안내)가 떠 있어도 id 를 확인하는 안전 API(`BannerManager::close_shown_if_id`)로 닫으므로 오폭하지 않는다. 서로 다른 TUI 가 쉘 경유 없이 연이어 같은 surface 에서 마우스 트래킹을 켜는 경우에도 이전 TUI 가 유발한 배너를 재사용하지 않고 교체한다(`BannerManager::push` 가 같은 id 라도 origin generation 이 다르면 `Replaced`). generation 은 이름 기반이라 동일 이름 프로그램의 연속 재실행(vim 종료 직후 다시 vim)은 새 인스턴스로 구분하지 못하는 한계가 있다(ADR-0055 의 이름 기반 블랙리스트 매칭과 동일한 트레이드오프). mirror(원격 attach) surface 는 로컬 foreground 폴링이 없어 이 자동 닫힘 대상이 아니다 · SGR 마우스(1006, 인코딩) · legacy X10 마우스 인코딩(`ESC[M`, SGR 미협상 시) · 포커스 트래킹(1004) · bracketed paste(2004) · 동기화 출력(2026) · **DECSCNM(5, 화면 반전 — 렌더러가 뷰포트 기본 fg/bg 를 스왑, `screen_reverse()` 로 노출; 렌더 적용 여부는 설정 `general.reverse_screen_enabled`(기본 on)로 게이트 — off 면 모드 플래그는 계속 추적하되(프로그램 조회 응답 정상) 화면 반전을 그리지 않아, readline `bell-style visible`(terminfo `flash`=DECSCNM 토글)이 내는 전체 화면 플래시를 억제)** · **DECOM(6, 원점 모드 — 절대 커서 위치(CUP/VPA/HVP)를 스크롤 리전 상대로 해석·리전 하단 클램프, 설정/해제 시 home 이동; 상대 이동의 리전 가둠은 미모델)**. **표준 모드(SM/RM, `?` 없는 `CSI .. h/l`)**: IRM(4, 삽입/덮어쓰기 — on 이면 출력 글리프가 기존 셀을 우측으로 밀어 ICH 처럼 삽입) · ShowCursor(25, DECTCEM 과 동일 동작). 그 외 표준 모드(KAM/SRM/LNM 등)는 무시. **XTWINOPS(`CSI Ps t`)**: 셀 크기 리포트(`18 t`→`CSI 8;rows;cols t`, `19 t`→`CSI 9;rows;cols t`)와 타이틀 스택(`22/23 t` push/pop, 단일 타이틀·64 entry bound)만 응답. 창 조작(Move/Resize/Maximize/Iconify/FullScreen/Raise/Lower)·창 위치/상태/타이틀 탐침·픽셀 크기 리포트(`14/16 t`)는 미지원([ADR-0011](../../adr/0011-xtwinops-window-ops-unsupported.md)). **OSC 8 하이퍼링크**(`OSC 8 ; params ; URI ST`): 열림 이후 출력되는 셀에 URI 를 셀 속성(`CellAttributes::hyperlink`)으로 부착, 빈 URI(`OSC 8 ; ; ST`)로 해제 — surface pen 에 상태를 실어 자동 적용하며 별도 상태 필드 없이 DECSTR/RIS 의 속성 리셋으로 함께 비워진다(hover·클릭은 [terminal-link](../terminal-link/index.md)). **OSC 52 클립보드 읽기 질의**(`OSC 52 ; c ; ? ST`): 터미널 크레이트는 `TerminalEventKind::ClipboardQuery` 이벤트만 발화하고 응답은 host 가 설정 게이트(`general.allow_clipboard_read`, 기본 off → 무응답) 후 처리 — 상세 [clipboard](../clipboard/index.md). **OSC 색상 질의**(OSC 10/11/12 = fg/bg/커서, OSC 4 = ANSI 팔레트): 앱이 `?` 로 질의하면(`OSC 11;? ST` 등) 현재 테마색을 `rgb:RRRR/GGGG/BBBB`(ST 종결) 로 회신해 다크/라이트 감지를 돕는다. 응답값은 호스트가 현재 테마에서 plumbing 한 팔레트(`Terminal::set_color_palette`)에서 가져오며 — fg/bg 는 terminal surface 의 focused 색, 커서는 fg(렌더러가 커서를 fg 색으로 그림), ANSI 16 은 테마 팔레트 — 생성·테마 변경 시 갱신된다. 색 *설정* 시퀀스(`?` 아닌 색 지정)는 저장소가 없어 현재 무시(후속). 팔레트 미주입 시 무응답.

**스크롤 리전과 자동 줄바꿈**: DECSTBM 으로 지정한 부분 영역은 명시적 개행(LF/IND/NEL)뿐 아니라 **오른쪽 끝에서의 자동 줄바꿈** 에도 적용된다. 커서가 영역 하단 행에 있을 때 긴 줄이 접히면 화면 전체가 아니라 그 영역만 한 줄 위로 스크롤하고 커서는 영역 하단 열 0 에 남는다 — 하단에 입력창을 남기는 TUI(Codex 등)의 영역 밖 행은 덮이지 않고, 위로 밀린 이력도 사라지지 않는다. 밀려난 행의 scrollback 적재는 명시적 스크롤과 같은 정책이다: 영역 상단이 화면 0 행인 경우에만 밀려난 행이 순서대로 이력에 들어가고, 상단이 0 이 아닌 내부 영역은 화면 밖으로 나가는 행이 없으므로 이력에 섞지 않는다. 커서가 영역 **아래** 의 마지막 화면 행에 있을 때는 영역도 화면도 스크롤하지 않고 그 행에서 줄바꿈이 되풀이된다(VT 의 "마진 밖 index 는 클램프"). 이 계약은 대체 화면에서도 같고, 대체 화면 출력은 primary 이력에 적재되지 않는다 — 대체 화면은 보통 영역을 안 깔아 화면 전체가 밀리는데, 그 경로에서도 회수하지 않는다. 요청된 마진은 **저장 시점에** 그리드 안으로 정규화한다(xterm 과 같이 화면 밖 하단은 마지막 행으로 자르고, 상단이 하단보다 크거나 화면 밖이면 한 줄 영역으로 접는다) — 명시 개행·자동 줄바꿈·DECOM 이 같은 경계를 읽게 하려는 것이다. DECAWM(`?7`)에 의한 자동 줄바꿈 **끄기** 는 별개 축으로 현재 미지원이다 — 끄더라도 줄바꿈은 일어나며, 다만 위 영역 계약을 지킨다.

**소거 명령과 걸친 커서**: ED/EL 은 **커서를 움직이지 않는 소거 연산**이고, 지우는 범위는 커서가 올라앉은 칸을 **포함**한다. 자동 줄바꿈이 대기 중인 상태("행 끝에 걸친 커서")는 커서 열이 화면 폭과 같은 값으로 나타나지만 그 칸은 그리드에 없다 — 커서가 실제로 올라앉은 칸은 마지막 열이고 줄바꿈은 다음 글자를 찍을 때로 미뤄져 있다. 그래서 범위를 셀 때 걸친 커서는 **마지막 열로 친다**: EL1(`CSI 1K`)은 행 전체를 지우고 그보다 한 칸 더 지우지 않으며(한 칸이 더 찍히면 그것이 다음 행으로 넘어가 **소거 명령이 줄바꿈을 일으킨다** — 부분 영역에서는 영역 스크롤 한 번 + 빈 이력 한 줄로 드러난다), EL0(`CSI 0K`)은 그 마지막 한 칸을 지운다. 반대쪽 끝도 같은 규칙이다 — 0 열 커서에서 EL1 이 지우는 칸은 0 이 아니라 **하나**다. 걸친 상태는 **여섯 분기 모두에서** 소거 뒤에도 보존되어 다음 글자가 정상적으로 줄바꿈된다.

| 명령 | 걸친 커서(`cx == cols`)에서 | 0 열에서 | 커서 |
|---|---|---|---|
| EL0 `CSI 0K` | 마지막 한 칸 | 해당 없음 | 보존(걸친 상태까지) |
| EL1 `CSI 1K` | 행 전체 | 한 칸 | 보존(걸친 상태까지) |
| EL2 `CSI 2K` | 행 전체 | 행 전체 | 보존(걸친 상태까지) |
| ED0 `CSI 0J` | 커서 행의 마지막 한 칸 + 아래 행 전부 | 해당 없음 | 보존(걸친 상태까지) |
| ED1 `CSI 1J` | 커서 행 전체 | 한 칸 | 보존(걸친 상태까지) |
| ED2 `CSI 2J` | 화면 전체 | 화면 전체 | 보존(걸친 상태까지) |

걸친 자리는 커서 이동으로 가리킬 수 없고(폭과 같은 열은 마지막 열로 잘린다) 거기로 돌아가는 길은 마지막 열에 한 칸을 **찍는 것** 하나뿐이다. 그 한 칸의 pen 이 나머지와 달라지는 것이 한때 EL2·ED2 의 걸침 보존과 EL0·ED0 의 범위를 함께 막고 있었는데, 아래 소거 pen 이 하나로 정해지면서 그 칸도 같은 pen 이라 막을 이유가 없어졌다.

**소거가 남기는 pen**: 지운 칸은 **기본 속성 + 소거 시점의 배경색**을 갖고(back color erase), 소거가 끝난 뒤 **pen 은 소거 전 그대로**다. 옮겨지는 것은 배경색 하나뿐이라 밑줄·역상 같은 나머지 속성은 지운 칸에 남지 않는다. 두 값은 따로 어긋날 수 있어 — 앞은 지운 칸에서, 뒤는 소거 직후 찍는 글자에서 보인다(`SGR → EL0 → 텍스트` 는 ncurses 류가 흔히 내는 순서다) — 시험이 여섯 분기에서 둘을 따로 잰다. 배경색 축을 규격이 정하지 않으므로(ECMA-48 은 "erased state" 만 말한다) 이 저장소는 자식에게 주는 `TERM=xterm-256color` 가 선언하는 `bce` 를 따른다. 근거·대안·재검토 조건은 [ADR-0292](../../adr/0292-erase-fills-with-the-current-background.md).

걸친 커서의 EL0·ED0 은 **다른 구현과 갈리는 자리다** — tmux 3.4 는 같은 `cx == cols` 인코딩을 쓰고 그 한 칸을 안 지운다(실측). 이 저장소는 규격 문언과 EL1·ED1 에서 이미 지키던 "걸친 커서는 마지막 열로 친다" 를 따라 지우는 쪽으로 정했다.

> **파서 스레드 분리**: 터미널마다 파서 스레드가 PTY raw 바이트를 읽는 즉시 그 스레드에서 VTE 파싱·grid 갱신(`ingest`). 메인(winit) 루프는 파싱하지 않아 백그라운드 터미널 출력이 포그라운드 입력/IPC 를 막지 않는다. grid 는 `Arc<Mutex<_>>` 공유, 8KB 청크마다 락 잡고 즉시 해제. 근거·대안 [ADR-0002](../../adr/0002-vte-parsing-off-input-thread.md).

### 스크롤백

화면 위로 밀린 줄을 `VecDeque` 에 보관(`scrollback_lines`, 기본 10,000, 0~100,000 설정). 마우스 휠/PageUp·Down 탐색, 타이핑 시 자동 라이브 뷰 복귀. 대체 화면(vim/less/htop)에선 스크롤백 비활성(모든 입력 PTY 로). 스크롤백 중 새 출력 도착 시 `scroll_offset` 자동 보정으로 위치 유지. 텍스트 wrap 에 의한 implicit 스크롤도 기록한다 — 출력 텍스트를 스크롤이 일어날 바이트 위치에서 잘라 적용하며 밀려나기 직전의 상단 행을 회수하므로, 사라진 행이 빠짐없이 순서대로 남는다(선택 영역 `absolute_row` 가 콘텐츠를 정확히 추적). 부분 스크롤 영역에서의 적재 범위는 위 VTE 절 "스크롤 리전과 자동 줄바꿈" 참조. 세션 간 보존은 disk scrollback. **ED3(`CSI 3J`)** 는 스크롤백 히스토리(메모리+디스크)를 비우고 뷰포트를 라이브로 되돌린다 — 화면 내용은 보존(`clear` 가 보내는 `\x1b[3J\x1b[2J` 에서 ED2 가 화면을, ED3 가 스크롤백을 담당).

### 키보드 입력

중앙 키보드 디스패처가 focused surface 타입에 따라 정확히 한 대상에만 전달 — Terminal 은 PTY 로 바이트. 특수 키(Enter/Backspace/Tab/Escape/방향키/Home·End/PageUp·Down/Insert·Delete/F1~F12) 매핑, DECCKM 모드에 따라 방향키 시퀀스 전환(`\x1b[{A..D}` ↔ `\x1bO{A..D}`). 복사/붙여넣기/선택/IME 는 [clipboard](../clipboard/index.md).

방향키와 함께 누른 Shift·물리 Alt(macOS Option)·Ctrl은 xterm 방식 `CSI 1 ; m A/B/C/D`로 전달한다. `m`은 1 + Shift(1) + Alt(2) + Ctrl(4)이며, 보조키가 있으면 DECCKM on/off 모두 CSI를 쓴다. 예: Option/Alt+↑는 `\x1b[1;3A`, Ctrl+Shift+←는 `\x1b[1;6D`. 보조키 없는 방향키는 위 DECCKM 규칙을 유지한다. 근거: [xterm cursor-key 및 modifier 표](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-PC-Style-Function-Keys)와 [modifyCursorKeys](https://invisible-island.net/xterm/manpage/xterm.html#VT100-Widget-Resources:modifyCursorKeys).

이 인코딩은 `option_as_meta`와 독립이다(그 설정은 문자 입력만 제어). 호스트 바인딩의 `alt` 토큰(macOS Command)으로 물리 Option을 치환하지 않으며, Super/Command 자체는 방향키 modifier 값에 넣지 않는다. 호스트 단축키·오버레이·vi 복사 모드가 소비한 키는 PTY로 중복 전달하지 않는다. 인코딩 결정은 `src/view/main/keyboard.rs`의 `decide_key_to_terminal`이 담당하고, 문자 Meta/compose와 IME Commit 경로는 별도로 처리한다.

**Option as Meta (macOS 전용)**: 설정 `general.option_as_meta`(기본 off, macOS 빌드에만 존재). on 이면 Option(Alt)+문자 입력이 특수문자(compose, 예 `å`) 대신 `ESC` + base 문자 Meta 시퀀스(예 `Option+a` → `\x1b a`)로 PTY 에 전달돼 readline/Emacs/vim 의 Meta 바인딩(`Alt+f`/`Alt+b` 등)이 동작한다. 물리 Option 키만 대상이며(Ctrl/Cmd 동시 누름 시 제외), 좌/우 Option 을 구분하지 않는다. base 문자는 Option 합성 이전의 US 레이아웃 문자(`physical_key_to_logical`)라 비-US 레이아웃에서는 US 기준으로 인코딩되는 한계가 있다. off 면 기존 Option=특수문자 동작을 보존한다. 키바인딩이 아니라 입력 인코딩 동작이라 `GeneralSettings` 에 둔다(`KeybindingSettings` 무관). 다른 OS 에는 Option 키가 없어 설정·UI 모두 노출하지 않는다.

### 색상 / 폰트

xterm-256color(ANSI 16 + 216 큐브 + 24 그레이) + TrueColor. 색은 Theme 의 ansi 팔레트([theme](../../design/systems/theme.md)). 폰트는 번들 D2Coding ligature(OFL 1.1, 임베드 — OS 미설치에도 동작), CJK fallback, 블록/박스 드로잉 글리프는 픽셀 퍼펙트 커스텀 렌더. 번들 폰트 파일은 합자 글리프를 포함하지만(폰트 자원), tasty 는 셀-격자 cross-cell 합자(프로그래밍 ligature) 적용도 설정 토글도 **미지원** — 보류 결정은 [ADR-0014](../../adr/0014-font-ligatures-deferred.md). 렌더 파이프라인(누적→flush→단일 패스, atlas LRU)은 [dev-guide/gpu-rendering](../../dev-guide/gpu-rendering.md).

### 이벤트 드리븐 렌더

파서 스레드가 ingest 직후 `AppEvent::TerminalOutput` 으로 메인 루프를 깨운다(`Waker`). 메인은 파싱이 아니라 변경된 grid 의 렌더·이벤트 수집만. 무조건 redraw 제거 — 실제 변경 시에만. **가시성 게이트**: 안 보이는 surface(비활성 워크스페이스/탭)는 출력을 항상 drain·파싱하되 `request_redraw` 는 생략(데이터 무손실, 재렌더만 절약). 출력·입력 없으면 CPU 0%.

Windows 에서는 focused terminal cursor 를 프로그램 주도 화면 갱신 뒤 짧게 숨긴다. VTE 파서가 carriage return, CSI cursor/edit, cursor save/restore action 을 실제로 본 뒤 120ms 동안 적용한다. Codex 같은 redraw-heavy CLI 가 커서 이동/줄 지우기를 빠르게 내보낼 때 중간 cursor hop 을 화면에 드러내지 않기 위한 렌더 정책이다. 단순 printable echo 는 화면 제어 action 이 아니므로 사용자가 직접 타이핑하는 동안 커서를 숨기지 않으며, 입력 직후라도 프로그램이 화면 제어 action 을 출력하면 억제를 적용한다. 커서를 숨긴 프레임은 후속 redraw 를 예약해 화면 갱신이 조용해진 뒤 커서가 다시 나타나게 한다. 다른 OS 에서는 이 억제 정책을 적용하지 않는다.

## 인터페이스

- **사용자**: 키보드/마우스 직접 입력.
- **AI Agent**: 입력 주입·출력 읽기는 [terminal-output](../terminal-output/index.md) + `surface.send*` ([reference/api](../../reference/api.md#surface-상호작용)). 터미널 surface 생성/닫기는 [work-area](../work-area/index.md).
- **원격**: PTY 없는 detached mirror 로 grid 재구성 ([remote-attach](../remote-attach/index.md)).

## 비-목표

- **인라인 그래픽**(Sixel/Kitty/iTerm 이미지) — 보류([ADR-0008](../../adr/0008-inline-graphics-protocols-deferred.md)). 이미지는 [image surface](../../plugins/image/index.md).
- **XTWINOPS 창 조작·창 탐침·픽셀 크기 리포트** — 미지원([ADR-0011](../../adr/0011-xtwinops-window-ops-unsupported.md), 사용자/에이전트 분리).
- **tmux control mode(DCS)·DECRQSS** — 미지원([ADR-0012](../../adr/0012-tmux-dcs-decrqss-unsupported.md), 범위 밖/드묾).
- **레거시·니치 입력 사설 모드**(Utf8Mouse 1005 · SGRPixels 1016 · Win32InputMode 9001 · DECCOLM 3 등) — 미지원([ADR-0013](../../adr/0013-niche-input-private-modes-unsupported.md), 표준 폴백으로 충분).
- 렌더/폰트 atlas 내부 구현 — [dev-guide/gpu-rendering](../../dev-guide/gpu-rendering.md).

## 관련

- [terminal-search](../terminal-search/index.md) · [terminal-link](../terminal-link/index.md) · [clipboard](../clipboard/index.md)
- [ADR-0002](../../adr/0002-vte-parsing-off-input-thread.md) · [dev-guide/gpu-rendering](../../dev-guide/gpu-rendering.md)
