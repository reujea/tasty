# macOS 권한 (TCC)

- **Status**: Partial — 파일 계열 + 화면 기록 pre-warm(+ debug 빌드 한정 손쉬운 사용 pre-warm), Full Disk Access 추정·안내 구현
- **주체**: 로컬 사용자 (AI Agent 가 간접 수혜 — 작업 도중 프롬프트로 멈추지 않는다)
- **ADR**: 없음
- **코드**: `crates/tasty-platform/src/macos_permissions.rs` (목록 결정 + 워커 + CoreGraphics/ApplicationServices FFI + FDA 추정), 호출부 `src/app/boot_machine.rs::finish_boot`, 캡처측 소비처 `crates/tasty-platform/src/screen_capture.rs`, 키 주입측 소비처 `src/adapters/ipc/handler/input_source.rs`, 설정 탭 `src/view/settings/ui/tabs/macos_permissions.rs`, 번들 usage description 은 `scripts/build-macos-dmg.sh` 의 Info.plist heredoc
- **화면**: 프롬프트는 OS 가 그린다. tasty 쪽 UI 는 부팅 안내 InfoModal + [설정 창](../settings/screens/settings.md) 일반 > 권한 탭

## 목적

macOS 는 보호 리소스에 **실제로 접근하는 그 순간**에만 권한 프롬프트를 띄운다. 터미널은 사용자가 친 명령을 대신 실행하는 것이 본업이라, 새 폴더를 처음 건드릴 때마다 프롬프트가 새로 뜬다. AI 에이전트가 터미널 안에서 자율 진행하는 도중에 프롬프트가 뜨면 **그 시점에서 작업이 멈춘다** — 자율 진행이 깨지는 실질적 원인이다.

발화 시점을 **부팅 직후로 몰아** 작업 중 중단을 없애는 것이 이 기능의 목적이다. 프롬프트의 총량을 줄이는 것이 아니라 **시점을 앞당기는 것**이다.

## 내부 동작

### 왜 tasty 이름으로 프롬프트가 뜨는가

PTY 로 띄운 자식 프로세스(zsh, 그 안의 에이전트)가 보호 리소스에 접근하면 macOS 는 그 접근의 *responsible process* 를 부모 GUI 앱(`Tasty.app`)으로 귀속시킨다. Terminal.app / iTerm2 와 같은 구조다. tasty 자신이 그 폴더를 읽지 않아도 프롬프트는 tasty 이름으로 뜬다.

### 발화 방법

파일 계열 TCC 서비스에는 "미리 물어보는" API 가 없다. 유일한 방법은 앱이 그 경로를 스스로 한 번 건드리는 것 — 대상 디렉터리에 `read_dir` 을 1 회 호출하고 결과를 버린다. 성공/실패 어느 쪽이든 무시한다(**거부는 정상 결과**다 — 사용자의 정당한 선택이므로 `debug!` 로만 남기고 `warn!` 하지 않는다).

### 대상과 순서

| TCC 서비스 | 경로 | 조건 |
|---|---|---|
| `SystemPolicyDownloadsFolder` | `~/Downloads` | 존재할 때 |
| `SystemPolicyDocumentsFolder` | `~/Documents` | 존재할 때 |
| `SystemPolicyDesktopFolder` | `~/Desktop` | 존재할 때 |
| `SystemPolicyRemovableVolumes` / `SystemPolicyNetworkVolumes` | `/Volumes/<마운트>` | 마운트가 있을 때 |

- **홈 폴더 3 곳이 먼저, 볼륨이 마지막.** 네트워크 볼륨의 `read_dir` 은 응답 없는 마운트에서 수 초~수십 초 걸리거나 영영 안 끝날 수 있다. 앞에 두면 사용자가 실제로 겪는 홈 폴더 프롬프트가 그만큼 늦어진다.
- **볼륨은 depth-1 로만** 나열해 마운트 항목당 한 번씩만 건드린다. 하위로 내려가지 않는다.
- **존재하지 않는 경로는 건너뛴다** — 없는 폴더를 읽어봐야 프롬프트가 뜨지 않는다.
- 볼륨 목록은 **경로로 정렬**한다. `read_dir` 순서는 파일시스템 마음이라 정렬하지 않으면 프롬프트 순서가 실행마다 달라진다.

### 워커 스레드 · 순차 발화

