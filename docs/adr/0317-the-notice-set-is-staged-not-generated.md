# ADR-0317: 고지 세트는 생성하지 않고 저장소의 고지 파일을 산출물마다 스테이징한다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: release, packaging, licensing, third-party, linux, appimage, deb, rpm, ci
- **Group**: build-release

## Context

이 저장소는 upstream 프로젝트 넷의 제3자 자산을 번들한다 — 본체가 임베드하는 D2Coding
ligature 폰트(OFL 1.1)와, markdown plugin 바이너리가 임베드하는 mermaid(MIT) · highlight.js
(BSD-3-Clause) · KaTeX(MIT, woff2 폰트 20 개 포함)다. 넷 다 **라이선스 본문(과 저작권 고지)을
배포물과 함께 넣을 것**을 요구한다. 그 의무를 적은 문서
(`THIRD_PARTY_LICENSES.md`)는 폰트가 들어온 커밋(2026-04-26)에 함께 생겼고, 릴리스 체크리스트
절에 "`.github/workflows/` 에 릴리스 자동화가 추가되는 시점에 위 두 파일이 에셋으로
업로드되도록 설정합니다" 라고 적었다.

> **사실 정정 (2026-09-21)** — 이 절은 처음에 "제3자 자산을 **하나** 번들한다 — D2Coding
> 폰트" 로 적혀 있었다. 결정 시점(2026-09-20)에도 틀린 서술이었다: markdown plugin 의 세 엔진은
> 2026-08-11 에 들어와 `crates/tasty-plugin-markdown/src/render.rs` 가 `include_str!`/
> `include_bytes!` 로 24 개 파일을 임베드하고 있었고, 그 출처·라이선스는
> `crates/tasty-plugin-markdown/assets/NOTICE.md` 에 이미 적혀 있었다. 최상위 인벤토리가
> plugin 디렉토리 안의 NOTICE 를 세지 않아서 모수가 하나로 보였다. 아래 Decision 의 세트
> 정의와 근거는 그 모수를 넷으로 바로잡아 다시 세운 것이다 — 결론(생성하지 않는다)은
> 유지되고 근거가 바뀌었다.

**그 조건은 닷새 뒤 충족됐다** — 릴리스 워크플로가 2026-05-01 에 들어왔다. 그런데 후속이 안
왔다. 이 결정을 쓰는 시점까지 넉 달 반 동안, 배포 입력 어디에도 고지가 없었다:
`scripts/build-linux.sh` · `scripts/build-macos-dmg.sh` · `scripts/build-windows.ps1` ·
`.github/workflows/release.yml` · 루트 `Cargo.toml` 의 deb/rpm asset 목록이 전부 licence 도
notice 도 0 회 언급한다. 유일한 예외가 `wix/main.wxs` 로, MSI 는 MIT 본문(`wix/License.rtf`)을
설치 동의 화면과 설치 디렉토리 양쪽에 넣는다 — 다만 그것은 Tasty 자신의 라이선스이고 폰트의
OFL 은 아니다.

즉 **문서가 자기 조건을 스스로 만족시킨 뒤에도 이행이 안 된 상태**였고, 그 사실을 아무 게이트도
보지 않았다. 고칠 때 먼저 정해야 하는 것은 "무엇을 넣을 것인가" 다 — 그 답이 플랫폼 넷의 입력을
동시에 정하기 때문이다.

## Decision

**고지 세트는 저장소에 이미 있는 고지 파일이고, 릴리스 시점에 생성하지 않는다.**

- `LICENSE` — Tasty 자체 코드의 MIT 본문
- `THIRD_PARTY_LICENSES.md` — 무엇이 번들되고 무슨 의무가 따르는지
- `LICENSES/` 의 **모든 파일** — 번들 자산의 라이선스 본문. upstream 태그의 라이선스 파일을
  고치지 않고 옮긴다(2026-09-21 기준 넷: D2Coding OFL · mermaid MIT · highlight.js BSD-3-Clause · KaTeX MIT)

