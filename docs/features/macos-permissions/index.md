# macOS 권한 (TCC)

- **Status**: Partial — 사용자가 요청하면 파일 계열과 화면 기록을 한 번에 요청하고(debug 빌드에서는 손쉬운 사용까지), Full Disk Access 상태를 추정하며, 허용되지 않은 권한이 있으면 부팅 안내를 띄운다
- **주체**: 로컬 사용자 (에이전트 작업 중 권한 요청으로 멈추는 상황도 줄인다)
- **설계**: 이 문서 (요청 시점) · [호스트 호출 계약](../../dev-guide/plugin-permissions.md) (손쉬운 사용 debug 격리)
- **코드**: `crates/tasty-platform/src/macos_permissions.rs` (목록 결정 + 워커 + CoreGraphics/ApplicationServices FFI + FDA 추정 + 표시용 스냅샷), 요청 호출부와 설정 탭 `src/view/settings/ui/tabs/macos_permissions.rs`, 안내 호출부 `src/app/boot_machine.rs::finish_boot`, 캡처측 소비처 `crates/tasty-platform/src/screen_capture.rs`, 키 주입측 소비처 `src/adapters/ipc/handler/input_source.rs`, 번들 usage description 은 `scripts/build-macos-dmg.sh` 의 Info.plist heredoc
- **화면**: 프롬프트는 OS 가 그린다. tasty 쪽 UI 는 부팅 안내 InfoModal + [설정 창](../settings/screens/settings.md) 일반 > 권한 탭

## 목적

macOS의 파일 접근 권한 요청은 보호된 경로에 처음 접근할 때 나타난다. 에이전트가 명령을
실행하는 중에도 사용자 응답을 기다릴 수 있으므로, 자주 사용하는 경로의 권한을 한 번에
미리 요청한다. 모든 권한 요청을 없애는 기능은 아니며 아래에 나열한 대상만 처리한다.

요청은 사용자가 설정 > 일반 > 권한에서 [모든 권한 요청하기]를 눌렀을 때만 일어난다.
부팅 직후에 자동으로 요청하지는 않는다. 앱을 처음 켠 사람에게 이유를 알 수 없는 프롬프트가
연달아 뜨고 그것을 끌 방법도 없었기
때문이다(이 문서).
대신 허용되지 않은 권한이 남아 있으면 부팅할 때 안내로 알린다(아래 "부팅 권한 안내").

## 내부 동작

### 왜 tasty 이름으로 프롬프트가 뜨는가

PTY 로 띄운 자식 프로세스(zsh, 그 안의 에이전트)가 보호 리소스에 접근하면 macOS 는 그 접근의 *responsible process* 를 부모 GUI 앱(`Tasty.app`)으로 귀속시킨다. Terminal.app / iTerm2 와 같은 구조다. tasty 자신이 그 폴더를 읽지 않아도 프롬프트는 tasty 이름으로 뜬다.

<a id="발화-방법"></a>

### 권한 요청 방법

파일 계열 TCC 서비스에는 "미리 물어보는" API 가 없다. 유일한 방법은 앱이 그 경로에 한 번 접근하는 것 — 대상 디렉터리에 `read_dir` 을 1 회 호출하고 결과를 버린다. 성공/실패 어느 쪽이든 무시한다(**거부는 정상 결과**다 — 사용자의 정당한 선택이므로 `debug!` 로만 남기고 `warn!` 하지 않는다).

### 대상과 순서

| TCC 서비스 | 경로 | 조건 |
|---|---|---|
| `SystemPolicyDownloadsFolder` | `~/Downloads` | 존재할 때 |
| `SystemPolicyDocumentsFolder` | `~/Documents` | 존재할 때 |
| `SystemPolicyDesktopFolder` | `~/Desktop` | 존재할 때 |
| `SystemPolicyRemovableVolumes` / `SystemPolicyNetworkVolumes` | `/Volumes/<마운트>` | 마운트가 있을 때 |

- **홈 폴더 3 곳이 먼저, 볼륨이 마지막.** 네트워크 볼륨의 `read_dir` 은 응답 없는 마운트에서 수 초~수십 초 걸리거나 끝나지 않을 수 있다. 앞에 두면 사용자가 실제로 겪는 홈 폴더 프롬프트가 그만큼 늦어진다.
- **볼륨은 depth-1 로만** 나열해 마운트 항목당 한 번씩만 접근한다. 하위로 내려가지 않는다.
- **존재하지 않는 경로는 건너뛴다** — 없는 폴더를 읽어봐야 프롬프트가 뜨지 않는다.
- 볼륨 목록은 **경로로 정렬**한다. `read_dir` 순서는 파일시스템에 따라 달라서 정렬하지 않으면 프롬프트 순서가 실행마다 달라진다.

### 요청 시점

`request_all_permissions`는 설정 > 일반 > 권한의 [모든 권한 요청하기]에서만 호출한다.
부팅 경로에는 호출이 없다.

요청이 진행 중일 때 다시 들어오는 것은 원자 플래그(`REQUEST_RUNNING`)로 막는다. 이미
진행 중이면 아무것도 하지 않고 `false`를 돌려주며, 화면에서도 그동안 버튼을 비활성으로
그린다. 요청이 겹치면 아래 "워커 스레드에서 순서대로 요청"의 전제가 깨져 프롬프트가
포개진다.