프롬프트가 떠 있는 동안 `read_dir` 은 **사용자가 응답할 때까지 리턴하지 않는다.** 따라서 두 제약이 따른다:

- **단일 워커 스레드**로 분리한다. 메인 스레드(winit 이벤트 루프)에서 부르면 프롬프트가 떠 있는 내내 UI 가 얼고, `boot_total` 계측도 사용자 응답 시간만큼 부풀려진다.
- 그 **한 스레드에서 경로를 하나씩 순차로** 처리한다. 동시에 건드리면 프롬프트가 겹쳐 뜬다 — 순차면 앞의 것을 닫아야 다음이 뜬다.

호출 지점은 `finish_boot` 의 `emit_startup_complete_event()` 직후다. 첫 윈도우가 등록돼 앱이 foreground 로 활성화된 뒤라야 프롬프트가 사용자에게 보인다 — 윈도우 생성 전에 부르면 백그라운드로 밀릴 수 있다.

### 매 부팅 반복 (첫 실행 플래그 없음)

이미 허용/거부가 결정된 항목에는 프롬프트가 뜨지 않으므로 반복 비용은 `read_dir` 몇 번뿐이다. 오히려 반복이 정확하다:

- 새 마운트(이동식/네트워크 볼륨)는 실행할 때마다 달라져 첫 실행 1 회로는 못 덮는다
- "첫 실행 여부" 플래그를 설정에 두면, 플래그만 남고 TCC 는 초기화된 상태(재설치·`tccutil reset` 이후)에서 pre-warm 이 영영 안 도는 어긋남이 생긴다

따라서 상태 플래그도, 끄는 토글도 두지 않는다. 이 기능은 프롬프트를 없애는 게 아니라 시점을 앞당기는 것이라 끄면 원래의 산발적 프롬프트로 돌아갈 뿐이다.

### 화면 기록 (Screen Recording)

파일 계열과 달리 **미리 요청하는 공개 API 가 있다** — CoreGraphics 의 `CGPreflightScreenCaptureAccess()`(상태 조회, 프롬프트 없음)와 `CGRequestScreenCaptureAccess()`(프롬프트 발화). 리소스를 몰래 건드려 유도할 필요 없이 정식으로 물어본다. 두 함수는 새 크레이트 없이 `#[link(name = "CoreGraphics", kind = "framework")]` 로 직접 선언한다 — `surface.raw_key` 의 CoreGraphics 선언과 같은 방식이다.

파일 폴더를 모두 처리한 **뒤 같은 워커 스레드에서** 이어서 부른다(프롬프트가 겹치지 않게). 절차는 preflight → 미승인일 때만 request **1 회**다:

- 이미 승인돼 있으면 아무것도 하지 않는다(프롬프트 없음).
- 이미 거부된 상태면 `CGRequestScreenCaptureAccess()` 가 프롬프트 없이 즉시 false 를 반환한다. 그래서 **재시도 루프를 두지 않는다** — 되돌리는 것은 시스템 설정에서 사용자가 할 일이다.
- 권한을 켜면 앱 재시작이 필요한 경우가 있고, 그 안내는 macOS 가 자체적으로 띄운다.

캡처 시점의 소비는 [원격 스크린샷 → 클립보드](../remote-screenshot-clipboard/index.md) 가 담당한다 — `screen_recording_authorized()` 를 캡처 **직전**에 불러 권한 미승인을 사용자 취소와 구분한다. 비-macOS 에는 같은 이름의 함수가 항상 `true` 를 반환해 그쪽 캡처 경로가 기존과 똑같이 동작한다.

### 손쉬운 사용 (Accessibility)

`surface.raw_key` 는 `CGEventPost` 로 시스템에 키 이벤트를 주입한다. 이 API 는 손쉬운 사용(`kTCCServiceAccessibility`) 권한을 요구하며, 권한이 없으면 **이벤트가 조용히 버려진다** — 호출자는 성공 응답을 받고도 아무 일도 일어나지 않는 것을 본다.

**이 기능은 debug 빌드 전용이다** — OS 전역 키 주입은 사용자 입력 재현이라 release IPC/CLI 표면에 없다([ADR-0115](../../adr/0115-input-reproduction-ipc-debug-isolation.md), [debug-ipc](../../dev-guide/debug-ipc.md)). 따라서 **손쉬운 사용 권한을 소비하는 코드가 release 빌드에는 하나도 없다.**

