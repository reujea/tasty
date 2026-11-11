# macOS 키 주입 측정의 권한

macOS에서 OS가 만든 키 입력으로 측정하려면 손쉬운 사용 권한이 필요하다. `CGEvent`를 게시하는 스크립트, `osascript`의 System Events, debug의 `surface.raw_key`가 여기에 해당한다. 이 문서는 원격 child가 그런 측정을 할 때 권한을 확인하고 요청하는 절차를 설명한다. 권한 자체의 설계는 [macOS 권한](../features/macos-permissions/index.md)에 있다.

## 권한을 받는 앱

macOS는 셸에서 실행한 프로세스의 보호 리소스 접근을 그 셸을 띄운 GUI 앱(responsible process)의 권한으로 판단한다([macOS 권한](../features/macos-permissions/index.md)). 원격 attach mirror로 띄운 child는 대상 Mac에서 실행 중인 Tasty 안의 셸에서 동작한다. 따라서 child가 실행한 키 입력 스크립트는 그 Tasty.app의 손쉬운 사용 권한으로 판정된다. 격리 debug 인스턴스의 권한이나 스크립트 파일 자체의 권한으로 판정하지 않는다.

## adhoc 서명 앱은 빌드마다 권한이 풀린다

배포 DMG와 `scripts/build-macos-dmg.sh`로 만든 앱은 adhoc 서명이다([빌드](../dev-guide/build.md)). 서명 방식은 `codesign -dv <앱 경로>`로 확인한다. 2026-11-11 측정 대상 Mac의 앱은 다음과 같았다.

- `Signature=adhoc`
- `flags=0x2(adhoc)`
- `TeamIdentifier=not set`

adhoc 서명의 designated requirement는 cdhash뿐이다. 그래서 앱을 다시 빌드하면 macOS가 다른 앱으로 보고 손쉬운 사용 허용을 적용하지 않는다. 사용자가 그 Mac의 Tasty를 다시 빌드했다면 측정 전에 권한을 다시 확인한다. 직접 빌드한 앱의 권한을 유지하는 `Tasty Dev` 인증서 서명은 [빌드](../dev-guide/build.md)에 있다. 이 인증서로 서명한 앱에서 측정 권한이 유지되는지는 미측정이다.

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

권한이 없을 때 대화상자를 띄우고 손쉬운 사용 목록에 앱을 올리는 함수는 두 가지다. 사용자가 요청한 경우에만 호출한다. `tccutil reset`처럼 설정을 바꾸는 명령은 쓰지 않는다. 대화상자는 사용자가 직접 허용하며, child는 누르거나 닫지 않는다.

- `CGRequestPostEventAccess()`: 이벤트 게시 권한을 요청한다.
- `AXIsProcessTrustedWithOptions`에 `kAXTrustedCheckOptionPrompt: true`를 넘긴다. 손쉬운 사용 권한을 요청한다.

```swift
// reqpost
print("CGRequestPostEventAccess=\(CGRequestPostEventAccess())")
// axprompt
let opts = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
print("AXIsProcessTrustedWithOptions(prompt)=\(AXIsProcessTrustedWithOptions(opts))")
```

호출 전후에 위 두 조회 값을 기록한다. 대화상자가 떴는지는 `screencapture -x`로 화면을 찍어 확인한다. 캡처에는 사용자 화면이 찍히므로 파일을 그 Mac의 작업 폴더에만 두고 판정한 뒤 지운다. 보고서에는 판정 결과만 적는다. `screencapture`에는 화면 기록 권한도 필요하다([스크린샷](screenshot-methods.md)).

2026-11-11 측정에서 확인한 사실과 확인하지 못한 사실은 다음과 같다.

- 두 함수 모두 false를 돌려줬다. 호출 뒤 두 조회 값도 false였다.
- 3초 뒤 찍은 캡처에서는 대화상자를 찾지 못했다. 다만 Mac 화면 전체를 원격 데스크톱 창이 덮고 있었다. 그래서 대화상자가 그 창 뒤나 다른 Space에 떴는지는 판정하지 못했다. 원격 데스크톱처럼 화면을 덮는 창이 있으면 캡처로 대화상자를 판정할 수 없다.
- 손쉬운 사용 목록에 어떤 항목이 생겼는지는 미확인이다. 시스템 설정 화면을 열지 않았기 때문이다.
- 사용자가 허용한 뒤 실행 중인 셸에서 값이 바로 true로 바뀌는지, 셸이나 앱을 다시 시작해야 하는지는 미측정이다.

## 대안: Terminal.app이 권한을 받게 한다

Tasty.app을 다시 빌드할 때마다 권한을 다시 받지 않으려면 키 입력 스크립트를 Terminal.app에서 실행한다. 그러면 Terminal.app이 responsible process가 된다. Terminal.app은 Apple이 서명했고 이 저장소에서 다시 빌드하지 않는다. 같은 날 `codesign -dv`로 확인한 값은 `Identifier=com.apple.Terminal`, `TeamIdentifier=not set`이었고 adhoc이 아니었다. 사용자는 Terminal.app의 손쉬운 사용을 한 번만 켜면 된다.

이 절차는 아직 실행하지 않았다. 아래 단계는 모두 미측정이다.

1. 측정 스크립트를 작업 폴더에 둔다. 스크립트는 결과를 작업 폴더의 파일에 쓰고, 마지막에 완료 표지 파일을 만든다.
2. child 셸에서 `open -a Terminal <스크립트>`로 Terminal.app 창에서 실행한다.
3. child는 완료 표지 파일이 생길 때까지 기다렸다가 결과 파일을 읽는다. Terminal 창의 화면은 읽지 않는다.
4. 끝나면 이 측정이 연 Terminal 창만 닫는다. 사용자가 쓰던 Terminal 창은 건드리지 않는다. 창을 닫는 방법은 미측정이다. AppleScript로 Terminal을 제어하면 Automation 권한을 따로 요청하게 된다.

이 경로에서도 권한 확인은 스크립트 안에서 먼저 한다. 키를 보내기 전 포커스 확인도 그대로 한다.
