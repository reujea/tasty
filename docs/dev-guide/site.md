# 공개 사이트 (GitHub Pages)

랜딩 페이지와 **사용자 가이드**를 정적 HTML 로 발행하는 경로. 산출물은 GitHub Pages 로 배포된다.

- 앱: `site/` (Astro + React, Node 22)
- 콘텐츠: `site/content/` (한국어 정본) · `site/content/en/` (영어 번역)
- 산출물: `site/` 아래 `dist/` — 빌드가 만들며 Git에서 제외한다(경로 인용으로 안 적는다. 갓 클론한
  트리에는 없어서 좌표를 실재로 판정하는 CI의 경로 검사가 실패할 수 있다)
- 배포: `.github/workflows/pages.yml`. main의 `site/**`, `assets/icons/tasty-melon.svg`, 해당 워크플로 변경 시 빌드와 링크 검사를 실행한다. **배포는 릴리스를 공개할 때(`release.yml`이 실행)와 수동 실행 때만** 한다. 사이트 전체가 사용자가 받을 수 있는 버전을 설명해야 하기 때문이다. 표시 버전은 `Cargo.toml`이 아니라 공개된 최신 릴리스(`release.json`)이고, 이 파일이 없는 로컬 빌드는 버전을 표시하지 않는다. 빌드 명령은 `npm run build`다.
  다음에 `npm run check-links` 로 산출물의 내부 링크·앵커를 전수 판정하고 나서 올린다.
  이 단계는 `site/content/` 앵커의 실제 HTML 결과를 검사한다([링크와 표](../documentation-model.md#링크와-표))

**`docs/` 는 발행하지 않는다.** `docs/` 는 코드를 고치는 사람과 에이전트를 위한 명세·설계·ADR 이고,
사이트는 Tasty 를 받아서 쓰는 사람을 위한 것이라 독자가 다르다. 사이트가 실을 내용은 전부
`site/content/` 에 사용자 시점으로 따로 쓴다. 명세가 바뀌면 기획서를 고치고, 가이드는 릴리스할 때 기획서에 따라 손본다
([`CLAUDE.md`](../../CLAUDE.md) "문서 갱신").

## 사이트 문구와 어조

사이트는 터미널 자체를 먼저 소개하고, 구체적인 사용 상황과 실제 동작으로 기능을 설명한다.
랜딩 제목과 설명은 사용자의 행동·감정을 연출하는 광고 문구보다 기능과 사용 조건을 담는다.
“에이전트에게 맡기고, 내 작업도 이어가세요”처럼 막연한 효익을 권하는 대구는 피한다.
설치·조작에 필요한 안내는 “하세요”로 쓰되, 기능 소개마다 행동 권유를 반복하지 않는다.
한국어는 설명에 "합니다", 안내에 "하세요"를 사용한다. "이 페이지를 읽으면" 같은 반복적인
도입부, 과장된 보장, 내부 구현을 빗댄 표현은 피한다. 사용자 상태 보호는 포커스·선택·스크롤처럼
보장하는 범위를 명시하고, 연동 설정이 필요한 알림은 그 조건을 함께 설명한다.
크로스 플랫폼 소개는 워크스페이스·탭·화면 분할처럼 같은 방식으로 사용하는 기능을 설명하고,
설치 방법과 지원 아키텍처는 설치 가이드로 연결한다.

- 랜딩은 `site/src/lib/landing.ts`, 검색·공유 소개는 `site/src/lib/meta.json`,
  공통 안내와 푸터는 `site/src/lib/strings.ts`에서 한국어와 영어를 함께 관리한다.
- 사용자 가이드는 `site/content/`와 `site/content/en/`를 같은 의미로 수정한다.
  명령어, 설정 키, 실제 메뉴 이름과 주의사항은 의미를 유지한다. 번역을 수정한 뒤 스탬프를 갱신한다.
- 제목을 바꿀 때 기존 앵커를 별칭으로 남겨 저장된 링크도 계속 열리게 한다.
- 디자인 소개는 `site/src/components/design/`, 갤러리 도입부는 `site/src/lib/design.ts`의
  `GALLERY_INTROS`에서 관리한다. 갤러리 본문 소개와 메타 설명은 같은 값을 사용한다.
  디자인 킷의 예시와 명세는 vendor 원본을 사용한다.

## FE 골격

앱 UI 를 그림이나 아스키 아트가 아니라 **디자인 시스템의 실제 컴포넌트**로 보여주는 것이
목적이다 — 랜딩의 제품 창도, 가이드의 UI 설명도, 디자인 섹션도 전부
[`site/vendor/`](../../site/vendor/README.md) 의 킷에서 온다.

```
site/
  package.json          astro · @astrojs/react · react 18
  astro.config.mjs      정적 출력, React 아일랜드
  vendor/               Design System Artifact의 사본. 사이트가 그대로 import한다
  scripts/              check-links · check-contrast · vendor-manifest · stamp
  src/pages/            라우트
  src/components/ src/layouts/ src/lib/ src/styles/
```

사본의 컴포넌트와 표본은 ES 모듈이다. 사이트는 변환 단계 없이 `site/vendor/`를 Astro 프로젝트 안에서 직접 import한다. 정본은 `site/vendor/` 하나이고 생성 트리는 없다.

토큰과 정적 파일은 빌드할 때 사본에서 파생한다.

| 산출물 | 만드는 곳 |
|------|------|
| 페이지 토큰 | `site/src/styles/tokens.css`. tier 순서로 사본을 `@import`한다 |
| `/design/tokens.css` · `/design/kit.css` · `/design/cards/*.html` | `site/src/lib/vendor-assets.ts`와 이를 읽는 엔드포인트 |
| `/tasty-melon.svg` · `/tasty-icon.png` | `assets/icons/`의 원본을 읽는 엔드포인트 |

### 갤러리 페이지

갤러리는 앱이 따르는 정본이다. 페이지 규칙은 사본의 `site/vendor/gallery/README.md`에 있고 사이트 구현은 다음과 같다.

- `site/src/layouts/GalleryPage.astro`: 공용 머리글·테마·푸터(`Base.astro`), 가이드 형식의 사이드바, 이전·다음을 그린다. 페이지 순서와 묶음은 `site/src/lib/design.ts`의 `GALLERY_GROUPS`가 정한다.
- `site/src/pages/design/gallery/*/index.astro`: 라우트마다 `vendor/gallery/<페이지>/index.jsx`의 `page`와 기본 내보내기를 싣는다.
- 표본의 범위는 `[data-gallery-page]`다. 사이트 전역 규칙 중 표본과 부딪히는 것(`img`, `:focus-visible`)은 이 범위를 뺀다.
- `parts/specimen.jsx`의 `Spec`이 제목, 데모, 상태 격자, 숨은 상태의 프레임, 명세 표 순서로 그린다. `?audit=1`을 붙이면 지연 마운트 없이 모든 데모를 그린다.

### 사이트 페이지의 바탕과 잉크

사이트 스타일은 1단계 색을 직접 읽지 않고 앱과 같은 역할 토큰을 읽는다. 위치마다 쓸 토큰은 사본 `tokens/ground-ink-rules.json`의 `site.places`가 정한다. 1단계 색을 읽으면 Mocha에서 통과한 값이 Latte에서 미달할 수 있기 때문이다.

- 브랜드: 표식은 `brand-melon-*` 색을 유지한다. 브랜드 색 글자와 글리프는 `brand-ink`를 읽는다. 워드마크 점은 글자가 아니라 표식이므로 `data-audit-exempt="brand"`를 붙인다.
- 주요 버튼: 바탕은 두 테마 모두 표식의 과육(`site-cta-bg`), 글자는 씨앗(`site-cta-fg`)이다. 포인터 올림과 누름도 과육의 색조다. 가이드 그림 라벨(`.anat::after`)도 같은 짝이다.
- 링크: `accent-primary-raised`다. 본문 안 링크는 평소에도 밑줄을 긋는다. Latte에서 이 잉크는 밝기만으로 본문과 구분하기 어렵다. 메모 안 링크는 본문 잉크에 밑줄이다. 틴트 위에 두 번째 색상을 쓰지 않기 위해서다.
- 머리글: 불투명한 `bg-sidebar`다. 뒤로 지나가는 내용이 잉크에 영향을 주지 않는다.
- 포커스 링: 앱과 같은 `border-focus`다.
- 건너뛰기 링크: 키보드 포커스를 받을 때만 머리글 위 왼쪽 위에 나타난다. 자기 `bg-panel` 칠과 1px 테두리에 링크 잉크를 쓰고 포커스 링이 그 둘레를 두른다.
- 인라인 코드: 칩 바탕은 `site-inline-code-bg`, 테두리는 1px `border-default`다. Latte에서 `raised`는 본문에서 지나치게 튀어 `mantle`로 내린다.
- 번역 안내: `.note`는 경고 틴트(번역이 뒤처졌을 때), `.note--untranslated`는 주 색 틴트(번역이 없을 때)다. 앞머리 낱말과 색 테두리는 없다.
- 표: 가이드의 Markdown 표는 렌더링 단계(`site/src/lib/satteri-table-wrap.mjs`)에서 `.table-wrap`으로 감싼다. 1px 테두리와 `mantle` 머리 행을 그리고 좁은 화면에서는 안쪽에서 가로 스크롤한다.
- 구분자: 이동 경로의 `/`는 장식이다. `aria-hidden`과 `data-ink-grade="none" data-ink-why="ornament"`를 붙이고 `glyph-dim`을 쓴다.
- 코드 블록: 두 테마에서 같은 어두운 팔레트(`site-code-*`)다. 하이라이터가 색을 인라인으로 쓰므로 `global.css`의 `pre.astro-code` 규칙이 `!important`로 바탕, 기본 잉크, 주석 색을 덮는다. 주석은 `style` 속성의 `#6A737D`로 찾으므로 하이라이터 테마를 바꾸면 이 선택자도 함께 고친다. 다른 하이라이터 잉크는 예외 없이 일반 기준으로 잰다.
- 앱 그림: 페이지가 직접 그리는 터미널과 창 그림은 프로그램 출력이 아니므로 `ansi-content` 예외를 쓰지 않는다. 선은 `term`과 `base` 바탕의 글자 셀을 읽는다. `AppShell.jsx`와 `WindowAnatomy.jsx`는 킷 `work.jsx`의 `C`와 같은 잉크를 쓴다.

가이드의 그림은 Markdown 렌더링 단계에서 킷을 정적 HTML로 그려 콘텐츠 캐시(`node_modules/.astro/data-store.json`)에 들어간다. Astro는 이 캐시를 본문과 설정의 해시로만 판단해 킷이 바뀌어도 다시 그리지 않으므로, `npm run build`는 `astro build --force`로 캐시를 비운다. 개발 서버에서 킷을 고친 뒤 가이드 그림이 그대로면 `astro dev --force`로 시작한다.

### 렌더링 대비 검사

`npm run check-contrast`는 빌드 결과를 띄워 갤러리 페이지와 사이트 페이지를 Mocha와 Latte로 렌더링한다. `--site-only`와 `--gallery-only`로 한쪽만 실행하고, `--page <키>`는 갤러리 한 페이지만 잰다. 코드는 `site/scripts/check-contrast.mjs`(갤러리), `contrast-site.mjs`(사이트), `contrast-page.mjs`(페이지 안에서 실행하는 측정)에 있다.

갤러리 페이지에서는 다음을 잰다.

- 고정 셀: `tokens/ground-ink-rules.json`의 행마다 부품의 잉크가 지정 토큰인지(채널당 1/255 허용), 합성 바탕에서 등급을 넘는지 본다. 행에 셀이 없거나 부품을 찾지 못해도 실패한다. 부품 위치는 `parts/part-map.js`에 있다.
- 나머지 글자·글리프·입력: 글자는 4.5, 글리프는 3을 기준으로 잰다. `data-audit-exempt`와 `data-ink-grade`의 규칙은 사본 `RULES.md` 14절을 따르고, 쓰인 예외 id와 횟수를 출력한다. 완전히 투명한 요소만 화면에 없는 것으로 보며 그 밖의 opacity는 합성해 잰다.
- SVG: 보이는 도형의 stroke와 fill을 각각 재고 도형 수를 출력한다. 도형 자신의 `stroke-opacity`·`fill-opacity`와 SVG 안 `<g>`를 포함한 모든 조상의 opacity를 곱한다. SVG 안 `<text>`는 fill을 글자 기준으로 잰다. 아무것도 칠하지 않는 SVG의 수도 출력한다. 바탕은 DOM 조상에서 읽으므로 같은 SVG 안에서 도형 아래 깔린 다른 도형은 바탕으로 보지 않는다. `<use>`의 대상 내부와 그라데이션·패턴 칠은 재지 않으며 그라데이션·패턴 칠의 수를 출력한다.
- 비활성 조상(`:disabled`, `aria-disabled="true"`, `data-ink-why="disabled"`) 안: 등급 없는 비활성 토큰만 등급을 면제한다. 그 밖의 잉크는 일반 기준으로 잰다.
- 측정 0건, 알 수 없는 예외 id, 페이지 오류도 실패다.
- 표본 밖의 머리글·사이드바·푸터도 같은 기준으로 잰다.

사이트 페이지는 갤러리 밖의 빌드된 페이지 전부(랜딩, 가이드, 디자인, privacy, 양쪽 언어, `design/cards/`의 지침 문서)다. 기준은 사본 `tokens/ground-ink-rules.json`의 `site.places`이고 사본 `RULES.md` 15절이 같은 내용을 설명한다.

- 패스: 각 페이지를 넓은 화면(1440px)과 좁은 화면(390px)에서 잰다. 넓은 화면에서는 랜딩 창의 나머지 탭, 포인터 올림과 누름, 키보드 포커스, 검색 결과도 잰다.
- 행: 선택자가 어느 페이지에서도 요소를 찾지 못하면 실패한다. 요소를 하나도 재지 못해도, 등급에 못 미쳐도, 어떤 행도 지정하지 않은 잉크를 보여도 실패한다.
- 여러 잉크를 묶는 행: 한 선택자를 여러 행이 나눠 쓰거나 `a`처럼 범위가 넓거나 `.shell`처럼 창 그림 전체를 가리키는 행이다. 해당 잉크가 한 번 이상 나타나면 되고, 요소마다 등급은 그대로 잰다.
- 행이 없는 것: 보이는 글자, 생성 내용(`::before`, `::after`), 입력값, placeholder는 4.5로 잰다. 24px 이상 또는 18.66px 이상 굵은 글자는 3이다. SVG 도형은 3이다.
- 바탕: 불투명한 칠까지 조상을 합성한다. 그라데이션은 색 지점마다 재서 가장 낮은 값을 쓰고, 그림 바탕 위 글자는 값을 잴 수 없어 실패한다. 재는 값은 바탕의 실제 값이며 행의 `grounds`와 대조하지는 않는다.
- 예외: 규칙 파일의 `exceptions` id를 쓴 `data-audit-exempt`와 `data-ink-grade="none"`만 인정한다. 쓰인 횟수를 출력한다.
- 고정 마크업: 릴리스 버전, 번역 상태, 가이드 본문에 따라 달라 빌드 결과에 없는 행(메모, 작은 제목, 링크 속 코드, 인용 속 링크, 버전 칩)은 `contrast-site.mjs`의 `FIXTURES`로 넣어 재고, 출력에 행 이름을 적는다. 규칙이 깨지면 다른 잉크가 나와 실패한다. 실제 페이지에 그 요소가 있다는 뜻은 아니다.
- `design/cards/`의 지침 문서는 자체 CSS를 쓰고 클래스 이름이 행과 겹칠 수 있어 행을 적용하지 않고 글자와 도형만 잰다.
- 재지 않는 것: 데모의 오버레이(설정 창, 명령 팔레트 등)는 갤러리에서 잰다. 포인터 패스는 행마다 첫 요소 하나만 잰다.

브라우저는 `CHROMIUM_PATH`, Playwright 캐시의 Chromium, `/snap/bin/chromium` 순으로 찾는다. `playwright-core`만 의존성으로 두고 브라우저는 내려받지 않는다. 그래서 이 검사는 `pages.yml`이 아니라 [릴리스 절차](release.md#3-사이트-갱신)에서 실행한다.

### vendor 자신은 어디서 오는가

`site/vendor/` 는 원격 Design System 프로젝트의 사본이라,
릴리스할 때 그것을 받아와야 한다. 절차는
[`site/vendor/README.md`](../../site/vendor/README.md) 의 "vendor 갱신 절차" 에 있고,
정합 루프에서 이 단계가 차지하는 자리는
[design-change-workflow](design-change-workflow.md#정합-대상--사이트-사본-필수) 에 있다. 결정 근거는
[테마 시스템 (운영 상세)](../design/systems/theme.md).

오래된 vendor 사본도 정상 빌드할 수 있다. 저장소의 두 사본을 대조하는 다음 검사는 릴리스 사이에 사본이 뒤처지는 것이 정상이라 `#[ignore]`이고 릴리스 절차에서 실행한다.
`site_vendor_tokens_track_the_app_export.rs`(토큰 이름)와
`site_vendor_icons_match_the_app_transcription.rs`(아이콘 기하·채움·viewBox·선 굵기·cap/join)다.
문구·구성·컨트롤 삭제는 검사 범위 밖이므로 원격 디자인과 직접 비교한다.

`site/vendor/MANIFEST`는 사본 파일마다 마지막 갱신 때 받은 원격 원본의 sha256을 적는다.
일부러 다르게 둔 파일은 `variant` 표시와 로컬 sha256, 이유 한 줄을 함께 적는다.
갱신 절차에서 `site/scripts/vendor-manifest.mjs --remote <받은 원본 폴더>`가 이 파일을 쓰며,
이유 없는 차이가 있으면 쓰지 않고 실패한다. `site_vendor_manifest_matches.rs`는 사본의
파일 집합과 해시가 `MANIFEST`와 같은지 원격 조회 없이 확인한다. 옛 판 파일이나 기록 없는
직접 수정은 검출하지만, 원격이 마지막 갱신 뒤에 바뀐 것은 알 수 없다.

디자인 페이지는 `VendorStamp`로 vendor의 마지막 변경 날짜를 표시한다.
빌드할 때 `site/src/lib/vendor-stamp.ts`가 `git log -1 -- site/vendor`에서 README와 `MANIFEST`를 제외한 최신 변경을 읽는다.
두 파일은 사본을 설명하는 파일이며 디자인 사본 자체가 아니기 때문이다. 문구와 스타일은
각각 `site/src/components/design/VendorStamp.astro`와
`site/src/styles/vendor-stamp.css`에서 함께 관리한다.

이 날짜는 원격 파일을 처음 받은 날짜나 모든 파일의 동기화 날짜가 아니다. 사본을
부분 수정했을 수도 있어 "마지막 변경"이라고 표시한다. Git 이력을 읽을 수 없으면
추측한 날짜를 표시하지 않는다.

## 빌드

```bash
cd site
npm install          # 최초 1 회

npm run build        # astro build --force -> site/dist/
npm run dev          # 개발 서버
npm run preview      # 빌드 결과 확인
npm run check-links     # 빌드 결과의 내부 링크·앵커 검사
npm run check-contrast  # 빌드 결과의 갤러리·사이트 페이지 렌더링 대비 검사

# 번역 파일에 원본 해시 스탬프 (아래 "번역 모델")
npm run stamp content/en/index.md
```

## URL 구조

| 경로 | 내용 |
|------|------|
| `/` | 영어 랜딩 |
| `/ko/` | 한국어 랜딩 |
| `/guide/**` | 영어 가이드. `site/content/en/**.md` 가 있으면 번역, 없으면 한국어 본문 + 배너 |
| `/ko/guide/**` | 한국어 가이드 (`site/content/**.md`, canonical) |
| `/privacy/` · `/ko/privacy/` | 개인정보 안내. 본문은 `site/src/components/Privacy.astro` 에 두 언어로 있고 푸터에서 연결한다 |
| `/design/` · `/ko/design/` | 디자인 가이드라인 (브랜드 · 색 · 타이포 · 간격). 크롬만 이중언어고 싣는 문서는 같다 |
| `/design/tokens/` · `/ko/design/tokens/` | 3티어 토큰 문서 |
| `/design/gallery/**` | 레퍼런스 갤러리 15 페이지. 사이트 머리글과 푸터를 쓰며 언어별로 나누지 않는다 |
| `/design/cards/*.html` | 가이드라인 원본 문서. 페이지가 iframe 으로 싣는다 |
| `/search-index.json` (en) · `/search-index.ko.json` | 검색 인덱스. 페이지마다 제목·섹션과 본문 앞 600자를 싣는다(`site/src/lib/search.ts`). 코드 블록과 HTML 태그는 빼고, 인라인 코드는 글자로 남기며, 단어 안의 하이픈은 지워지지 않아 `task-await` 같은 명령 이름이 입력한 그대로 맞는다. 명령 이름으로 찾게 하려면 그 이름을 제목이나 첫 문단에 쓴다 |

### 배포 경로 (base)

사이트는 GitHub Pages의 커스텀 도메인 `https://tasty.zilhak.org/`의 **루트**에서 서빙한다. 도메인은
저장소의 Pages 설정에 지정하며, Actions로 배포하므로 `CNAME` 파일을 두지 않는다. 경로 접두사는
[`site/src/lib/base.mjs`](../../site/src/lib/base.mjs) **한 곳에만** 적고(현재 빈 문자열), 나머지는
전부 거기서 파생한다.

| 소비자 | 어떻게 받는가 |
|--------|---------------|
| 페이지·컴포넌트의 모든 내부 링크 | `astro.config.mjs` 의 `base` → `import.meta.env.BASE_URL` → `site/src/lib/site.ts` 의 `url()` |
| 개발 서버 | 같은 `base` — `astro dev` 도 그 경로 아래에 마운트한다. **로컬과 배포가 같은 값으로 돈다** |
| 마크다운 파이프라인의 다이어그램 | 직접 주입받는다. Astro 설정에서 로드돼 `BASE_URL` 이 닿지 않는 컨텍스트라 그 경계에서 한 번 적용한다 |
| `npm run check-links` | 직접 import 한다. 여기서 base 를 모르면 배포가 못 여는 링크를 통과시킨다 |
| 킷·갤러리 모듈 | `import.meta.env.BASE_URL` 을 읽는 `BASE` 상수를 파일 머리에 둔다 |
| `/design/cards/*.html` | 상대경로. 문서 안에서 `import.meta.env` 를 못 읽으므로 문서 위치 기준으로 쓴다 |

**어떤 경로도 두 번 적지 않는다.** 도메인 루트가 아닌 경로로 옮기면 `base.mjs` 의 값만 바꾼다.

기본 URL과 랜딩 `/`는 영어로 제공한다.
정본이 한국어인 것과는 별개다 — 작성은 한국어로, 노출 기본은 영어로.

랜딩 `/`는 첫 화면을 그리기 전에 언어를 판단한다(`site/src/pages/index.astro`).
헤더의 EN/KO로 고른 값(`localStorage`의 `tasty-lang`)이 있으면 그 값을, 없으면 `navigator.language`를 따르며
`ko`로 시작하면 `/ko/`로 이동한다. 다른 페이지는 이동하지 않아 공유한 링크가 그 언어로 열린다. JS가 꺼져 있으면
영어 랜딩이 그대로 보인다. 페이지마다 `Base.astro`가 두 언어 주소를 `hreflang`으로 알리며, 절대 주소는
`site/src/lib/base.mjs`의 `SITE`로 만든다.

## 방문자 분석

방문 통계는 Cloudflare Web Analytics로 수집한다. `tasty.zilhak.org`는 프록시를 거치지 않는 DNS 전용
레코드라 자동 삽입이 동작하지 않으므로, 비콘 스크립트를 페이지에 직접 넣는다.

- 스크립트와 사이트 토큰은 `site/src/components/Analytics.astro` 한 곳에 둔다. 토큰은 발행된 모든
  페이지에 노출되는 공개 값이다.
- `<head>`를 가진 `Base.astro`가 이 컴포넌트를 싣는다. `Design.astro`, `Guide.astro`, `GalleryPage.astro`는 `Base.astro`를 통해 함께 포함된다. iframe으로 싣는 `/design/cards/*.html`은 집계하지 않는다.
- production 빌드에만 포함되어 `astro dev` 방문은 집계되지 않는다.
- 수집 항목이나 처리 주체가 바뀌면 개인정보 안내(`site/src/components/Privacy.astro`)의 두 언어 본문과 최종 수정일을 함께 고친다. 앱에 외부 전송 기능을 추가할 때도 안내의 앱 항목을 확인한다.
- 통계는 Cloudflare 대시보드의 Web Analytics에서 본다. 저장소 Insights의 Traffic은 Pages 방문을 집계하지 않는다.

## 콘텐츠 구조

`site/content/` 의 디렉토리가 사이드바 섹션이고, 페이지 순서는 `site/src/lib/guide.ts` 의 `ORDER` 가
정한다. 파일 이름 순이 아니다 — 읽을 순서를 배열로 명시한다.

```
site/content/
  index.md                      가이드 홈
  getting-started/  install · first-look
  using/            workspaces · workflow-layouts · panes-tabs-splits · terminal · files
  customize/        keybindings · settings · themes · scripts
  agents/           cli · background-tasks · claude-codex · tasks · hooks-notifications
  remote/           attach
  plugins/          index
  help/             troubleshooting
  en/               위 트리의 1:1 미러 (번역)
```

- 새 페이지를 추가하면 `ORDER` 에도 넣는다. 빠뜨리면 발행되지 않는다.
- CLI 입문(`agents/cli.md`)·명령 참조(`agents/cli-reference.md`)·진단(`help/cli-diagnostics.md`) 페이지의 `sh` 코드 블록에 있는 `tasty …` 예시는 `crates/tasty-cli/tests/guide_examples_parse.rs` 가 실제 명령 트리로 파싱한다(ko·en). 명령 경로·필수 인자·옵션 이름만 확인하며 plugin 명령, 자리 표시(`<ID>`·`…`), 셸 치환·파이프가 있는 줄은 판정하지 않는다. 문장의 정확성과 실행 결과는 이 검사가 보장하지 않는다.
- 새 디렉토리(섹션)를 만들면 `SECTIONS` 에 (디렉토리, 한국어 라벨, 영어 라벨) 을 추가한다.
  등록되지 않은 디렉토리에 페이지를 두면 빌드가 실패한다.
- 페이지 첫 줄 `# 제목` 이 사이드바 라벨이다 (괄호 · 대시 이후는 잘린다).
- `# 제목` 바로 아래 문단의 첫 문장이 그 페이지의 검색 · 공유 설명(`description`, `og:description`)이다(`site/src/lib/guide.ts`의 `descriptionOf`). 첫 문장은 페이지를 혼자서 설명하게 쓴다.

### 집필 규칙

독자는 **설치해서 쓰는 사람**이다. 다음은 가이드에 넣지 않는다: 소스 경로 · ADR · IPC 메서드
이름(CLI 명령으로 대신) · 내부 타입명 · Status/주체 같은 명세 머리 항목 · 수용 기준 · 구현 히스토리 ·
`docs/` 로의 링크. 메뉴 · 버튼 · 설정 항목은 `lang/ko.toml` 의 실제 라벨을 쓰고, 처음 등장할 때 영어
라벨을 HTML 주석으로 붙인다(`**설정** <!-- en: Settings -->`) — 번역이 그대로 쓴다.
확인 못 한 사실은 쓰지 말고 `<!-- TODO verify: … -->` 로 남긴다.

**문체는 합니다체다** — 마이크로소프트 한국어 스타일 가이드를 따른다. 사이트에 나가는 한국어는
가이드 본문도 랜딩 문구도 UI 문자열도 전부 `합니다` / `합니다체` 로 끝맺고, 사용자에게 시키는
문장은 `하십시오` 가 아니라 `하세요` 로 쓴다. 명사에 `수행` · `실행` · `제공` 을 붙이는 번역투는
동사로 푼다(`개요를 제공합니다` → `설명합니다`). 표 셀이나 목록의 명사구 조각은 그대로 둔다 —
종결어미가 있는 완전한 문장에만 해당한다. `docs/` 개발 문서는 이 규칙 밖이며 한다체를 유지한다
(독자가 다르다).

## 번역 모델

한국어(`site/content/`)가 canonical 이고 영어(`site/content/en/`)가 번역이다.

- **경로 1:1** — `site/content/en/<rel>` 이 `site/content/<rel>` 을 그대로 미러한다.
- **폴백** — 번역이 없는 페이지도 `/guide/` 에 발행된다. 한국어 본문 위에
  "아직 번역되지 않았습니다" 배너가 붙는다. 영어 트리는 **항상 완전**하다.
- **스탬프** — 번역 파일 첫 줄에 원본의 내용 해시를 박는다:

  ```
  <!-- source-hash: 90acc4edaa45 -->
  # Tasty guide
  ```

  원본이 바뀌어 해시가 달라지면 그 페이지에 "원문보다 오래됐습니다" 배너가 뜬다.
  스탬프가 없는 번역은 stale 로 본다.
- **링크 경로는 원본과 동일하게, 앵커는 영어 제목의 slug 로 쓴다.** 영어 페이지의 상대 링크는
  *한국어 위치 기준*으로 해석되므로 파일 경로는 그대로 둔다. 반면 `#앵커` 는 제목 텍스트에서
  만들어지므로 번역된 페이지의 앵커는 영어 slug 다(`install.md#설치-위치` → `install.md#install-locations`).
  `npm run check-links` 가 언어별 산출 트리에서 모든 `#앵커` 의 존재를 검사해 없으면 깨진
  링크로 집계한다.
- **고아 번역** — 대응하는 한국어 원본이 없는 `en/` 파일은 발행되지 않는다(경고 없이 빠진다 — 빌드는 `ORDER` 만 돈다).

### 번역 절차

```bash
# 1. 원본을 같은 상대경로에 번역해 쓴다 (링크 경로는 그대로, 앵커는 영어 slug)
$EDITOR content/en/using/terminal.md

# 2. 스탬프를 찍는다 (첫 줄에 삽입하거나, 이미 있으면 갱신)
npm run stamp content/en/using/terminal.md
```

`npm run stamp` 는 **번역을 실제로 손본 직후에만** 실행한다. 갱신 없이 스탬프만 찍으면 stale 표시만
사라지고 내용은 어긋난 채 남는다.

**콘텐츠의 내부 링크는 상대경로로 쓴다.** hast 플러그인이 그것을 라우트로 바꾸고, 그 라우트는
`site/src/lib/site.ts` 의 `url()` 을 통해 설정된 base 를 반영한다.

## 문서 변환 규칙

원본 마크다운은 고치지 않는다. 변환은 전부 렌더 단계 — Astro 7 의 기본 마크다운 처리기인
Sätteri 의 hast 플러그인(`src/lib/satteri-*.mjs`)에서 일어난다.

| 원본 | 산출 |
|------|------|
| `site/content/` 안의 `*.md` 링크 | 대응하는 라우트 (파일 깊이가 아니라 **라우트 깊이** 기준의 상대경로) |
| 콘텐츠 트리를 벗어나는 링크 (`../../../CHANGELOG.md`) | GitHub blob URL |
| 존재하지 않는 대상 · 없는 `#앵커` | `npm run check-links` 가 집계한다 |
| `##` · `###` | 우측 목차 + 앵커. slug 는 GitHub 규칙(한글 보존) |
| 표 | 가로 스크롤 컨테이너로 감싼다 |
| 코드 펜스 | Shiki 가 하이라이팅한다 — 색은 토큰이 정한다 |
| `<!-- tasty-diagram: <이름> -->` + 다음 코드 블록 | 그 블록을 킷 컴포넌트로 그린 다이어그램으로 교체 |

## 테마

**사이트는 색을 정의하지 않는다.** 팔레트는 `site/vendor/tokens/` 에서 오고, 그것이
앱 UI 킷이 렌더하는 것과 같은 원본이다 ([`site/vendor/README.md`](../../site/vendor/README.md)).
`site/src/styles/tokens.css`가 tier 순서(`primitives` → `semantic` → `components` → `base`)로 사본을 import하고, 레이아웃이 사이트 스타일보다 **먼저** import 한다.

- 테마 이름은 앱의 것을 그대로 쓴다 — **mocha**(다크, 기본) · **latte**(라이트).
  latte 만 `data-theme="latte"` 를 달고, mocha 는 속성 없는 `:root` 다.
- 코드 하이라이팅도 같은 토큰(`--tasty-color-mauve` · `-green` · `-peach` …)을
  참조하므로 테마 토글에 함께 반응한다.
- 색은 모두 vendor 토큰을 쓴다. **치수(간격·크기·반경)는 출처에 따라 두 척도로 나뉜다.**
  vendor 스케일은 앱 chrome 용이라(14px UI 상한, 조밀한 4/8/12 간격) 웹 본문에 그대로 쓰면
  읽히지 않기 때문이다.

선택은 `localStorage` 에 남고(`tasty-theme`), 저장값이 없으면 `prefers-color-scheme` 을
따른다. 첫 페인트 전에 인라인 스크립트가 적용해 깜빡임이 없다. 옛 빌드가 저장한
`"light"` · `"dark"` 도 그대로 해석된다.

### 치수 척도의 두 출처

판정 단위는 파일이 아니라 **규칙(선택자 블록)** 이다. 한 파일에 두 출처가 섞이면 규칙마다
판정하고, 한 규칙 안에서 출처가 갈리는 선언이 있으면 그 선언만 따로 판정한다. 물음은 하나다 — *Design System 이 킷에서 이 값을 바꾸면 이 규칙도 따라 바뀌어야
하는가.* 그렇다면 Design System 출처, 아니면 사이트 독자다.

| 출처 | 대상 | 쓰는 척도 |
|------|------|-----------|
| Design System | `site/vendor/` 사본과 거기서 파생한 파일(`kit.css` · `tokens.css` · 갤러리). 사이트 스타일시트 안에서는 킷 컴포넌트가 그리는 앱 화면을 재현하거나 그 값을 그대로 옮기는 규칙 — 랜딩·가이드의 제품 창 chrome(`app-shell.css` 의 `.shell` · `.shell__*` · `.shell-overlays*`), 킷 창 크기를 옮긴 Settings 무대(`.anat-settings__stage`), 갤러리 경로의 `gallery-site.css` · `vendor-stamp.css`, 디자인 섹션에서 iframe 으로 띄우는 킷 문서 | Design System 토큰 `--tasty-*` 의 semantic 층(`--tasty-space-*`, `--tasty-radius*`, `--tasty-font-size-<역할>` …). 토큰 파일 머리말대로 primitive(`--tasty-size-N` 등)는 직접 읽지 않는다 |
| 사이트 독자 | 사이트가 스스로 디자인한 것 — 페이지 레이아웃, 헤더 · 사이드바 · 목차, 본문 타이포그래피(`global.css`), 랜딩 섹션과 제품 창을 감싸는 카드(`landing.css` 의 `.window` 포함), 디자인 섹션의 머리 · 카드 · 타일 틀(`design.css`), 가이드 그림의 영역 라벨(`.anat*`)과 배치(`.workflow-diagram*`) | 사이트 척도 — 간격 `--sp-1`(4px) ~ `--sp-24`(96px), 반경 `--radius-sm` · `--radius` · `--radius-lg`, 요소 크기 `--size-control-sm` · `-md` · `-lg`(32 · 36 · 44px)와 `--size-icon-sm` · `-md` · `-lg` · `-xl`(16 · 18 · 20 · 24px), 레이아웃 변수(`--header-h`, 여러 규칙이 함께 쓰는 폭 `--page-max` 1440px · `--landing-max` 1180px 등). `global.css` 의 `:root` 가 정의한다 |

- 선언 단위로 갈리는 경우 — 제품 창을 **페이지에 얼마나 크게 보일지**(`.shell` 의 높이
  `clamp(…, vw, …)`, 가이드 그림의 높이 · 너비)는 킷이 정하지 않는 페이지 배치라 Design System
  출처 규칙 안의 선언이어도 사이트 독자 값으로 본다(hero 높이 clamp, 그림 높이 420 · 400 · 620px, 그림 폭
  820px, 작업 흐름 패인 최소 높이 190px). 반대로 킷이 정한 창 크기를 옮긴 값은 Design System 출처다 —
  Settings 무대는 `--tasty-settings-window-width` · `-height`, 제품 창 토스트의 하단 거리는
  `--tasty-toast-stack-offset-bottom` 을 읽는다.
- 갤러리 표본의 스타일시트(`vendor/gallery/parts/specimen.css`)는 `--tasty-*`만 쓴다. 갤러리의 페이지 배치(`site/src/styles/gallery-site.css`)는 사이트 독자 규칙이다.
- primitive 직접 참조 금지는 Design System 출처 규칙의 **치수**에만 적용한다. 색 토큰을 어느 층에서
  읽는지는 이 절이 다루지 않는다.
- 요소 크기 척도는 용도로 나눈다. `--size-control-*` 는 컨트롤의 높이 · 너비다 — `sm` 은 헤더의
  아이콘 버튼 · 언어 전환 · 검색 입력과 검색 아이콘 칸(좁은 화면에서 접힌 검색 폭 포함), `md` 는
  랜딩 기능 카드 칩과 터미널 그림 제목 막대, `lg` 는 랜딩 CTA 버튼이다. `--size-icon-*` 는 글리프
  크기다 — `sm` 헤더 아이콘 버튼과 검색 돋보기, `md` CTA와 기능 카드 칩, `lg` 플랫폼 줄, `xl`
  브랜드 마크다.
- 척도에 넣지 않는 값은 숫자로 남기고 파일 머리 주석에 그 범주를 적는다 — 글꼴 크기, 선 굵기,
  pill 반경, 그림의 세부인 장식(eyebrow 점 7px · 창 점 10px · 사이드바 스크롤바 8px), 한 곳에서만
  쓰는 레이아웃 폭(검색 기본 폭 220px, 펼친 검색 190px, 검색 결과 `min(460px, 82vw)`, 좁은 화면
  사이드바 `min(320px, 86vw)`, 푸터 열 최소 140px, 디자인 타일 최소 200px). `--sp-*` 는 간격
  척도라 크기 값에 빌려 쓰지 않고, `--size-*` 를 간격에 빌려 쓰지도 않는다.
- 상대 단위(`em` · `rem` · `ch`)인지는 한 물음으로 가른다 — *사용자가 브라우저 글꼴 크기를 키웠을 때
  이 값이 그대로면 글자와 어긋나는가.* 글줄 안에서 글자에 붙은 값(인라인 코드의 안쪽 여백
  `0.15em 0.4em`)과 글줄 길이를 정하는 값(본문 폭 `62ch` · `42em` · `34ch`)은 어긋나므로 상대 단위로
  두고 파일 머리 주석에 그 범주를 적는다. 블록과 블록 사이 간격은 어긋나지 않는다 — 문단 · 목록의
  위아래 여백(`.prose p` 의 `--sp-4`)과 같은 축이라 사이트 척도를 쓴다. 그림의 위아래 여백
  (`.prose img` · `.anat-window` · `.anat-settings` · `.workflow-diagram`)과 그림과 캡션 사이 여백이
  여기에 든다.
- 고른 척도에 대응 값이 없으면 값이 우연히 같은 다른 토큰·변수를 부르지 않고 숫자로 둔 채
  "대응 값 없음" 주석을 단다 — 없는 관계가 생기기 때문이다. Design System 출처인데 토큰이 없으면
  값을 로컬에서 정하지 않고 디자인 요청으로 올린다. 사이트 독자 척도에 필요한 값이 없어도 변수를
  즉석에서 만들지 않는다.
- 미디어 쿼리와 컨테이너 쿼리의 **조건**은 숫자로 남긴다. media feature 는 custom property 를
  읽지 못한다(`@media (max-width: var(--x))` 는 파싱되지 않는다). 블록 안의 선언은 위 규칙을 따른다.

## 랜딩의 다운로드 버튼

랜딩 hero 의 기본 다운로드 버튼(`#dl-primary`)은 처음에는 릴리스 페이지(`RELEASES_URL`)로 연결된다.
빌드 때 `site/src/lib/release.ts` 가 `release.json`(`pages.yml` 이 빌드 전에 사이트 디렉터리에 만드는 생성물)의 자산 이름을 파일명 접미사로 OS·아키텍처에
대응시키고, 자산이 있는 조합만 `data-<os>-<arch>` 속성으로 버튼에 싣는다.

| 속성 | 자산 접미사 |
|---|---|
| `data-macos-arm64` | `-macos-arm64.dmg` |
| `data-windows-x64` | `-windows-x64.msi` |
| `data-windows-arm64` | `-windows-arm64.msi` |
| `data-linux-x64` | `-x86_64.AppImage` |
| `data-linux-arm64` | `-aarch64.AppImage` |

브라우저에서는 `site.js` 가 OS 와 아키텍처를 판별해 두 값이 모두 맞는 속성이 있을 때만 버튼의 링크와
문구(`{os}용 다운로드`)를 바꾼다.

- OS 는 `navigator.userAgentData.platform` 또는 `navigator.platform` 으로 판별한다.
- Windows·Linux 의 아키텍처는 `navigator.userAgentData.getHighEntropyValues(["architecture", "bitness"])` 를
  먼저 사용한다. 64비트 `arm` 은 arm64, 64비트 `x86` 은 x64 로 본다. 이 API 가 아키텍처를 주면 그 값만
  따르며, 32비트 같은 다른 조합은 대응 자산이 없으므로 릴리스 페이지에 남는다.
- 이 API 가 없거나 거부되거나 빈 값을 주면 `navigator.platform`, 그다음 user agent 문자열에서
  `aarch64`·`arm64` 는 arm64, `x86_64`·`amd64`·`x64` 는 x64 로 읽는다. `armv8`·`armv8l` 처럼 32비트 ARM 환경일 수 있는 이름은 arm64 로 보지 않는다. 어느 쪽도 맞지 않으면
  아키텍처를 모르는 것으로 보고 릴리스 페이지에 남긴다.
- macOS 는 Apple silicon 산출물만 배포하고 Safari 가 아키텍처를 알려 주지 않으므로 아키텍처를 묻지 않고
  `data-macos-arm64` 를 사용한다.
- 브라우저가 보고한 값만 사용한다. Windows ARM 기기의 브라우저가 x64 라고 보고하면 x64 설치 파일로
  연결된다. 아키텍처를 판별하지 못한 방문자에게 다른 아키텍처의 파일을 대신 고르지 않는다.
- `release.json` 이 없거나 해당 조합의 자산이 없으면 속성이 없으므로 버튼은 릴리스 페이지로 간다.
  한국어·영어 랜딩은 같은 컴포넌트와 스크립트를 사용한다.

## 랜딩의 제품 창

랜딩 hero 의 창은 스크린샷도 전사본도 아니다 — 킷의 `app.jsx` 와 **같은 컴포넌트**를 같은
순서로 합성한 것이고(타이틀바 → 사이드바 | 탭 스트립 → 분할 서피스 → 상태바), 킷이 바뀌면
같이 바뀐다. `site/src/components/AppShell.jsx`, 프레임 스타일은 `site/src/styles/app-shell.css`.

**동작한다.** 탭을 고르고 닫고 열 수 있고(마크다운 탭은 패인을 마크다운 서피스로 바꾼다),
워크스페이스를 옮기고, 사이드바를 접고, 터미널 검색을 열고, 사이드바 버튼으로 Settings ·
Plugins · Ports · Remote · 커맨드 팔레트를 연다. 페이지이기 때문에 킷 프리뷰와 다른 점이 둘 있다.

- **테마는 페이지의 것이다.** `site.js` 가 `window.tastyTheme` 로 getter/setter 를 내주고
  셸의 상태바와 Settings 창이 그것을 통해서만 테마를 읽고 쓴다 — 헤더 토글과 어긋날 수 없고
  저장 규칙이 한 곳에 남는다.
- **오버레이는 `<body>` 끝의 고정 레이어로 나간다.** Settings 는 1100×700 이라 hero 가 셸에
  주는 프레임보다 크고, 도구 메뉴는 뷰포트 좌표에 앵커한다. 그림 안에 욱여넣어 줄이는 대신
  제 크기로 페이지 위에 띄운다.

랜딩의 AppShell과 갤러리처럼 상호작용이 필요한 곳은 React를 실행한다.
일반 가이드는 React 앱을 띄우지 않지만 테마·내비게이션을 위한 공용 사이트 스크립트는 사용한다.

## 디자인 섹션

디자인 시스템 자체를 발행하는 구역이다. 둘 다 `site/vendor/` 가 정본이고 사이트는 소비자다.

- **가이드라인** (`/design/`, `/design/tokens/`) — `vendor/guidelines/*.html`. 각 파일이
  독립 문서이고 첫 줄 `@dsCard` 주석이 그룹 · 이름 · 부제 · 뷰포트를 들고 있다. 클래스
  이름이 일반적(`.c` · `.l` · `.grid`)이라 인라인하면 서로 충돌하므로 파일로 서빙하고
  iframe 으로 싣는다. 선언한 뷰포트는 상한이 아니라 **하한**이다 — 프레임은 컬럼을 채우되
  그 폭 아래로는 안 내려가고, 좁으면 가로로 스크롤한다(두 줄로 접힌 색 램프는 램프가 아니다).
  `data-theme` 을 스스로 박은 카드(Latte 계열)는 그대로 두고, 나머지는 사이트 테마를 따른다.
- **갤러리** (`/design/gallery/**`) — `vendor/gallery/<페이지>/` 15 페이지. 살아 있는 표본이라
  호버 · 포커스 · 열고 닫기가 실제로 동작하고, 각 표본이 레이아웃 스펙과 소비하는 토큰을
  함께 적는다. 사이트 공용 머리글과 푸터를 쓰며 구현은 위 [갤러리 페이지](#갤러리-페이지) 절에 있다.

## 작업 흐름 가이드의 그림

`using/workflow-layouts`는 페인별 작업 배치와 프리셋 재사용을, `agents/background-tasks`는 숨겨진 PTY의 생성·입력·조회·탭 전환·정리를 안내한다. 두 페이지는 한국어와 영어로 제공한다.

`site/src/components/diagrams/WorkflowLayout.jsx`는 vendor의 TabStrip과 TerminalPane을 조합한 정적 예시다. Markdown의 `tasty-diagram: workflow-layout` 및 `tasty-diagram: background-task` 표식을 통해 렌더한다. 가이드의 캡션은 실제 앱이 아닌 배치 예시임을 명시하고, 랜딩은 `caption` 값으로 자기 문구를 넘긴다. 비활성 컨트롤은 inert 처리한다. 좁은 화면에서는 페인을 세로로 배치한다. 문서의 실제 조작 순서와 명령이 그림의 설명을 보완한다.

## 갤러리 개발 서버와 빌드 캐시

개발 서버를 켠 상태에서도 배포 빌드를 실행할 수 있도록 Vite 의 의존성 캐시는 command·mode 별로 분리한다. `site/astro.config.mjs`의 `tasty-isolate-dependency-cache` 플러그인이 이 경로를 설정한다. 개발용 JSX 코드는 React의 `jsxDEV`를 사용하므로 production 의존성 번들을 재사용하면 갤러리 hydration이 실패하고 지연 렌더링하는 Stage가 비게 된다.

갤러리 검증은 HTTP 응답과 pageerror만으로 판정하지 않는다. 콘솔의 hydration 오류, React 아일랜드의 SSR 상태 해제, 첫 Stage의 실제 컨트롤 표시, 아래쪽 Stage의 스크롤 후 표시를 함께 확인한다. 개발 서버를 유지한 채 build한 뒤에도 새 브라우저에서 같은 검사를 수행한다.

가이드 사이드바의 현재 페이지와 본문 목차의 현재 절은 디자인 시스템의 `--tasty-accent-primary`로 강조한다. 선택 배경도 같은 토큰에서 파생하며, 로고의 수박색인 `--brand`를 선택 상태에 사용하지 않는다.

## 랜딩의 혼합 서피스 예시

메인 AppShell은 터미널과 마크다운이 한 탭에 나란히 놓인 혼합 배치로 시작한다. 기존 에이전트 터미널과 README는 별도 탭으로 전환한다. vendor의 MarkdownSurface는 path·children·focused를 받아 주소바와 본문, 포커스 배경을 구분한다. 이 컴포넌트와 layouts 갤러리의 혼합 분할 예시는 같은 디자인 원본에서 가져온다.

사이트 소유 AppShell은 한국어·영어 문서 예시와 현재 CLI 문법을 사용하며, 예시 명령을 실제로 실행하지 않는다. 혼합 배치에서는 터미널이 포커스된 표면이고 검색과 상태바도 해당 표면을 가리킨다. 모바일에서는 두 서피스를 세로로 배치해 문서를 숨기지 않는다.