요청이 끝나면 표시용 스냅샷을 갱신하고 `on_finished`로 호출자를 깨운다. 그 시점에는
사용자 입력이 없어 화면이 저절로 다시 그려지지 않기 때문이다. 순서는 스냅샷 갱신, 플래그
해제, 완료 통지다. 순서가 바뀌면 호출자가 낡은 스냅샷을 읽는다.

<a id="워커-스레드--순차-발화"></a>

### 워커 스레드에서 순서대로 요청

프롬프트가 떠 있는 동안 `read_dir` 은 **사용자가 응답할 때까지 리턴하지 않는다.** 따라서 두 제약이 따른다:

- **단일 워커 스레드**로 분리한다. 메인 스레드(winit 이벤트 루프)에서 부르면 프롬프트가 떠 있는 내내 UI가 멈추고, `boot_total` 계측도 사용자 응답 시간만큼 부풀려진다.
- 그 **한 스레드에서 경로를 하나씩 순차로** 처리한다. 동시에 건드리면 프롬프트가 겹쳐 뜬다 — 순차면 앞의 것을 닫아야 다음이 뜬다.

워커 스레드는 egui가 화면을 그리는 도중에 시작한다. 그리는 코드에서 요청을 직접
실행하면 프롬프트가 떠 있는 내내 설정 창이 멈추므로, 버튼은 스레드만 띄우고 곧바로
반환한다.

### 여러 번 눌러도 문제가 없다

이미 허용/거부가 결정된 항목에는 프롬프트가 뜨지 않으므로 반복 비용은 `read_dir` 몇 번뿐이다. 오히려 반복이 정확하다:

- 새로 연결한 이동식·네트워크 볼륨은 실행할 때마다 달라져 한 번의 요청으로는 덮을 수 없다
- "이미 요청했다"는 기록을 설정에 두면, 재설치나 `tccutil reset`, ad-hoc 재빌드로 앱 식별이 바뀌어 TCC가 초기화됐을 때 기록만 남고 요청은 다시 일어나지 않는다

그래서 요청 이력을 저장하지 않는다. 버튼은 언제 눌러도 같은 일을 한다.

### 화면 기록 (Screen Recording)

파일 계열과 달리 **미리 요청하는 공개 API 가 있다** — CoreGraphics 의 `CGPreflightScreenCaptureAccess()`(상태 조회, 프롬프트 없음)와 `CGRequestScreenCaptureAccess()`(프롬프트 발생). 리소스에 직접 접근하지 않고 이 API로 요청한다. 두 함수는 새 크레이트 없이 `#[link(name = "CoreGraphics", kind = "framework")]` 로 직접 선언한다 — `surface.raw_key` 의 CoreGraphics 선언과 같은 방식이다.

파일 폴더를 모두 처리한 **뒤 같은 워커 스레드에서** 이어서 부른다(프롬프트가 겹치지 않게). 절차는 preflight → 미승인일 때만 request **1 회**다:

- 이미 승인돼 있으면 아무것도 하지 않는다(프롬프트 없음).
- 이미 거부된 상태면 `CGRequestScreenCaptureAccess()` 가 프롬프트 없이 즉시 false 를 반환한다. 그래서 **재시도 루프를 두지 않는다** — 되돌리는 것은 시스템 설정에서 사용자가 할 일이다.
- 권한을 켜면 앱 재시작이 필요한 경우가 있고, 그 안내는 macOS 가 자체적으로 띄운다.

캡처 시점의 소비는 [원격 스크린샷 → 클립보드](../remote-screenshot-clipboard/index.md) 가 담당한다 — `screen_recording_authorized()` 를 캡처 **직전**에 불러 권한 미승인을 사용자 취소와 구분한다. 비-macOS 에는 같은 이름의 함수가 항상 `true` 를 반환해 그쪽 캡처 경로가 기존과 똑같이 동작한다.

### 손쉬운 사용 (Accessibility)

`surface.raw_key` 는 `CGEventPost` 로 시스템에 키 이벤트를 주입한다. 이 API 는 손쉬운 사용(`kTCCServiceAccessibility`) 권한을 요구하며, 권한이 없으면 **이벤트가 조용히 버려진다** — 호출자는 성공 응답을 받고도 아무 일도 일어나지 않는 것을 본다.

**이 기능은 debug 빌드 전용이다** — OS 전역 키 주입은 사용자 입력 재현이라 release IPC/CLI 표면에 없다([호스트 호출 계약](../../dev-guide/plugin-permissions.md), [debug-ipc](../../dev-guide/debug-ipc.md)). 따라서 **손쉬운 사용 권한을 소비하는 코드가 release 빌드에는 하나도 없다.**

손쉬운 사용 권한은 debug 빌드에서만 요청한다. release에는 이 권한을 사용하는 코드가
없으므로 요청 프롬프트나 활성화 안내를 표시하지 않는다.