> **사실 정정 (2026-09-21, D2Coding 본문)** — 위 항목은 처음에 "upstream 태그의 `LICENSE` 를
> 고치지 않고 옮긴다" 였고, D2Coding 에는 두 겹으로 맞지 않았다. 번들한 Ver 1.3.2 의 태그와
> 릴리스 zip 에는 라이선스 파일이 아예 없고, 저장소의 `LICENSES/D2Coding-OFL.txt` 머리(저작권
> 줄 · Reserved Font Name)는 upstream 어느 자료에도 없는 문구였다(`D2Coding-Bold` RFN 누락).
> 지금 그 파일은 upstream 이 라이선스 파일을 처음 둔 태그 `VER1.3.3` 의 `OFL.txt` 와 바이트
> 동일하다. 출처 URL · 커밋 · 해시는 `THIRD_PARTY_LICENSES.md` 의 D2Coding 절이 정본이다.
> 이 결정(세트를 생성하지 않는다)은 그대로다.

셋째 항목은 파일 이름이 아니라 **디렉토리**로 정한다. 본문이 하나 늘 때 스테이징 자리마다 파일
이름을 다시 적게 하면, 그 자리 중 하나가 빠지는 것이 기본값이 된다(이 결정의 첫 판이 정확히
그렇게 셋째 줄을 하나로 고정했다).

생성 단계를 두지 않는 근거는 **생성기가 세는 대상과 이 세트가 세는 대상이 다르다**는 것이다.
이 세트에 드는 것은 사람이 저장소에 **벤더링한 파일**(폰트 · 미니파이된 JS/CSS · woff2)이고,
생성기(대안 A 의 `cargo about` 류)는 `Cargo.lock` 의 크레이트를 센다 — 벤더링한 JS 는 그 도구의
입력에 없다. 그래서 생성기를 들여도 이 넷은 여전히 손으로 적어야 하고, 넷은 한 upstream 이
`LICENSE` 한 파일로 덮이는 형태라 사람이 관리할 수 있는 크기다. 생성기를 두면 빌더마다 그 도구를
깔아야 하고(릴리스 러너는 self-hosted 다) 출력이 도구 버전에 따라 흔들린다는 비용은 그대로다.

> **정정 전 문구 (2026-09-20 판)** — 세트는 "저장소에 이미 있는 세 파일"(`LICENSE` ·
> `LICENSES/D2Coding-OFL.txt` · `THIRD_PARTY_LICENSES.md`)이었고, 생성하지 않는 근거는 "세는
> 대상이 하나 — 번들 제3자 자산이 폰트 하나" 였다. 바뀐 이유는 Context 의 사실 정정이다: 모수가
> 넷이었으므로 셋째 줄이 부족했고, "하나라서 손으로 된다" 는 근거는 모수가 넷이 되자 설명력을
> 잃었다. 위 근거는 그 자리를 "생성기가 이 모수를 세지 못한다" 로 바꾼 것이다.

**자리는 산출물의 관례를 따르고, 그래서 플랫폼마다 다르다.** `tar.gz` 는 패키지 매니저가 없어
최상단, deb 은 Debian 관례대로 `/usr/share/doc/tasty/`, rpm 은 RPM 관례대로
`/usr/share/licenses/tasty/`, AppImage 는 후자와 같은 배치를 쓴다. `LICENSES/` 하위 경로는
deb·tar.gz·AppImage 에서 유지한다 — `THIRD_PARTY_LICENSES.md` 안의 상대 링크가 설치된 트리에서
그대로 풀리게 하기 위함이다. 정본 표는 그 문서가 갖고, 이 ADR 은 그 표를 복제하지 않는다.

**이행은 검증 가능한 플랫폼부터 한다.** 이 결정과 함께 착지한 것은 Linux 넷과 릴리스 에셋
업로드인데 **둘의 근거가 다르다.** Linux 넷은 빌더에서 산출물을 열어 고지 세트가 그 자리에
있는 것을 확인했다. 릴리스 에셋 업로드는 **배선까지다** — 그 스텝을 담은 잡이 태그 푸시에서만
돌기 때문에, 이 결정이 착지한 시점에는 실행을 관측할 방법이 없었다. 그쪽은 통과가 아니라
**미측정**이고, 처음 태그를 미는 릴리스가 그 값을 만든다. Windows `.zip` 과 macOS `.dmg` 는
그 빌드 환경이 아니면 산출물을 열 수 없어 **의도적으로 미이행으로 남겼다.**