**요청 시퀀스도 debug 빌드에서만 돈다.** 소비자가 0 인데 첫 실행에 "이 앱이 내 모든 입력을 볼 수 있게 해달라" 로 읽히는 프롬프트를 띄우는 것은 최소권한 원칙에 어긋난다. release 사용자에게 손쉬운 사용은 **켜라고 안내하지도, 프롬프트를 띄우지도 않는 항목**이다. 자기검증용 debug 빌드에서만 아래 시퀀스가 돌고, 권한을 켠 뒤 재시작이 필요한 것도 그 빌드에서의 이야기다.

화면 기록과 마찬가지로 **정식 요청 API 가 있다** — ApplicationServices 의 `AXIsProcessTrusted()`(프롬프트 없는 상태 조회)와 `AXIsProcessTrustedWithOptions()`(`kAXTrustedCheckOptionPrompt: kCFBooleanTrue` 를 넘기면 프롬프트). 새 크레이트 없이 `#[link(name = "ApplicationServices", kind = "framework")]` 로 선언한다. 옵션 딕셔너리는 `CFDictionaryCreate` 에 `kCFTypeDictionaryKeyCallBacks`/`ValueCallBacks` 를 함께 넘겨 만든다 — 콜백을 생략하면 키가 `CFEqual` 이 아니라 포인터 동일성으로 비교돼 옵션이 무시된다.

**시퀀스에서 맨 마지막**(debug 빌드). 파일 폴더 → 화면 기록 → 손쉬운 사용 순으로 같은 워커 스레드에서 이어 부른다. 앞의 둘은 그 자리에서 허용/거부가 끝나지만, 손쉬운 사용 프롬프트는 "시스템 설정을 열겠느냐"는 안내여서 **그 자리에서 권한이 켜지지 않고** 사용자를 시스템 설정으로 내보낸다. 그 이탈을 시퀀스 맨 끝에 둬야 앞의 프롬프트들이 묻히지 않는다. release 빌드의 시퀀스는 화면 기록에서 끝난다.

- 이미 승인돼 있으면(`AXIsProcessTrusted()`) 요청하지 않는다.
- 요청은 **부팅당 1 회**다. 미설정·거부 상태에서 반복 호출하면 프롬프트가 계속 뜬다.
- 권한을 켜면 실행 중인 프로세스에 즉시 반영되지 않아 **앱 재시작이 필요한 경우가 많다.**

**주입 시점의 소비** (debug 빌드) — `handle_raw_key` 는 주입 직전에 `AXIsProcessTrusted()` 를 부르고, 미승인이면 `-32001 permission_denied: …` 에러를 돌려준다(권한 계열 거부의 기존 코드·접두사와 같다). 이 조회는 **호출 시점마다** 한다 — 부팅 값을 캐시하면 사용자가 그 사이 시스템 설정에서 켠 것을 반영하지 못한다. 판정 자체(`raw_key_decision`)는 `#[cfg(any(debug_assertions, test))]` 순수 함수라 전 플랫폼에서, release 빌드에서도 유닛테스트된다. 주입 경로에는 `--enable-input-simulation` 런타임 게이트가 손쉬운 사용 권한 확인보다 **먼저** 걸린다 — 플래그 없이 띄운 debug 인스턴스에서는 권한 여부와 무관하게 `-32001` 로 거부된다.

### Full Disk Access — 추정과 안내

FDA(`kTCCServiceSystemPolicyAllFiles`)를 부여하면 "다른 앱의 데이터" 를 포함한 **파일 접근 계열 전부**가 프롬프트 없이 통과한다. 대상 앱 디렉터리 단위로 갈라져 pre-warm 이 불가능한 AppData 계열을 없앨 수 있는 유일한 수단이다.

**앱이 요청할 수 없다.** 요청 API 가 없고 `tccutil`/TCC.db 조작은 SIP 가 막는다. tasty 가 할 수 있는 것은 (a) 보유 추정과 (b) 해당 패널로 보내는 안내뿐이다.

**추정 방법과 그 한계** — FDA 로만 읽히는 것으로 알려진 경로(`/Library/Application Support/com.apple.TCC/TCC.db`, 보조로 사용자 홈의 같은 경로)를 열어본다. 그 경로는 거부될 때 **프롬프트 없이 조용히** `EPERM` 을 내므로 안전하게 시도할 수 있다. 다만 이는 공개 API 가 아니라 우회 판정이며, macOS 가 그 경로의 보호 정책을 바꾸면 **오탐**이 난다. 그래서 이 값은 **안내를 띄울지 여부에만** 쓰고 어떤 기능도 이 값으로 막지 않는다.