화면 기록과 마찬가지로 **정식 요청 API 가 있다** — ApplicationServices 의 `AXIsProcessTrusted()`(프롬프트 없는 상태 조회)와 `AXIsProcessTrustedWithOptions()`(`kAXTrustedCheckOptionPrompt: kCFBooleanTrue` 를 넘기면 프롬프트). 새 크레이트 없이 `#[link(name = "ApplicationServices", kind = "framework")]` 로 선언한다. 옵션 딕셔너리는 `CFDictionaryCreate` 에 `kCFTypeDictionaryKeyCallBacks`/`ValueCallBacks` 를 함께 넘겨 만든다 — 콜백을 생략하면 키가 `CFEqual` 이 아니라 포인터 동일성으로 비교돼 옵션이 무시된다.

**시퀀스에서 맨 마지막**(debug 빌드). 파일 폴더 → 화면 기록 → 손쉬운 사용 순으로 같은 워커 스레드에서 이어 부른다. 앞의 둘은 그 자리에서 허용/거부가 끝나지만, 손쉬운 사용 프롬프트는 "시스템 설정을 열겠느냐"는 안내여서 **그 자리에서 권한이 켜지지 않고** 사용자를 시스템 설정으로 내보낸다. 그 이탈을 시퀀스 맨 끝에 둬야 앞의 프롬프트들이 묻히지 않는다. release 빌드의 시퀀스는 화면 기록에서 끝난다.

- 이미 승인돼 있으면(`AXIsProcessTrusted()`) 요청하지 않는다.
- 요청은 사용자가 [모든 권한 요청하기] 버튼을 누를 때마다 한다. 진행 중인 요청과 겹치는 것만 막고 횟수 기록은 두지 않으며, 부팅 경로에서는 요청하지 않는다. 미설정·거부 상태에서 버튼을 다시 누르면 안내가 다시 뜬다.
- 권한을 켜면 실행 중인 프로세스에 즉시 반영되지 않아 **앱 재시작이 필요한 경우가 많다.**

**주입 시점의 소비** (debug 빌드) — `handle_raw_key` 는 주입 직전에 `AXIsProcessTrusted()` 를 부르고, 미승인이면 `-32001 permission_denied: …` 에러를 돌려준다(권한 계열 거부의 기존 코드·접두사와 같다). 이 조회는 **호출 시점마다** 한다 — 부팅 값을 캐시하면 사용자가 그 사이 시스템 설정에서 켠 것을 반영하지 못한다. 판정 자체(`raw_key_decision`)는 `#[cfg(any(debug_assertions, test))]` 순수 함수라 전 플랫폼에서, release 빌드에서도 유닛테스트된다. 주입 경로에는 `--enable-input-simulation` 런타임 게이트가 손쉬운 사용 권한 확인보다 **먼저** 걸린다 — 플래그 없이 띄운 debug 인스턴스에서는 권한 여부와 무관하게 `-32001` 로 거부된다.

### Full Disk Access — 추정과 안내

FDA(`kTCCServiceSystemPolicyAllFiles`)를 허용하면 "다른 앱의 데이터"를 포함한 파일
접근 권한 전체가 프롬프트 없이 통과한다. 앱 디렉터리 단위로 프롬프트가 갈라져 미리
요청할 수 없는 AppData 계열을 없앨 수 있는 유일한 방법이다.

**앱이 요청할 수 없다.** 요청 API 가 없고 `tccutil`/TCC.db 조작은 SIP 가 막는다. tasty 가 할 수 있는 것은 (a) 보유 추정과 (b) 해당 패널로 보내는 안내뿐이다.

**추정 방법과 그 한계** — FDA 로만 읽히는 것으로 알려진 경로(`/Library/Application Support/com.apple.TCC/TCC.db`, 보조로 사용자 홈의 같은 경로)를 열어본다. 그 경로는 거부될 때 **프롬프트 없이 조용히** `EPERM` 을 내므로 안전하게 시도할 수 있다. 다만 이는 공개 API 가 아니라 우회 판정이며, macOS 가 그 경로의 보호 정책을 바꾸면 **오탐**이 난다. 그래서 이 값은 **안내를 띄울지 여부에만** 쓰고 어떤 기능도 이 값으로 막지 않는다.

판정은 `FullDiskAccess::{Granted, Denied, Unknown}`의 세 상태다. 파일을 열면 허용,
`PermissionDenied`면 미허용, 경로가 없는 경우 등은 확인 불가로 처리한다. 경로 부재를
미허용으로 처리하면 macOS가 파일을 옮기거나 없앤 환경에서 매 부팅마다 잘못 안내할 수 있다.

**안내 방식** — 부팅 안내는 FDA만을 위한 것이 아니라 권한 전반을 다룬다. 아래 "부팅 권한
안내" 절을 참고한다.

안내를 표시했다는 기록이나 끄는 토글은 두지 않고 매 부팅마다 다시 판정한다. 사용자가
권한을 취소하거나 `tccutil reset`, ad-hoc 재빌드로 앱 식별이 바뀐 경우도 반영하기 위해서다.
권한이 확인되면 다음 부팅부터는 안내하지 않는다. 파일 권한 요청에 이력을 남기지 않는
것과 같은 이유다.