**규율은 "배선하지 않는다" 가 아니라 "배선을 동작으로 적지 않는다" 다.** 위 셋이 서로 다른
자리에 놓이는 이유가 그것이다 — Linux 넷은 관측해서 동작으로 적었고, 릴리스 에셋은 배선했으되
미측정으로 적었으며, Windows·macOS 는 관측할 환경이 없어 배선조차 안 했다. 세 상태가 문서에서
구별되는 한, 배선이 앞서는 것 자체는 문제가 아니다.

## Consequences

- **얻은 것**: Linux 산출물 넷이 OFL 1.1 이 요구하는 형태를 만족한다 — 빌더에서 열어 확인한
  값이다. 릴리스 에셋 쪽은 그 형태를 **만든다고 선언한 배선**이고 아직 관측된 실행이 없다(위
  Decision). 스테이징이
  `build-linux.sh` 의 한 함수(`stage_notice`)라 새 Linux 산출물이 생겨도 한 줄로 붙는다.
  같은 스크립트의 sanity check 가 tar.gz 셋·deb·rpm 의 고지 유무를 함께 본다.
- **잃은 것 (중요)**: 이 세트는 **정적 링크되는 Rust 의존 크레이트의 저작권 고지를 담지
  않는다.** `Cargo.lock` 에 이름이 844 개 있고 그중 MIT·Apache-2.0·BSD 계열은 바이너리 배포 시
  저작권 문구 재현을 요구한다. 그 의무의 현재 충족률은 **0** 이다 — 이 결정 전에도 0 이었고 이
  결정이 그것을 바꾸지 않는다. `deny.toml` 의 licenses allowlist 는 **정책 게이트**이지 고지
  산출물이 아니다.
- **운영 비용 / 유지 부담** (2026-09-21 에 세트를 디렉토리로 읽게 되면서 이 항목의 자리 수가
  줄었다 — 현재 값은 [ADR-0370](0370-macos-and-windows-artifacts-carry-the-notice-set-before-it-is-observed.md)
  의 Consequences 가 갖는다. 아래는 결정 시점의 기록이다): 새 제3자 자산이 들어오면 움직일 자리가 **다섯**이다 —
  `LICENSES/` · `THIRD_PARTY_LICENSES.md` · `build-linux.sh` 의 `stage_notice` ·
  `[package.metadata.deb]` · `[package.metadata.generate-rpm]`. **그 다섯이 함께 움직이는지
  재는 채널은 없다.** 만들지 않은 이유는 이 회차가 새 게이트를 만들지 않기로 한 회차이기
  때문이고, 없다는 사실을 여기 적는 것으로 대신한다.

## Alternatives Considered

- **A: 의존 크레이트 고지를 생성한다** (`cargo about` · `cargo-bundle-licenses` 류) — 위
  "잃은 것" 을 실제로 닫는 유일한 길이다. 이번에 안 고른 이유는 셋이다. 첫째, 이 머신에도
  릴리스 러너에도 그 도구가 없어서 **이 회차에서 출력을 확인할 수 없다**(설치 자체가 새 공급망
  입력이다). 둘째, 844 개짜리 출력의 정확성은 도구 버전에 딸려 있어 "배선했다" 와 "맞다" 가
  갈린다. 셋째, 그것은 고지 세트의 **내용을 바꾸는 결정**이라 벤더링한 본문을 나르는 이 이행과 크기가
  다르다. **따로 결정할 일로 남긴다** — 위 "잃은 것" 이 그 입구다.
- **B: 전 플랫폼을 한 자리(`usr/share/licenses/tasty/`)로 통일한다** — 배치가 하나라 표가 짧아
  진다. 안 고른 이유는 deb 이 그 경로를 쓰지 않기 때문이다. Debian 패키지 문서는
  `/usr/share/doc/<pkg>/` 이고, 거기 cargo-deb 이 만드는 `copyright` 가 이미 산다. 관례를
  거스르면 배포판 도구가 보는 자리와 실제 자리가 갈린다.