**안내 방식** — 부팅 시 FDA 가 없어 보이면 InfoModal 을 **평생 1 회** 띄우고, 띄운 사실을 `general.macos_fda_notice_shown` 에 즉시 기록한다. 모달에는 시스템 설정의 전체 디스크 접근 권한 패널을 여는 버튼(`x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles`, `open(1)` 로 실행)이 붙는다. 다시 보려면 설정 > 일반 > 권한의 토글을 켠다 — 오탐으로 안내가 떴을 때의 탈출구이자, 나중에 다시 보고 싶을 때의 경로다.

**안내 문구가 지켜야 할 것** — FDA 는 파일 접근 프롬프트만 없앤다. **Automation(다른 앱 제어) · 화면 기록 · 손쉬운 사용은 FDA 와 별개 TCC 서비스라 그대로 남는다.** 문구가 "모든 프롬프트가 사라진다" 로 읽히면 안 된다. 또 ad-hoc 서명 빌드는 재빌드마다 다른 앱으로 인식돼 FDA 가 초기화되므로, 직접 빌드하는 사용자에게 `Tasty Dev` 인증서 서명이 선행 조건임을 함께 알린다([build.md](../../dev-guide/build.md) 참조).

### 설정 탭 (일반 > 권한)

macOS 에서만 노출된다. FDA(추정)·화면 기록의 현재 상태, FDA 가 추정임을 밝히는 주석, 전체 디스크 접근 권한 패널 바로가기, 부팅 안내 재표시 토글을 담는다. 부팅 안내를 지나쳤거나 껐어도 여기서 현재 상태를 볼 수 있다.

**손쉬운 사용 상태 행은 debug 빌드에만 있다.** release 에는 이 권한을 소비하는 코드가 없어 프롬프트도 띄우지 않으므로, 행을 남기면 켤 이유도 끌 이유도 없는 항목이 영구히 "미승인" 으로 보인다. debug 빌드에서는 주입 경로를 자기검증할 때 승인 상태를 확인할 자리가 필요해 그대로 둔다.

### 프롬프트 본문 설명 문구

번들 `Info.plist` 의 `NS*UsageDescription` 키가 프롬프트 본문에 그대로 표시된다. 키가 없으면 이유 없는 프롬프트가 뜨고, pre-warm 처럼 여러 개를 연달아 띄우면 그 문제가 커진다. 키 목록과 문구 정책은 [build.md](../../dev-guide/build.md) 의 배포 패키징 절 참조.

### 플랫폼 격리

목록 결정 로직(`prewarm_targets`)은 파일시스템 조회를 `FsProbe` 로 추상화한 **순수 함수**라 전 플랫폼에서 컴파일·유닛테스트된다. 실제 파일 접근부(`RealFs`·`spawn_prewarm`)만 `#[cfg(all(target_os = "macos", feature = "gui"))]` 로 좁힌다. cfg 로 잘린 코드는 rustc 가 타입체크 전에 걷어내므로, 로직을 순수부에 몰아둘수록 다른 플랫폼에서 검증되는 면적이 넓어진다. headless 는 프롬프트를 띄울 GUI 주체가 없으므로 macOS 여도 돌지 않는다. 비-macOS/headless 에는 같은 이름의 no-op 이 노출돼 호출부에 `#[cfg]` 가 흩어지지 않는다.

## 비-목표 (Out of scope)

**pre-warm 이 원천적으로 불가능한 것** — 대상별로 프롬프트가 갈라져 사전 열거가 안 된다:

- **다른 앱의 데이터** (`kTCCServiceSystemPolicyAppData`, macOS 14+) — `~/Library/Application Support/<앱>`, `~/Library/Containers/<번들ID>` 처럼 **대상 앱 디렉터리 단위로 개별 프롬프트**가 뜬다. 존재하는 디렉터리를 전부 순회하면 프롬프트가 수십 개 뜨므로 현실적 선택지가 아니다. Full Disk Access 로 덮이며, 그 안내는 위 "Full Disk Access" 절이 담당한다.
- **다른 앱 제어** (Automation / Apple Events) — 대상 앱 단위. 셸에서 `osascript` 를 쓸 때 발생하며 사전 열거 불가. **FDA 로도 덮이지 않는다** — FDA 는 파일 접근 서비스이고 Automation 은 완전히 별개 서비스라, FDA 를 줘도 대상 앱별 승인은 계속 요구된다. **어떤 방법으로도 사전 일괄 발화가 불가능**하며 tasty 가 할 수 있는 일이 없다. tasty 자신은 Apple Events 를 보내지 않는다.