**FDA 문단의 갈래** — 안내 본문의 FDA 문단은 보유 이력에 따라 셋으로 갈린다
(`tasty_platform::macos_permission_notice::FdaNoticeBranch`). 제목·버튼·표시 여부는 갈래와
관계없이 같다.

| 갈래 | 조건 | 문단 |
|---|---|---|
| `Never` | 보유한 기록이 없다 | 시스템 설정에서 직접 추가하라는 기본 문단(`fda_never`) |
| `Stale` | 보유했고, 그 뒤 앱의 서명 해시가 바뀌었다(업데이트·재빌드) | 목록의 항목이 켜져 보이거나 사라졌을 수 있다는 문단(`fda_stale`), 지우고 다시 추가하는 번호 2단계 목록(`fda_stale_step_remove`·`fda_stale_step_add`), 그래도 실패하면 종료 후 다시 열라는 줄(`fda_stale_retry`). 재시작이 꼭 필요하다고 단정하지 않는다 |
| `Revoked` | 보유했고 해시가 같은데 꺼졌다 | Tasty 밖에서 꺼졌으니 다시 켜고, `tccutil reset` 등으로 목록에서 사라졌으면 다시 추가하라는 문단(`fda_revoked`) |

`Stale`과 `Revoked`는 원인 문단 뒤에 "무엇을 덮는지" 문단(`fda_covers`)을 붙인다. 문단 순서와
조립은 `permission_notice_paragraphs`·`permission_notice_body`가 정하고 본체와 갤러리가 함께
쓴다. 서명 문단(`self_built`)은 갈래와 관계없이 모든 안내의 맨 끝에 보조 문단으로 넣는다.

본문의 모양은 안내 모달(`tasty_ui_widgets::info_modal`)의 강조 표기로 정한다.

| 문단 | 표기 | 모양 |
|---|---|---|
| 일반 문단 | 없음 | 본문 크기 · text-secondary, 문단 사이 `info-modal-para-gap` |
| 번호 목록 항목 | 문단 머리 `1. ` | 본문 크기 · text-primary. 번호는 줄 머리에, 글은 `space-xl` 들여쓰기에 맞춰 감싼다. 이어진 항목 사이는 `space-xs` |
| 보조 문단 | 문단 머리 `> ` | caption 크기 · text-muted |

번호와 `> `는 번역 문자열이 아니라 `permission_notice_body`가 붙인다.
문단 안의 명령(`` `…` ``)은 UI 문장의 code run 과 같다. 그 문단과 같은 크기의 mono 글자, `ui-code-bg` 채움, 좌우 `ui-code-padding-x`, `ui-code-radius` 반경이며 명령 안에서는 줄을 바꾸지 않는다.

**갈래를 고르는 근거** — FDA 보유를 관측하면 그때의 자기 서명 해시(cdhash)를 데이터 홈의
`macos-fda-grant` 파일에 `cdhash=<16진>` 한 줄로 남긴다. 거부로 보이면 이 기록과 지금 해시를
비교한다. 기록이 없으면 `Never`, 해시가 다르면 `Stale`, 같으면 `Revoked`다. 어느 쪽 해시든
얻지 못했으면 `Stale`로 본다. `Stale`의 처방(지우고 다시 추가)은 밖에서 꺼진 경우도 고치지만
반대는 아니기 때문이다. 보유·판단 불가로 보이면 FDA 문단은 `Never`다.

- 기록은 보유를 관측했고 값이 달라질 때만 쓴다. 거부·판단 불가 관측은 기록을 지우지 않는다.
  지우면 거부가 이어지는 다음 부팅에서 원인을 가를 수 없다.
- 기록은 안내를 억제하지 않는다. 안내를 띄울지는 아래처럼 매 부팅 현재 상태로 정하고, 기록은
  문단과 설정 탭 처방 줄을 고르는 데만 쓴다. 사용자 설정(`config.toml`)에는 넣지 않는다.
- 해시는 `codesign`을 띄우지 않고 Security.framework(`SecCodeCopySelf` →
  `SecCodeCopyStaticCode` → `SecCodeCopySigningInformation`의 `kSecCodeInfoUnique`)로 한 번 구한다.
- 판정은 측정할 때(`refresh_permission_snapshot`) 한 번 하고 결과를 보관한다.
  `macos_permissions::notice_inputs()`는 보관된 갈래를 읽기만 한다.
- 판정과 기록 형식은 `tasty_platform` 내부 `macos_fda_history`의 순수 함수라 macOS가 아닌
  환경에서도 시험한다. 파일 접근과 해시 취득은 macOS GUI 빌드에만 있다. macOS가 아니면 갈래는
  언제나 `Never`다.


**실 기기 확인 범위** — 2026-10-08 macOS 27.0.1 실 기기(arm64, ad-hoc 서명 `target/debug/tasty`)에서 debug 빌드를 실제 실행해 확인한 것:

