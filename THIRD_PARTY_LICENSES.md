# Third-Party Licenses

Tasty 자체 코드는 [`LICENSE`](LICENSE) 파일에 명시된 MIT 라이선스를 따릅니다.
아래 외부 프로젝트의 자산도 함께 배포합니다. 각 자산에는
OFL 1.1, Ubuntu Font Licence 1.0, MIT(Hack은 Bitstream Vera 라이선스 포함) 또는
BSD-3-Clause 라이선스가 적용됩니다.

| 프로젝트 | 버전 | 라이선스 | 포함 위치 | 라이선스 본문 |
|---|---|---|---|---|
| D2Coding ligature | Ver 1.3.2 | OFL 1.1 | 본체 바이너리 (폰트 2 개) | [`LICENSES/D2Coding-OFL.txt`](LICENSES/D2Coding-OFL.txt) |
| mermaid | 11.16.1 | MIT | markdown plugin 바이너리 | [`LICENSES/mermaid-MIT.txt`](LICENSES/mermaid-MIT.txt) |
| highlight.js | 11.10.0 | BSD-3-Clause | markdown plugin 바이너리 | [`LICENSES/highlightjs-BSD-3-Clause.txt`](LICENSES/highlightjs-BSD-3-Clause.txt) |
| KaTeX | 0.18.4 | MIT | markdown plugin 바이너리 (JS · CSS · 폰트 20 개) | [`LICENSES/KaTeX-MIT.txt`](LICENSES/KaTeX-MIT.txt) |
| egui 기본 폰트: Hack Regular | epaint_default_fonts 0.31.1 | MIT, Bitstream Vera | 본체 바이너리, egui를 링크하는 plugin 바이너리 | [`LICENSES/Hack-MIT-Bitstream-Vera.txt`](LICENSES/Hack-MIT-Bitstream-Vera.txt) |
| egui 기본 폰트: Noto Emoji Regular | epaint_default_fonts 0.31.1 | OFL 1.1 | 본체 바이너리, egui를 링크하는 plugin 바이너리 | [`LICENSES/NotoEmoji-OFL-1.1.txt`](LICENSES/NotoEmoji-OFL-1.1.txt) |
| egui 기본 폰트: Ubuntu Light | epaint_default_fonts 0.31.1 | Ubuntu Font Licence 1.0 | 본체 바이너리, egui를 링크하는 plugin 바이너리 | [`LICENSES/Ubuntu-Ubuntu-font-1.0.txt`](LICENSES/Ubuntu-Ubuntu-font-1.0.txt) |
| egui 기본 폰트: emoji-icon-font | epaint_default_fonts 0.31.1 | MIT | 본체 바이너리, egui를 링크하는 plugin 바이너리 | [`LICENSES/emoji-icon-font-MIT.txt`](LICENSES/emoji-icon-font-MIT.txt) |

`LICENSES/`의 파일은 각 프로젝트의 원문을 수정 없이 복사했습니다. mermaid, highlight.js,
KaTeX는 번들 버전의 태그에 있는 `LICENSE`를 사용합니다. egui 기본 폰트 네 개의 본문은
crates.io `epaint_default_fonts` 0.31.1 패키지의 `fonts/` 폴더에 있는 파일입니다. D2Coding Ver 1.3.2의 태그와
릴리스 ZIP에는 라이선스 파일이 없어, 다음 태그 `VER1.3.3`의 `OFL.txt`를 사용합니다.
출처는 아래 D2Coding 절에 기록했습니다. 프로젝트별 저작권 고지가 달라 일반 템플릿으로
대체하지 않습니다.

## 폰트

### D2Coding (D2Coding ligature)

