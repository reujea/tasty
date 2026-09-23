# ADR-0370: macOS·Windows 산출물도 관측 전에 고지 세트를 배선한다 — ADR-0317 의 이행 순서 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: release, packaging, licensing, third-party, macos, dmg, windows, msi, zip, wix, adr-0317
- **Group**: build-release

## Context

[ADR-0317](0317-the-notice-set-is-staged-not-generated.md) 은 고지 세트를 산출물마다 스테이징하기로
하면서 **이행은 검증 가능한 플랫폼부터** 하기로 했다. Linux 넷은 빌더에서 산출물을 열어 확인했고,
Windows `.zip` · macOS `.dmg` 는 "그 빌드 환경이 아니면 산출물을 열 수 없어 의도적으로 미이행" 으로
남겼다(그 ADR 의 대안 D 가 "미이행 플랫폼까지 한 번에 배선한다" 를 기각한 자리). MSI 는 Tasty
자신의 MIT 본문(`wix/License.rtf`)만 넣고 제3자 고지는 넣지 않았다.

그 판단은 번들 제3자 자산이 폰트 하나라는 전제 위에 있었고, 그 전제는 틀렸다 — ADR-0317 의 Context
정정대로 markdown plugin 바이너리가 mermaid(MIT) · highlight.js(BSD-3-Clause) · KaTeX(MIT) 를
임베드한다. 세 산출물(`.dmg` · `.zip` · `.msi`)은 그 plugin 바이너리를 싣고 나가므로, 미이행으로
두는 동안 **알려진 부족을 안고 배포된다.** 미이행의 비용이 폰트 하나의 OFL 에서 라이선스 셋으로
커졌다.

한편 ADR-0317 의 규율은 스스로 "배선하지 않는다" 가 아니라 **"배선을 동작으로 적지 않는다"** 라고
정했다. 관측하지 못한 배선을 미측정으로 적는 한, 배선이 관측에 앞서는 것 자체는 그 규율과
부딪히지 않는다. 이 머신(Linux arm64)에는 macOS 빌더도 PowerShell 도 WiX 도 없어, 이번에도 세
산출물을 열어 볼 방법은 없다.

## Decision

**macOS `.dmg` · Windows `.zip` · Windows `.msi` 에도 고지 세트를 배선하고, 관측 전까지는
문서에 미측정으로 적는다.** ADR-0317 의 "이행은 검증 가능한 플랫폼부터 한다" 조항과 대안 D 의
기각을 이 한 점에서 개정한다.

- **자리** — 산출물의 관례를 따른다(ADR-0317 과 같은 원칙).
  - `.dmg`: `Tasty.app/Contents/Resources/`, `LICENSES/` 하위 경로 유지. 번들의 비-코드 파일
    자리이고 codesign 봉인이 일반 리소스로 덮는 자리다. 스테이징은 **codesign 전**이다 — 뒤에
    번들을 바꾸면 서명이 깨진다.
  - `.zip`: 최상단(`tasty.exe` 옆). Linux `tar.gz` 와 같은 배치다.
  - `.msi`: 설치 디렉토리 최상단, `LICENSES\` 하위 유지. 설치 동의 화면용 `License.rtf` 는 그대로
    둔다 — 그 화면은 RTF 하나를 보여 주는 자리라 세트 전체를 담을 곳이 아니다.
- **확인** — 각 스크립트가 산출물 쪽 트리를 저장소 사본과 **바이트로** 대조하고, 어긋나면 빌드를
  실패시킨다. `.dmg` 는 조립한 `.app` · `hdiutil` 에 넘기는 스테이징 트리 · 만든 이미지를 읽기
  전용으로 붙인 트리 세 곳, `.zip` 은 푼 트리, `.msi` 는 관리 설치(`msiexec /a`)로 푼 트리를 본다.
- **세트를 읽는 법** — `LICENSES/` 를 디렉토리로 읽는다(ADR-0317 의 세트 정의). 셸 두 스크립트는
  공용 라이브러리 `scripts/lib/notice-set.sh` 를, PowerShell 은 같은 규칙의 함수를 쓴다.
  **WiX 만은 파일마다 이름을 적어야 해서** `wix/main.wxs` 에 본문마다 `Component` 가 있고,
  `build-windows.ps1` 이 MSI 를 만들기 전에 `LICENSES/` 의 파일마다 대응 `Source` 가 있는지 보고
  없으면 멈춘다.
- **상태 표기** — `THIRD_PARTY_LICENSES.md` 의 "산출물별 위치" 표와 `docs/dev-guide/build.md#dist-빌드-명령-카탈로그`
  가 세 자리를 "배선했고 열어 본 적은 없다" 로 적는다. 관측한 Linux 넷과 같은 줄로 읽히지 않게
  한다.

**개정하지 않는 것** — ADR-0317 의 나머지는 그대로 유효하다.

- 고지 세트를 생성하지 않고 저장소의 파일을 스테이징한다는 결정과 그 근거.
- Linux 넷의 자리(`tar.gz` 최상단 · deb `/usr/share/doc/tasty/` · rpm `/usr/share/licenses/tasty/`
  · AppImage `usr/share/licenses/tasty/`).
- 릴리스 에셋 업로드가 산출물 동봉을 대신하지 못한다는 판단(그 ADR 의 대안 C).
- 정적 링크 의존 크레이트의 고지를 담지 않는다는 한계와 그 입구(대안 A).