- FDA를 가진 앱의 자식으로 부팅하면 기록 파일이 `cdhash=8df309b5a1c9ba05454791f4388afe017c5adada`로 채워졌고 `codesign -dvvv`의 `CDHash`와 같았다. Security.framework 링크와 기본 플래그(0)의 `kSecCodeInfoUnique` 취득이 동작한다.
- launchd로 띄워 자기 자신이 TCC 판정 대상이 되게 하면 프로브가 거부(`Denied`)를 봤다. 시스템 경로 TCC.db는 FDA 없이 열리지 않는다.
- 그 상태에서 기록 해시가 같으면 `Revoked`, 기록 해시를 다른 값으로 바꾸면 `Stale`, 기록을 지우면 `Never`가 골라졌다. `Revoked` 안내 모달에 다시 켜기와 `tccutil reset` 뒤 다시 추가 처방 문단이 보였다.

재빌드로 cdhash가 바뀐 Tasty.app에서 TCC가 실제로 권한을 거두는지, 시스템 설정 토글·`tccutil reset` 뒤에 각 갈래가 보이는지는 실 기기에서 확인하지 않았다. 위 확인은 기록 파일을 바꿔 갈래를 고른 것이라 TCC의 실제 동작은 포함하지 않는다.
배포 DMG도 직접 빌드도 ad-hoc 서명이라 앱이 둘을 가를 수 없으므로 서명 문단은 모든 안내에 넣는다.

**안내 문구가 지켜야 할 것** — FDA 는 파일 접근 프롬프트만 없앤다. **Automation(다른 앱 제어) · 화면 기록 · 손쉬운 사용은 FDA 와 별개 TCC 서비스라 그대로 남는다.** 문구가 "모든 프롬프트가 사라진다" 로 읽히면 안 된다. 또 배포 DMG 와 직접 빌드 모두 ad-hoc 서명이라 업데이트·재빌드마다 다른 앱으로 인식돼 FDA 행이 더 이상 적용되지 않는다. 서명 문단은 이 사실을 배포본 사용자도 자기 이야기로 읽게 쓰고, 직접 빌드하는 사용자에게는 `Tasty Dev` 인증서 서명을 권한다([build.md](../../dev-guide/build.md) 참조).

### 부팅 권한 안내

부팅할 때 확인할 수 있는 권한 중 하나라도 허용되지 않았으면 InfoModal을 띄운다
(`should_show_permission_notice`). 판단에 쓰는 값은 Full Disk Access 추정 결과와 화면
기록 승인 여부 두 가지다. 모달의 [권한 설정 열기]는 OS 패널이 아니라 Tasty의 권한
화면(설정 > 일반 > 권한)을 연다. 권한 확인과 요청을 한자리에 모으는 것이 목적이고,
안내는 그 화면으로 가는 입구 역할을 한다. Full Disk Access는 결국 시스템 설정에서
켜야 하지만 그 링크도 권한 화면 안에 있다(아래 "설정 탭").