- **C: 릴리스 에셋으로만 올리고 산출물에는 안 넣는다** — 워크플로 한 스텝으로 끝난다. 안 고른
  이유는 OFL 1.1 이 요구하는 것이 **함께 배포되는 것**이라서다. 따로 내려받을 수 있는 것은 그
  요구를 대신하지 못한다. 다만 에셋 업로드 자체는 유용해서 **함께** 한다.
- **D: 미이행 플랫폼까지 한 번에 배선한다** — Windows·macOS 도 같은 회차에 넣는 안. 안 고른
  이유는 이 빌더에서 그 산출물을 열 수 없어 확인이 원리적으로 불가능하기 때문이다. 확인 없이
  넣으면 다음 사람이 그 자리를 "돌아간다" 로 읽는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `LICENSES/` 의 파일 수와 `THIRD_PARTY_LICENSES.md` 첫 표의 행 수가 어긋난다. 좌변 둘은
  `ls LICENSES | wc -l`(2026-09-21 에 4)과 그 표의 데이터 행 수(같은 날 4)다. 어긋나면 세트와
  인벤토리 중 하나가 새 자산을 놓친 것이다. **오늘 이 둘을 견주는 가드는 없다.**
  (이 자리에 있던 이전 트리거 "`LICENSES/` 파일이 둘 이상이 된다" 는 결정 시점에 이미 충족돼
  있었다 — 위 Context 정정 참조. 2026-09-21 에 재검토했고, 결론은 유지·근거 교체다.)
- 벤더링하는 upstream 하나가 `LICENSE` 한 파일로 덮이지 않는다 — 예컨대 번들 안의 하위 구성요소가
  **다른** 라이선스를 요구하거나, upstream 이 `NOTICE` 파일(Apache-2.0 류)을 따로 둔다. 좌변은
  그 upstream 태그의 루트 파일 목록이다. 그때는 "upstream 하나 = 본문 하나" 라는 관리 단위가
  깨지고, 세트를 upstream 별 디렉토리로 나누거나 생성기를 다시 볼 이유가 생긴다.
- 고지 세트를 스테이징하는 자리가 위 "운영 비용" 의 다섯을 넘어선다. 새 산출물 종류가 생겼다는
  뜻이고, 그때는 세트를 파일 목록 하나로 뽑아 소비처가 이름으로 부르는 형태가 싸진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- Windows·macOS 빌더에서 미이행 두 자리를 확인할 수 있게 된다. 재는 법: 그 환경에서
  `.zip`/`.dmg` 를 풀어 고지 세트가 있는지 본다.
- 정적 링크 의존의 저작권 고지를 실제로 요구받는다(배포 채널·법무 검토). 재는 법: 그 요구의
  형태(목록인지 본문 전문인지)를 먼저 확정한 뒤 대안 A 를 다시 연다.

## References

- `THIRD_PARTY_LICENSES.md` — 고지 세트와 산출물별 자리의 정본 표. (결정이 실현된 현재 위치)
- `scripts/lib/notice-set.sh` 의 `stage_notice` — 스테이징 함수. `scripts/build-linux.sh` · `scripts/build-macos-dmg.sh` 가 source 한다. (결정이 실현된 현재 위치)
- 루트 `Cargo.toml` 의 `[package.metadata.deb]` · `[package.metadata.generate-rpm]` asset 목록. (결정이 실현된 현재 위치)
- `.github/workflows/release.yml` 의 publish 잡. (결정이 실현된 현재 위치)
- `wix/main.wxs` 의 `Component Id='License'` — MSI 가 이미 MIT 본문을 넣는 자리. (결정 시점의 기록)
- [`docs/dev-guide/dist-build.md`](../dev-guide/dist-build.md) — 빌더가 읽는 절차.
- 부분 개정: [0370](0370-macos-and-windows-artifacts-carry-the-notice-set-before-it-is-observed.md) (이행 순서 조항과 대안 D 개정 — macOS·Windows 산출물도 관측 전에 배선한다)
