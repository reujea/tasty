# macOS 키 주입 측정의 권한

macOS에서 OS가 만든 키 입력으로 측정하려면 손쉬운 사용 권한이 필요하다. `CGEvent`를 게시하는 스크립트, `osascript`의 System Events, debug의 `surface.raw_key`가 여기에 해당한다. 이 문서는 원격 child가 그런 측정을 할 때 권한을 확인하고 요청하는 절차를 설명한다. 권한 자체의 설계는 [macOS 권한](../features/macos-permissions/index.md)에 있다.

## 권한을 받는 앱

macOS는 셸에서 실행한 프로세스의 보호 리소스 접근을 그 셸을 띄운 GUI 앱(responsible process)의 권한으로 판단한다([macOS 권한](../features/macos-permissions/index.md)). 원격 attach mirror로 띄운 child는 대상 Mac에서 실행 중인 Tasty 안의 셸에서 동작한다. 따라서 child가 실행한 키 입력 스크립트는 그 Tasty.app의 손쉬운 사용 권한으로 판정된다. 격리 debug 인스턴스의 권한이나 스크립트 파일 자체의 권한으로 판정하지 않는다.

그래서 측정은 [macOS 키 입력 도우미](../dev-guide/build.md#macos-키-입력-도우미)로 한다. 도우미를 `open`으로 띄우면 도우미 앱이 책임 프로세스가 되므로, Tasty.app이 아니라 도우미 하나에만 손쉬운 사용을 허용한다.

## adhoc 서명 앱은 빌드마다 권한이 풀린다

배포 DMG와 `scripts/build-macos-dmg.sh`로 만든 앱은 adhoc 서명이다([빌드](../dev-guide/build.md)). 서명 방식은 `codesign -dv <앱 경로>`로 확인한다. adhoc 서명 앱은 출력에 다음 값이 나온다.

- `Signature=adhoc`
- `flags=0x2(adhoc)`
- `TeamIdentifier=not set`

adhoc 서명의 designated requirement는 cdhash뿐이다. 그래서 앱을 다시 빌드하면 macOS가 다른 앱으로 보고 손쉬운 사용 허용을 적용하지 않는다. 사용자가 그 Mac의 Tasty를 다시 빌드했다면 측정 전에 권한을 다시 확인한다. 직접 빌드한 앱의 권한을 유지하는 `Tasty Dev` 인증서 서명은 [빌드](../dev-guide/build.md)에 있다. `Tasty Dev`로 서명한 도우미 앱은 지정 요구가 `identifier "<번들 ID>" and certificate leaf = H"<인증서 해시>"`이고, Info.plist를 바꿔 다시 빌드해도 지정 요구가 같다(측정함). 도우미 앱은 소스를 바꿔 다시 빌드·설치해 CDHash가 바뀐 뒤에도, 다시 허용하지 않은 상태에서 두 조회 값이 true였다(측정함). 재빌드 뒤 실제 키 게시까지 되는지는 미측정이다.

## 권한 확인

측정 전에 아래 두 값을 조회한다. 조회는 대화상자를 띄우지 않는다.

```swift
// perm.swift: /usr/bin/swift perm.swift status
import ApplicationServices
import CoreGraphics
print("CGPreflightPostEventAccess=\(CGPreflightPostEventAccess()) AXIsProcessTrusted=\(AXIsProcessTrusted())")
```

`CGPreflightPostEventAccess()`가 false이면 게시한 키 이벤트가 버려진다. 호출이 실패로 돌아오지 않으므로 키 입력 결과를 보고 판정하지 말고 이 값으로 판정한다. false이면 측정을 시작하지 않고 멈춰 보고한다.

키를 보내는 스크립트는 보내기 직전에 `NSWorkspace.shared.frontmostApplication`의 PID가 격리 인스턴스의 PID인지 확인한다. 다르면 보내지 않는다. 합성 키는 그 순간 OS 포커스를 가진 앱이 받기 때문이다.

## 권한 요청 함수

권한이 없을 때 대화상자를 띄우고 손쉬운 사용 목록에 앱을 올리는 함수는 두 가지다. 키 입력 도우미는 `open`으로 띄웠다고 판정한 경우에만 `request-access`와 권한 없이 `key` 단계가 든 명령 파일에서 이 두 함수를 부른다([macOS 키 입력 도우미](../dev-guide/build.md#macos-키-입력-도우미)). 판정은 부모가 launchd(PID 1)인지로 한다. 그래서 부모 셸이 먼저 끝나 launchd로 옮겨진 직접 실행(`nohup … &` 뒤 셸 종료, 이중 fork, `setsid`·`disown`)도 `open` 실행으로 판정되고, 그때는 요청이 셸을 띄운 앱에 붙을 수 있다. 직접 실행은 포그라운드로 한다. 셸에서 실행 파일을 직접 실행하면 요청이 셸을 띄운 앱에 붙으므로, 직접 실행은 조회 전용이다. 직접 실행에서는 `request-access`와 명령 파일(빈 파일 포함)을 권한과 관계없이 종료 코드 2로 거부하고 요청하지 않는다. 그 밖의 앱에서는 사용자가 요청한 경우에만 호출한다. 대화상자는 사용자가 직접 허용하며, child는 누르거나 닫지 않는다. `tccutil reset`은 도우미 요청 창을 다시 띄울 때 도우미 번들 ID(`com.zilhak.tasty.input-helper`)에만 쓴다. 사용자 Tasty.app 항목은 초기화하지 않는다.

- `CGRequestPostEventAccess()`: 이벤트 게시 권한을 요청한다.
- `AXIsProcessTrustedWithOptions`에 `kAXTrustedCheckOptionPrompt: true`를 넘긴다. 손쉬운 사용 권한을 요청한다.

```swift
// reqpost
print("CGRequestPostEventAccess=\(CGRequestPostEventAccess())")
// axprompt
let opts = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
print("AXIsProcessTrustedWithOptions(prompt)=\(AXIsProcessTrustedWithOptions(opts))")
```

두 함수는 권한이 아직 없으면 false를 돌려준다. 호출 전후에 위 두 조회 값을 기록한다. 대화상자가 떴는지는 `screencapture -x`로 화면을 찍어 확인한다. 캡처에는 사용자 화면이 찍히므로 파일을 그 Mac의 작업 폴더에만 두고 판정한 뒤 지운다. 보고서에는 판정 결과만 적는다. `screencapture`에는 화면 기록 권한도 필요하다([스크린샷](screenshot-methods.md)). 원격 데스크톱처럼 화면 전체를 덮는 창이 있으면 대화상자가 그 뒤나 다른 Space에 뜰 수 있어 캡처로 판정할 수 없다. 손쉬운 사용 목록의 항목은 시스템 설정을 열어야 보이므로 child가 확인하지 않는다.

사용자가 허용하면 이미 실행 중인 셸에서 다시 조회한 값이 true가 된다. 셸과 Tasty.app을 다시 시작하지 않아도 된다(측정함).

## 측정 경로: 키 입력 도우미

키 입력은 [macOS 키 입력 도우미](../dev-guide/build.md#macos-키-입력-도우미)로 보낸다. 설치, 허용 대상, 호출 형식, 결과 파일, 종료 코드, 합성 입력의 한계, 보고 규칙은 그 절에 있다.

- 도우미는 실행할 때마다 `AXIsProcessTrusted`와 `CGPreflightPostEventAccess`를 결과 파일에 적는다. 게시 권한이 없으면 어떤 단계도 실행하지 않고 종료 코드 3으로 끝난다. `open`으로 띄웠고 `key` 단계가 있었으면 끝내기 전에 위 두 요청 함수를 한 번 부른다. 위 "권한 확인"의 판정을 도우미가 대신한다.
- 처음 쓰는 Mac에서는 `request-access`로 요청 창을 띄워 도우미를 손쉬운 사용 목록에 올린다. 창은 앱마다 한 번만 뜨고, 다시 띄우려면 `tccutil reset Accessibility com.zilhak.tasty.input-helper` 뒤에 실행한다.
- 키를 보내기 전에 `require_front <격리 인스턴스 PID>` 단계를 넣는다. 앞 앱이 다르면 종료 코드 5로 끝나고 키를 보내지 않는다.
- 허용 전에는 `open`으로 띄운 경우와 셸에서 실행 파일을 직접 실행한 경우 모두 두 값이 false다. 도우미 하나만 허용한 뒤에는 `open`으로 띄우면 `AXIsProcessTrusted`·`CGPreflightPostEventAccess`가 true이고 종료 코드 0이다. 같은 실행 파일을 Tasty 셸에서 직접 실행하면 false·false이고 종료 코드 3이다(측정함). `open` 경로에서는 도우미가, 직접 실행에서는 셸을 띄운 Tasty.app이 책임 프로세스다.

Terminal.app을 책임 프로세스로 쓰는 경로는 쓰지 않는다. 스크립트를 띄운 Terminal 창을 AppleScript 없이 닫을 방법이 없고, 그 경로의 단계는 측정한 적이 없다.