- 출처: NAVER Corporation — https://github.com/naver/d2-coding-font (옛 이름 `naver/d2codingfont` 는 이 주소로 넘어갑니다)
- 버전: Ver 1.3.2 (2018-05-24)
- 적용 범위: `crates/tasty-font/assets/D2Coding-ligature-Regular.ttf`, `crates/tasty-font/assets/D2Coding-ligature-Bold.ttf`
- 라이선스: SIL Open Font License, Version 1.1 (OFL 1.1)
- 라이선스 본문: [`LICENSES/D2Coding-OFL.txt`](LICENSES/D2Coding-OFL.txt) — upstream 태그 `VER1.3.3`
  (커밋 `d95bc36438113099566bb39064334608955c2612`)의 `OFL.txt` 와 바이트 동일
  (`https://raw.githubusercontent.com/naver/d2-coding-font/VER1.3.3/OFL.txt`, sha256
  `1807e8dec4d65f474cbf9be39f5e2254ecb81702babc320749e272ea66ffcc69`). 태그 `VER1.3.2` 에는
  라이선스 파일이 없고, 그 시점 README 가 라이선스로 링크한 upstream wiki 의 `Open-Font-License`
  문서도 같은 두 저작권 줄을 적고 있습니다.
- 저작권: Copyright (c) 2015, NAVER Corporation — 위 파일 머리의 두 줄을 그대로 따릅니다.
- Reserved Font Name: `D2Coding`, `D2Coding-Bold`

OFL 1.1은 폰트의 사용·변경·재배포(상용 포함)를 허용하지만 다음을 요구합니다.

- 라이선스 본문(`LICENSES/D2Coding-OFL.txt`)을 함께 배포할 것.
- Reserved Font Name(`D2Coding` · `D2Coding-Bold`)을 보존할 것. 폰트 파일을 수정·재포장하여 재배포하려는 경우 파생물에 다른 이름을 사용해야 합니다. 본 저장소는 NAVER 공식 ttf를 그대로 번들하므로 원래 이름을 사용할 수 있습니다.

### egui 기본 폰트