**해당 없음** (레포 전체 확인): 카메라·마이크·위치(`AVCaptureDevice`/`AVAudioSession`/`CLLocationManager` 미사용), OS 알림 센터(tasty 는 자체 토스트만 쓴다), Local Network(IPC 서버는 loopback bind — 대상 제외), 드래그앤드롭·네이티브 파일 피커(사용자가 그 자리에서 명시적으로 고른 파일이라 macOS 가 별도 동의로 취급, 프롬프트 없음).

## Acceptance Criteria

- Given `~/Documents` 가 없는 홈 When pre-warm 목록을 결정 Then 그 경로가 목록에서 빠진다
- Given `/Volumes` 에 마운트 2 개 When 목록을 결정 Then 홈 폴더 뒤에, 경로 정렬 순으로 붙는다
- Given 마운트가 하나도 없음 When 목록을 결정 Then 볼륨 항목이 하나도 없다
- Given macOS 에서 TCC 승인을 초기화 When Tasty 실행 Then 부팅 직후 프롬프트가 순차로 뜨고, 떠 있는 동안에도 창이 그려지고 클릭·스크롤이 반응한다
- Given 프롬프트를 모두 허용 When 터미널에서 `ls ~/Downloads; ls ~/Documents; ls ~/Desktop` Then 추가 프롬프트가 뜨지 않는다
- Given 화면 기록 권한 미결정 When Tasty 실행 Then 파일 프롬프트들 **뒤에** 화면 기록 프롬프트가 뜬다
- Given 화면 기록 권한을 거부한 뒤 재실행 Then 프롬프트가 다시 뜨지 않는다(무한 재요청 없음)
- Given 이미 안내함(`macos_fda_notice_shown` = true) When 부팅 Then FDA 안내를 띄우지 않는다
- Given 아직 안내 안 함 + FDA 가 있어 보임 When 부팅 Then FDA 안내를 띄우지 않는다
- Given 아직 안내 안 함 + FDA 가 없어 보임 When 부팅 Then FDA 안내를 1 회 띄우고 표시 기록을 남긴다
- Given FDA 안내가 떠 있음 When 설정 열기 버튼 클릭 Then 전체 디스크 접근 권한 패널이 열린다
- Given 손쉬운 사용 권한 미결정 When debug 빌드 실행 Then 화면 기록 프롬프트 **뒤에** 손쉬운 사용 프롬프트가 뜬다
- Given 손쉬운 사용 권한 미결정 When release 빌드 실행 Then 손쉬운 사용 프롬프트가 뜨지 않는다(요청 자체가 없다)
- Given 손쉬운 사용 권한이 이미 승인됨 When debug 빌드 실행 Then 프롬프트가 뜨지 않는다
- Given 손쉬운 사용 권한 미승인 + `--enable-input-simulation` 으로 띄운 debug 빌드 When `surface.raw_key` 호출 Then 성공이 아니라 `permission_denied` 에러가 돌아온다
- Given 손쉬운 사용 권한 승인 후 재시작 + `--enable-input-simulation` 으로 띄운 debug 빌드 When `surface.raw_key` 호출 Then OS 포커스를 가진 대상에 키가 실제로 입력된다
- Given `--enable-input-simulation` 없이 띄운 debug 빌드 When `surface.raw_key` 호출 Then 손쉬운 사용 권한 여부와 무관하게 `-32001` 로 거부된다
- Given release 빌드 When `surface.raw_key` 호출 Then `method_not_found` 로 떨어진다(메서드가 표에도 라우터에도 없다)
- Given release 빌드 When 설정 > 일반 > 권한 탭 열기 Then 손쉬운 사용 상태 행이 없다(FDA·화면 기록 두 행만)
- Given debug 빌드 When 설정 > 일반 > 권한 탭 열기 Then 손쉬운 사용 상태 행이 있다