안내 본문은 여섯 가지 안내 메시지 중 유일하게 강조 표기를 쓴다. 문단 도입부(`**…**`)와
설정 경로(`*…*`)는 `text-primary`로, 명령(`` `…` ``)은 고정폭 글꼴과 `surface-raised`
배경으로 그린다. 표기는 번역 파일 `[macos_permissions.notice]`의 문단 문자열 안에 있고
info modal 셸이 해석한다. 셸의 크기·버튼 배치와 시안 대응은
[디자인·갤러리 매핑](../../design/systems/design-gallery-mapping.md#settings--general--permissions-macos)에 있다.

버튼을 눌렀을 때 창이 두 단계를 거쳐 열리는 것은 의도한 구조다. 팝업을 그리는 코드는
`MainViewState`만 가지고 있어 winit 이벤트 루프에 접근할 수 없다. 그래서
`dialogs.permission_settings_requested`만 표시해 두고, App 계층의
`dispatch_pending_info_modal_requests`가 프레임 시작에 이를 읽어 `AppEvent::OpenSettings`를
보낸다. 파일 핸들러 피커의 결과 처리와 같은 방식이다. 진입 탭은 L1 `General`과 L2
`MacosPermissions`를 함께 지정한다. L1만 맞추면 권한 화면이 아니라 일반 탭의 기본 화면이
열리는데, 창이 열린 것은 마찬가지여서 잘못된 것을 알아채기 어렵다.

설정 창이 이미 열려 있으면 새 창을 요청하지 않고 그 창의 탭을 바꾼 뒤 포커스를 준다.
`open_settings_modal`은 모달이 이미 있으면 바로 반환하므로, 이때 탭 지정 플래그를 세우면
쓰이지 않은 채 남아 다음에 설정 창을 열 때 엉뚱하게 권한 탭이 열린다.

버튼을 눌러도 안내는 닫히지 않는다. 설정 창은 별도 창이라 안내를 가릴 수 있고, 그 창을
닫은 뒤에도 무엇을 왜 허용해야 하는지 다시 읽을 수 있어야 한다. [확인]을 눌러야 안내가
닫힌다.

Full Disk Access만 보고 판단하지 않는 이유는, 그 권한을 이미 가진 사용자가 화면 기록이
허용되지 않았어도 아무 안내를 받지 못하기 때문이다. 화면 기록도 한 번 거부하면 앱이 다시
물을 수 없고 시스템 설정에서만 되돌릴 수 있어 같은 문제를 갖는다.

판단에 넣을 수 있는 권한과 그렇지 않은 권한이 갈린다.

- 넣는다: Full Disk Access(경로 접근으로 추정), 화면 기록(`CGPreflightScreenCaptureAccess`로 프롬프트 없이 조회)
- 넣을 수 없다: 파일 폴더(다운로드·문서·데스크탑·볼륨). 파일 계열 TCC에는 preflight API가 없고, 상태를 알아보는 유일한 방법인 `read_dir`이 미결정 상태에서는 곧바로 프롬프트를 띄운다(위 "권한 요청 방법"). 상태를 확인하는 행위 자체가 사용자가 요청하지 않은 프롬프트를 띄우는 일이 된다.
- 넣지 않는다: 손쉬운 사용. release에는 이 권한을 쓰는 코드가 없어(위 "손쉬운 사용") 켤 이유가 없는 항목으로 안내하게 된다.

그래서 안내가 닿지 않는 경우가 하나 남는다. Full Disk Access와 화면 기록을 모두
허용했지만 파일 폴더를 한 번도 요청하지 않은 사용자에게는 안내가 뜨지 않는다. 이 빈틈을
없애려면 파일 폴더 상태를 확인해야 하고, 확인하는 순간 프롬프트가 뜬다.

Full Disk Access가 `Unknown`인 경우는 허용되지 않은 것으로 보지 않는다. 이 값은 경로
접근으로 추정한 것이라 macOS가 그 경로를 옮기면 근거 자체가 사라진다. 그것을 미승인으로
세면 권한을 가진 사용자 전원에게 부팅마다 안내가 뜨는데, 안내에는 "다시 보지 않기"가
없어 벗어날 방법이 없다. 다만 화면 기록은 확실히 확인할 수 있으므로, Full Disk Access가
`Unknown`이어도 화면 기록이 허용되지 않았으면 안내를 띄운다.

### 설정 탭 (일반 > 권한)

macOS에서만 보이며, 권한 요청이 시작되는 유일한 화면이다. 위에서부터 상태 행(전체 디스크
접근 권한, 화면 기록, 폴더 접근, debug 빌드에서는 `debug` 태그가 붙은 손쉬운 사용(키 입력 전달)),
상태를 언제 확인했는지 밝히는 설명 줄, [모든 권한 요청하기] 버튼과 그 설명 줄로 이루어진다. 각
행은 왼쪽에 이름과 (있으면) 물음표 도움말·보조 줄, 오른쪽에 상태 글리프와 상태 단어를 둔다. Full Disk Access 행에만
[시스템 설정 열기] 버튼이 있다. 앱이 요청할 수 없는 권한이라 요청 버튼 대신 그 행에서 시스템
설정으로 보낸다. 창이 좁으면 상태와 버튼은 다음 줄 오른쪽으로 내려간다. 부팅 안내를 그냥
닫았더라도 여기서 현재 상태를 볼 수 있다. 화면은 `tasty_ui_widgets::mac_permissions`로
그리며 갤러리 예제도 같은 함수를 부른다.

Full Disk Access 행은 허용됨, 허용 안 됨, 확인 불가 세 가지를 그대로 보여준다. 판단할
근거가 없는 상태를 "허용 안 됨"으로 적으면 이미 허용한 사용자에게 잘못 안내하게 된다.
보유했다가 잃은 경우에도 다섯 번째 상태나 별도 칩을 만들지 않는다. 상태는 "허용 안 됨" 그대로
두고 행 이름 아래 보조 줄에 갈래별 처방을 적는다. 행 버튼은 같다. 위 갈래와 같은
`notice_inputs()`를 읽고, 키는 `macos_permission_notice::fda_settings_detail_key`가 고른다.

| 갈래 | 보조 줄 |
|---|---|
| `Stale` | 시스템 설정의 항목이 켜져 보여도 더 이상 적용되지 않으니 목록에서 지우고 다시 추가하라(`full_disk_access_stale_detail`) |
| `Revoked` | Tasty 밖에서 꺼졌으니 목록에서 다시 켜고, 목록에서 사라졌으면(`tccutil reset` 뒤 등) + 로 다시 추가하라(`full_disk_access_revoked_detail`). 한 caption 문단이며 둘째 줄 요소를 두지 않는다 |
| `Never` | 없음 |

보조 줄은 안내 본문과 다른 키다. 안내 본문의 `fda_revoked`도 같은 재추가 처방을 담지만 문장이 다르다(경로를 강조하고 `tccutil reset`을 code run 으로 보이며, 다시 추가한 뒤 켜라고까지 적는다). 설정 행은 강조 표기를 해석하지 않는 평문 caption이라 키를 나눠 둔다.

폴더 접근 행은 보조 줄에 대상 폴더(다운로드 · 문서 · 데스크탑 · 볼륨)를 적는다. 폴더를
확인하는 동작 자체가 프롬프트를 띄우므로 상태는 "자동 확인 불가"로 표시하고, 그 이유는 행
이름 옆 물음표 도움말로 밝힌다. 전체 디스크 접근 권한 상태가 추정이라 틀릴 수 있다는 사실도
행 이름에 붙이지 않고 그 행의 물음표 도움말로 전달한다.

중복 요청을 막기 위해 요청 중에는 [모든 권한 요청하기] 버튼을 비활성화하고, 버튼 아래
설명 줄 자리에 스피너와 진행 문구를 표시한다. 플랫폼 코드에서도 진행 중인 요청이 있으면 새 요청을 받지 않는다.

화면은 `permission_snapshot()`으로 보관된 상태를 읽는다. 권한 조회에는 파일 접근과
TCC 데몬 응답 대기가 필요하므로 매 프레임 조회하면 설정 창이 멈출 수 있다.
`refresh_permission_snapshot()`은 다음 시점에 표시용 상태를 갱신한다.

- 부팅할 때 (`wants_permission_notice`): 시작 안내 여부를 판단할 때도 같은 값을 쓴다.
- 권한 탭에 들어올 때 (`apply_l2_select`).
- 설정 창에 포커스가 돌아올 때 (`SettingsView::handle_event`): 시스템 설정에서 바꾼 권한을 반영한다.
- 권한 요청이 끝날 때 (`request_all_permissions`): 완료를 알리기 전에 결과를 반영한다.

아직 보관된 값이 없다면 `permission_snapshot()`이 한 번 측정한다. 정상적인 시작 과정에서는
부팅 때 먼저 측정하므로 화면을 그릴 때 다시 측정하지 않는다.

안내 여부를 판단하는 함수는 모든 GUI 빌드에 포함한다. Linux와 Windows에서는 실제 권한을
조회하지 않고 Full Disk Access를 `Unknown`, 화면 기록을 허용 상태로 반환하므로 안내가 뜨지 않는다.

캡처와 키 주입 경로는 이 스냅샷을 쓰지 않는다. `screen_capture.rs`의
`screen_recording_authorized()`와 `input_source.rs`의 `accessibility_trusted()`는 동작
직전에 직접 측정하도록 되어 있다(위 "화면 기록"과 "주입 시점의 소비"). 표시용 캐시로
묶으면 실제 동작 여부를 낡은 값으로 판단하게 된다.

손쉬운 사용 상태 행은 debug 빌드에만 있다. release에는 이 권한을 쓰는 코드가 없어
프롬프트도 띄우지 않으므로, 행을 남기면 켤 이유도 끌 이유도 없는 항목이 계속 "미승인"으로
보인다. debug 빌드에서는 키 주입을 검증할 때 승인 상태를 확인할 자리가 필요해 그대로 둔다.

### 프롬프트 본문 설명 문구

번들 `Info.plist` 의 `NS*UsageDescription` 키가 프롬프트 본문에 그대로 표시된다. 키가 없으면 이유 없는 프롬프트가 뜨고, 일괄 요청처럼 여러 개를 연달아 띄우면 그 문제가 커진다. 키 목록과 문구 정책은 [build.md](../../dev-guide/build.md) 의 배포 패키징 절 참조.

### 플랫폼 격리

목록 결정 로직(`prewarm_targets`)은 파일시스템 조회를 `FsProbe` 로 추상화한 **순수 함수**라 전 플랫폼에서 컴파일·유닛테스트된다. 실제 파일 접근부(`RealFs`·`request_all_permissions`)만 `#[cfg(all(target_os = "macos", feature = "gui"))]` 로 좁힌다. cfg 로 잘린 코드는 rustc 가 타입체크 전에 걷어내므로, 로직을 순수부에 몰아둘수록 다른 플랫폼에서도 더 많은 로직을 검증할 수 있다. headless 는 프롬프트를 띄울 GUI 주체가 없으므로 macOS 여도 돌지 않는다. 비-macOS/headless 에는 같은 이름의 no-op 이 노출돼 호출부에 `#[cfg]` 가 흩어지지 않는다.

## 비-목표 (Out of scope)

**미리 일괄 요청하지 않는 권한** — 접근할 앱마다 승인이 필요하다:

- **다른 앱의 데이터** (`kTCCServiceSystemPolicyAppData`, macOS 14+) — `~/Library/Application Support/<앱>`, `~/Library/Containers/<번들ID>` 처럼 **대상 앱 디렉터리 단위로 개별 프롬프트**가 뜬다. 존재하는 디렉터리를 전부 순회하면 프롬프트가 수십 개 뜨므로 현실적 선택지가 아니다. Full Disk Access 로 덮이며, 그 안내는 위 "Full Disk Access" 절이 담당한다.
- **다른 앱 제어** (Automation / Apple Events) — 대상 앱 단위. 셸에서 `osascript` 를 쓸 때 발생하며 사전 열거 불가. **FDA 로도 덮이지 않는다** — FDA 는 파일 접근 서비스이고 Automation 은 완전히 별개 서비스라, FDA 를 줘도 대상 앱별 승인은 계속 요구된다. tasty는 이를 미리 일괄 요청하지 않는다. tasty 자신은 Apple Events 를 보내지 않는다.

**해당 없음** (레포 전체 확인): 카메라·마이크·위치(`AVCaptureDevice`/`AVAudioSession`/`CLLocationManager` 미사용), OS 알림 센터(tasty 는 자체 토스트만 쓴다), Local Network(IPC 서버는 loopback bind — 대상 제외), 드래그앤드롭·네이티브 파일 피커(사용자가 그 자리에서 명시적으로 고른 파일이라 macOS 가 별도 동의로 취급, 프롬프트 없음).

## Acceptance Criteria

- Given `~/Documents` 가 없는 홈 When 요청 목록을 결정 Then 그 경로가 목록에서 빠진다
- Given `/Volumes` 에 마운트 2 개 When 목록을 결정 Then 홈 폴더 뒤에, 경로 정렬 순으로 붙는다
- Given 마운트가 하나도 없음 When 목록을 결정 Then 볼륨 항목이 하나도 없다
- Given macOS 에서 TCC 승인을 초기화 When Tasty 실행 Then 권한 프롬프트가 하나도 뜨지 않는다
- Given 같은 상태 When 설정 > 일반 > 권한 의 [모든 권한 요청하기] 클릭 Then 프롬프트가 순차로 뜨고, 떠 있는 동안에도 설정 창이 그려지고 클릭·스크롤이 반응한다
- Given 요청이 도는 중 When 버튼을 다시 누름 Then 두 번째 시퀀스가 시작되지 않는다
- Given 요청이 끝남 When 화면을 봄 Then 상태 표시가 갱신돼 있다
- Given 프롬프트를 모두 허용 When 터미널에서 `ls ~/Downloads; ls ~/Documents; ls ~/Desktop` Then 추가 프롬프트가 뜨지 않는다
- Given 화면 기록 권한 미결정 When 요청 Then 파일 프롬프트들 **뒤에** 화면 기록 프롬프트가 뜬다
- Given 화면 기록 권한을 거부한 뒤 다시 요청 Then 프롬프트가 다시 뜨지 않는다(무한 재요청 없음)
- Given release 빌드 When 요청 Then 손쉬운 사용 프롬프트는 뜨지 않는다
- Given FDA 프로브 경로가 열리고 화면 기록도 승인됨 When 부팅 Then 안내를 띄우지 않는다
- Given FDA 프로브가 `PermissionDenied` 로 거부됨 When 부팅 Then 안내를 띄운다 — 몇 번째 부팅인지는 보지 않는다
- Given FDA 는 허용됐지만 화면 기록이 미승인 When 부팅 Then 안내를 띄운다
- Given FDA 프로브 경로가 하나도 존재하지 않음(`NotFound`) + 화면 기록 승인 When 부팅 Then 판정 불가로 보고 안내를 띄우지 않는다
- Given FDA 판정 불가 + 화면 기록 미승인 When 부팅 Then 안내를 띄운다 — 화면 기록 쪽 근거는 확실하다
- Given FDA 와 화면 기록을 모두 허용했지만 파일 폴더는 한 번도 요청하지 않음 When 부팅 Then 안내가 뜨지 않는다 (폴더 접근 권한은 확인 과정에서 권한 요청 대화상자가 뜰 수 있어 시작 안내의 판단에서 제외한다)
- Given 안내를 본 뒤 권한을 부여 When 재부팅 Then 안내가 뜨지 않는다
- Given 안내를 본 뒤 권한을 주지 않음 When 재부팅 Then 안내가 다시 뜬다
- Given ad-hoc 서명 빌드에 FDA 를 준 뒤 재빌드 When 부팅 Then 승인이 초기화돼 안내가 다시 뜬다
- Given FDA 를 보유한 부팅이 기록을 남긴 뒤 업데이트·재빌드로 서명 해시가 바뀌고 FDA 가 거부로 보임 When 부팅 Then 안내의 FDA 문단이 `Stale`이고 지우고 다시 추가하는 단계가 번호 목록 1·2로 보이며, 설정 탭 FDA 행은 "허용 안 됨" 아래에 "켜져 보여도 적용되지 않는다"는 처방 줄이 붙는다
- Given FDA 를 보유한 부팅이 기록을 남긴 뒤 서명 해시는 같은데 FDA 가 거부로 보임 When 부팅 Then 안내의 FDA 문단이 `Revoked`(다시 켜기, 목록에서 사라졌으면 다시 추가)이고, 설정 탭 FDA 행은 "허용 안 됨" 아래 한 caption 문단에 다시 켜라는 처방과 목록에서 사라졌으면 + 로 다시 추가하라는 문장이 붙는다(별도 칩이나 상태는 없다)
- Given 보유 기록이 없음 When FDA 가 거부로 보이는 부팅 Then 안내의 FDA 문단은 `Never`(직접 추가)이고 설정 탭 FDA 행에는 처방 줄이 없다
- Given 어느 갈래든 When 부팅 안내를 끝까지 스크롤 Then 마지막 문단이 서명 문단이고 caption 크기 · text-muted 로 보인다
- Given 보유 기록이 있음 When FDA 가 거부로 보이는 부팅이 이어짐 Then 기록은 지워지지 않고 같은 갈래가 매 부팅 유지된다. 안내 표시 여부는 기록과 관계없다
- Given 권한 탭이 열려 있음 When 그 위에서 마우스를 움직여 repaint 가 반복됨 Then TCC 측정은 한 번도 더 일어나지 않는다
- Given 권한 탭이 열린 설정 창 When 시스템 설정에 다녀와 그 창에 포커스가 돌아옴 Then 상태를 다시 재서 표시가 갱신된다
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