## Consequences

- **얻은 것**: 지원하는 산출물 일곱이 모두 고지 세트를 싣도록 배선됐다. 새 라이선스 본문이
  `LICENSES/` 에 들어오면 셸 두 스크립트 · PowerShell · deb/rpm asset glob · 릴리스 업로드가
  디렉토리를 읽어 저절로 따라간다.
- **잃은 것**: 세 자리는 **관측되지 않은 배선**이다. 확인 코드 자체도 그 빌더에서 돈 적이 없어,
  첫 실행이 확인 코드의 결함을 드러낼 수 있다(예: `msiexec /a` 가 러너 정책에 막히는 경우).
  그때 빌드가 **빨갛게** 멈추는 쪽이지 조용히 통과하는 쪽이 아니다 — 확인이 실패하면 스크립트가
  종료한다.
- **운영 비용 / 유지 부담**: 새 라이선스 본문 하나에 저장소 쪽에서 움직일 자리는 셋이다 —
  `LICENSES/` 의 파일 · `THIRD_PARTY_LICENSES.md` 첫 표의 행 · `wix/main.wxs` 의 `Component` 와
  `ComponentRef`. 셋째는 Windows 빌드가 멈춰서 알려 준다. 둘째는 **재는 채널이 없다**(ADR-0317 의
  재검토 트리거에 적힌 그대로다).

## Alternatives Considered

- **A: ADR-0317 대로 두 자리를 미이행으로 둔다** — 확인 못 한 배선이 문서에서 "돌아간다" 로 읽힐
  위험을 피한다. 안 고른 이유는 그 위험을 상태 표기로 막을 수 있는 반면, 미이행은 **이미 알려진
  부족을 안고 배포되는** 상태이기 때문이다. 비교 대상은 "확인된 배선" 이 아니라 "확인 못 한
  배선 대 확인된 부재" 다.
- **B: WiX `heat` 로 `LICENSES/` 를 수확해 파일 목록을 생성한다** — MSI 도 디렉토리를 읽게 된다.
  안 고른 이유는 cargo-wix 흐름이 정적 `main.wxs` 하나를 쓰는데 거기에 수확 단계와 생성
  fragment 를 끼워야 하고, 이 머신에서 그 결과를 한 번도 볼 수 없기 때문이다. 이름을 적고 빌드
  전에 대조하는 쪽이 같은 누락을 더 적은 부품으로 잡는다.
- **C: `.dmg` 는 볼륨 최상단(앱 옆)에 둔다** — 이미지를 연 사람이 바로 본다. 안 고른 이유는
  드래그 설치가 `.app` 만 복사해 **설치된 앱에는 고지가 없게** 되기 때문이다. 앱 번들 안이면 설치
  후에도 남는다.
- **D: 확인을 스테이징 트리에서만 한다** — Linux AppImage 처럼 패킹 전 트리만 보면 확인 코드가
  단순해진다. `.dmg` · `.msi` 는 산출물을 직접 열 수단(`hdiutil attach` · `msiexec /a`)이 그 빌더에
  기본으로 있어서 산출물 쪽까지 보기로 했다. 확인이 보는 것이 입력이냐 산출물이냐는 티켓의 완료
  조건이 가르는 지점이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `LICENSES/` 의 파일과 `wix/main.wxs` 의 고지 `Source` 가 어긋난다. 좌변 둘은 `ls LICENSES` 와
  `grep "Source='LICENSES" wix/main.wxs` 다. 오늘 이것을 보는 것은 Windows 빌드 스크립트 하나이고,
  레포 쪽 가드는 없다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- macOS 빌더에서 `build-macos-dmg.sh` 가 처음 돈다. 재는 법: 그 로그에 고지 확인 실패가 없는지,
  그리고 만든 `.dmg` 를 붙여 `Tasty.app/Contents/Resources/LICENSES/` 를 본다. 통과하면 표의 그
  칸을 관측 값으로 바꾼다.
- Windows 빌더에서 `build-windows.ps1` 이 처음 돈다. 재는 법: ZIP 을 풀어 최상단을, MSI 를 설치해
  설치 디렉토리를 본다. `msiexec /a` 가 그 러너에서 막히면 MSI 확인의 방식을 다시 정한다.

## References

- 개정 대상: [ADR-0317](0317-the-notice-set-is-staged-not-generated.md) (이행 순서 조항과 대안 D)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- `THIRD_PARTY_LICENSES.md` — 고지 세트의 정의와 산출물별 자리의 정본 표. (결정이 실현된 현재 위치)
- `scripts/lib/notice-set.sh` 의 `stage_notice` · `verify_notice_tree` · `verify_notice_listing` — 셸 쪽 스테이징과 확인. (결정이 실현된 현재 위치)
- `scripts/build-windows.ps1` 의 `Get-NoticeSetFiles` · `Stage-Notice` · `Test-NoticeTree` — PowerShell 쪽. (결정이 실현된 현재 위치)
- `wix/main.wxs` 의 `NoticeLicensesDir` 트리 — MSI 의 고지 자리. (결정이 실현된 현재 위치)
- [`docs/dev-guide/build.md`](../dev-guide/build.md#dist-빌드-명령-카탈로그) — 빌더가 읽는 절차.