본체와 egui를 링크하는 plugin은 `epaint`의 기본 폰트(`default_fonts` 기능)를 사용합니다.
`epaint_default_fonts` 크레이트(https://github.com/emilk/egui, 0.31.1)가 아래 네 폰트를
`include_bytes!`로 넣으므로 이 폰트들이 바이너리에 포함됩니다. 본문은 이 크레이트의
`fonts/` 폴더 파일과 바이트 동일합니다.

- Hack Regular(`Hack-Regular.ttf`, https://github.com/source-foundry/Hack): Hack 작업물은 MIT,
  바탕이 된 Bitstream Vera Sans Mono는 Bitstream Vera 라이선스(Reserved Font Name "Bitstream", "Vera").
  본문: [`LICENSES/Hack-MIT-Bitstream-Vera.txt`](LICENSES/Hack-MIT-Bitstream-Vera.txt) (크레이트의 `fonts/Hack-Regular.txt`)
- Noto Emoji Regular(`NotoEmoji-Regular.ttf`): SIL Open Font License 1.1.
  본문: [`LICENSES/NotoEmoji-OFL-1.1.txt`](LICENSES/NotoEmoji-OFL-1.1.txt) (크레이트의 `fonts/OFL.txt`)
- Ubuntu Light(`Ubuntu-Light.ttf`): Ubuntu Font Licence 1.0.
  본문: [`LICENSES/Ubuntu-Ubuntu-font-1.0.txt`](LICENSES/Ubuntu-Ubuntu-font-1.0.txt) (크레이트의 `fonts/UFL.txt`)
- emoji-icon-font(`emoji-icon-font.ttf`, https://github.com/jslegers/emoji-icon-font): MIT — Copyright (c) 2014 John Slegers.
  본문: [`LICENSES/emoji-icon-font-MIT.txt`](LICENSES/emoji-icon-font-MIT.txt) (크레이트의 `fonts/emoji-icon-font-mit-license.txt`)

OFL 1.1과 Ubuntu Font Licence 1.0은 폰트를 재배포할 때 저작권 고지와 라이선스 본문을 함께
넣도록 요구합니다. Tasty는 폰트 파일을 수정하지 않고 크레이트가 넣은 그대로 배포합니다.

## markdown plugin 의 렌더링 엔진

마크다운 플러그인(`tasty-official-plugins/crates/tasty-plugin-markdown`)은 아래 세 엔진의 24개 파일을 컴파일할 때
바이너리에 포함합니다(`include_str!` 4개, `include_bytes!` 20개). 이 자산은 실행 중
다운로드하지 않습니다. 출처 URL, sha512, 갱신 방법은
[자산 고지](https://github.com/zilhak/tasty-official-plugins/blob/develop/crates/tasty-plugin-markdown/assets/NOTICE.md)에 있으며, 여기에는 배포에 필요한
라이선스 정보를 정리했습니다.

### mermaid

- 출처: https://github.com/mermaid-js/mermaid (npm `mermaid`)
- 버전: 11.16.1
- 적용 범위: `tasty-official-plugins/crates/tasty-plugin-markdown/assets/mermaid.min.js`
- 라이선스: MIT — Copyright (c) 2014 - 2022 Knut Sveidqvist
- 라이선스 본문: [`LICENSES/mermaid-MIT.txt`](LICENSES/mermaid-MIT.txt) (태그 `mermaid@11.16.1` 의 `LICENSE`)
- 번들에 포함된 다른 MIT 구성요소(jQuery 이벤트 모듈 등)의 고지 주석도
  `mermaid.min.js` 안에 그대로 보존돼 있습니다.

### highlight.js

- 출처: https://github.com/highlightjs/highlight.js (cdnjs 의 prebuilt "common" 번들)
- 버전: 11.10.0
- 적용 범위: `tasty-official-plugins/crates/tasty-plugin-markdown/assets/highlight.min.js`
- 라이선스: BSD-3-Clause — Copyright (c) 2006, Ivan Sagalaev
- 라이선스 본문: [`LICENSES/highlightjs-BSD-3-Clause.txt`](LICENSES/highlightjs-BSD-3-Clause.txt) (태그 `11.10.0` 의 `LICENSE`)
- 번들 파일 머리의 배너(`(c) 2006-2024 Josh Goebel <hello@joshgoebel.com> and other contributors`)도
  고치지 않고 보존돼 있습니다. BSD-3-Clause 는 바이너리 재배포 시 위 저작권 고지·조건·면책 문구를
  배포물에 함께 넣을 것을 요구하고, 저작권자의 이름을 파생물 홍보에 쓰지 못하게 합니다.

### KaTeX

- 출처: https://github.com/KaTeX/KaTeX (npm `katex`)
- 버전: 0.18.4
- 적용 범위: `tasty-official-plugins/crates/tasty-plugin-markdown/assets/katex.min.js`, `tasty-official-plugins/crates/tasty-plugin-markdown/assets/katex.min.css`, `tasty-official-plugins/crates/tasty-plugin-markdown/assets/fonts/KaTeX_*.woff2` (20 개)
- 라이선스: MIT — Copyright (c) 2013-2020 Khan Academy and other contributors
- 라이선스 본문: [`LICENSES/KaTeX-MIT.txt`](LICENSES/KaTeX-MIT.txt) (태그 `v0.18.4` 의 `LICENSE`)
- 폰트에도 같은 MIT 라이선스가 적용됩니다. 해당 `LICENSE`에는 폰트에만 적용하는 별도
  조항이 없습니다.

MIT는 위 저작권 고지와 허가 문구를 "소프트웨어의 모든 사본 또는 상당 부분"에 포함하도록
요구합니다. 따라서 플러그인 바이너리를 배포할 때 라이선스 본문도 함께 제공합니다.

## 고지 세트

다음 파일을 배포물에 함께 넣습니다. 저장소의 원본을 사용하며 릴리스 때 새로 생성하지
않습니다. [릴리스 절차](docs/dev-guide/release.md).

- `LICENSE` — Tasty 자체 코드의 MIT 본문
- `THIRD_PARTY_LICENSES.md` — 본 문서 (무엇이 번들되고 무슨 의무가 따르는지)
- `LICENSES/` 의 **모든 파일** — 번들 자산의 라이선스 본문. 지금은 위 표의 여덟 파일입니다.

`LICENSES/`는 폴더 안의 모든 파일을 포함합니다. 새 자산의 라이선스 파일을 이 폴더에
추가하고 위 표도 갱신합니다. 이 폴더에는 라이선스 본문 파일만 둡니다. RPM은 본문을 한 폴더에
펼쳐 놓으므로 하위 폴더나 링크가 있으면 모든 패키징 스크립트가 실패합니다. 점으로 시작하는
항목(`.DS_Store` 등)도 거부합니다. 라이선스 본문이 아니고, 숨김 항목을 열거에서 빼면 그 자리의
폴더·링크가 위 거부를 피해 배포물에서 조용히 빠지기 때문입니다.

## 산출물별 위치

| 산출물 | 고지 파일 위치 |
|---|---|
| GitHub 릴리스 | 고지 파일을 릴리스 에셋으로 따로 올리지 않습니다. 아래 모든 배포물 안에 들어 있고 저장소에서도 볼 수 있습니다. |
| Windows `.msi` | 설치 디렉토리(`<설치 폴더>\tasty\`) 최상단, `LICENSES\` 하위 경로 유지. MIT 본문은 설치 동의 화면용 `License.rtf` 로도 한 번 더 들어갑니다. |
| Linux `tar.gz` | 압축을 풀면 나오는 디렉토리 최상단 |
| Linux 헤드리스 `tar.gz` | 압축을 풀면 나오는 디렉토리 최상단(`LICENSES/` 하위 경로 유지) |
| Linux `.deb` | `/usr/share/doc/tasty/` (Debian 관례. `LICENSES/` 하위 경로를 그대로 유지합니다) |
| Linux `.rpm` | `/usr/share/licenses/tasty/` (RPM 관례라 deb 과 배치가 다릅니다 — `LICENSES/` 의 본문도 하위 디렉토리 없이 이 자리에 바로 놓입니다) |
| Linux `.AppImage` | `usr/share/licenses/tasty/` |
| macOS `.dmg` | `Tasty.app/Contents/Resources/` (`LICENSES/` 하위 경로를 그대로 유지합니다). |
| macOS 헤드리스 `tar.gz` | 압축을 풀면 나오는 디렉토리 최상단(`LICENSES/` 하위 경로 유지) |
| Windows `.zip` | 압축을 풀면 나오는 최상단(`tasty.exe` 옆). |
| Windows 헤드리스 `.zip` | 압축을 풀면 나오는 최상단(`tasty.exe` 옆, `LICENSES\` 하위 경로 유지) |

빌드 스크립트는 만든 산출물을 다시 열어 고지 파일을 확인합니다. Linux `tar.gz`(헤드리스 포함)·
`.deb`는 목록에서 위 자리의 경로를 확인합니다. `.rpm`의 목록 확인은 `rpm` 명령이 있는 빌더에서만
실행되고, 없으면 건너뜁니다. macOS `.dmg`(읽기 전용 마운트)·macOS 헤드리스
`tar.gz`·Windows `.zip`(헤드리스 포함)·`.msi`(관리 설치, 다른 아키텍처는 WiX `dark` 추출)는 꺼낸 파일을, `.AppImage`는 패키징 직전
AppDir를 저장소 사본과 바이트 비교합니다.

OFL 1.1, Ubuntu Font Licence 1.0, MIT, BSD-3-Clause의 고지는 산출물 안에도 포함해야 합니다. 릴리스에서 별도로
다운로드할 수 있게 하는 것만으로는 이를 대신하지 못합니다.
