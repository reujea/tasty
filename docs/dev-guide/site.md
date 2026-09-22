# 공개 사이트 (GitHub Pages)

랜딩 페이지와 **사용자 가이드**를 정적 HTML 로 발행하는 경로. 산출물은 GitHub Pages 로 배포된다.

- 앱: `site/` (Astro + React, Node 22)
- 콘텐츠: `site/content/` (한국어 정본) · `site/content/en/` (영어 번역)
- 산출물: `site/` 아래 `dist/` — 빌드가 만들고 gitignore 다(경로 인용으로 안 적는다. 갓 클론한
  트리에는 없어서 좌표를 실재로 판정하는 가드가 CI 에서만 빨개진다)
- 배포: `.github/workflows/pages.yml` (main 에 `site/**` 변경이 푸시되면 자동) — `npm run build`
  다음에 `npm run check-links` 로 산출물의 내부 링크·앵커를 전수 판정하고 나서 올린다.
  그 스텝은 `site/content/` 의 앵커를 보는 **유일한** 판사다([ADR-0247](../adr/0247-site-anchors-are-judged-by-the-artifact-not-a-copy-of-the-rule.md))

**`docs/` 는 발행하지 않는다.** `docs/` 는 코드를 고치는 사람과 에이전트를 위한 명세·설계·ADR 이고,
사이트는 Tasty 를 받아서 쓰는 사람을 위한 것이라 독자가 다르다. 사이트가 실을 내용은 전부
`site/content/` 에 사용자 시점으로 따로 쓴다. 명세가 바뀌면 가이드도 같은 커밋에서 손본다
([`CLAUDE.md`](../../CLAUDE.md) "문서 갱신").

## 사이트 문구와 어조

사이트는 사용자가 하려는 일을 먼저 소개하고, 구체적인 사용 상황으로 기능을 설명한다.
한국어는 설명에 "합니다", 안내에 "하세요"를 사용한다. "이 페이지를 읽으면" 같은 반복적인
도입부, 과장된 보장, 내부 구현을 빗댄 표현은 피한다. 사용자 상태 보호는 포커스·선택·스크롤처럼
보장하는 범위를 명시하고, 연동 설정이 필요한 알림은 그 조건을 함께 설명한다.
크로스 플랫폼 소개는 모든 OS에서 동일한 동작과 작업 환경을 제공한다는 사용자 경험 원칙을 설명한다.

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
  scripts/vendor-to-esm.mjs   vendor -> ES module 변환기
  src/pages/            라우트
  src/components/ src/layouts/ src/lib/ src/styles/
  src/ds/ src/kit/ src/gallery/          ← 생성물 (gitignore)
  src/styles/tokens.css src/styles/kit.css  ← 생성물 (gitignore)
  public/design/                          ← 생성물 (gitignore)
```

킷은 브라우저가 Babel standalone 으로 통째로 읽는 것을 전제로 쓰였다. 파일마다 필요한 것을
전역에서 꺼내고 정의한 것을 전역에 얹는다 — **전역으로 쓰인 모듈 그래프**라 규칙적이고,
그래서 기계적으로 다시 쓸 수 있다.

| 원본 | 변환 |
|------|------|
| `const { Tab } = window.TastyDesignSystem_41fd3f;` | `import { Tab } from …/ds/…` |
| `const { Sidebar } = window.TastyKit;` | `import { Sidebar } from …/kit/…` |
| `window.TastyKit = Object.assign(window.TastyKit \|\| {}, { A, B });` | `export { A, B };` |
| `pluginAttentionCount: <식>` (계산값 등록) | `const pluginAttentionCount = <식>;` + export |
| `ReactDOM.createRoot(...).render(<App />)` (프리뷰 부트스트랩) | 제거하고 `App` 을 export |
| `const CSS = \`…\`` + 런타임 `<style>` 주입 | 생성되는 `styles/kit.css` 로 뽑아낸다 (주입은 그대로 둔다) |
| `const { Section } = window.Gallery;` | `import { Section } from "./shell.jsx"` |
| `Gallery.mount(active, nav, head, <Page/>)` | `export const page = { … }` + 라우트용 엔트리 컴포넌트 생성 |
| `preset_editor.jsx` 전체를 감싼 IIFE | 벗긴다 (import 는 최상위여야 한다) |
| 갤러리 산문의 `href="components.html#forms"` | `/design/gallery/components/#forms` |
| 가이드라인 문서의 `href="../styles.css"` | `public/design/tokens.css` + 테마 동기화 스크립트 주입 |

`site/vendor/components/` 는 손대지 않는다 — 이미 제대로 export 한다. Astro 프로젝트 루트
밖은 import 할 수 없어서 복사만 한다.

**생성물은 커밋하지 않는다.** 파생물이고 `npm run build` 가 `prebuild` 로 매번 다시 만든다.
정본은 `site/vendor/` 하나다.

### vendor 자신은 어디서 오는가

위 변환은 `vendor → src` 한 방향만 말한다. 그 앞에 **`원격 → vendor`** 방향이 하나 더 있고,
그쪽이 사람이 도는 단계다 — `site/vendor/` 는 원격 Claude Design 프로젝트의 사본이라,
디자인 결정이 착지하면 누군가 그것을 받아와야 한다. 절차는
[`site/vendor/README.md`](../../site/vendor/README.md) 의 "vendor 갱신 절차" 에 있고,
정합 루프에서 이 단계가 차지하는 자리는
[design-change-workflow](design-change-workflow.md#정합-대상--사이트-사본-필수) 에 있다. 결정 근거는
[ADR-0510](../adr/0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md).

따라오지 않아도 **사이트는 정상 빌드된다** — 낡은 사본을 성실히 렌더할 뿐이다. 그 침묵을
깨는 판정기가 둘 있고 둘 다 **레포 안의 두 사본만** 본다:
`site_vendor_tokens_track_the_app_export.rs` 가 토큰 **이름 집합**을,
`site_vendor_icons_match_the_app_transcription.rs` 가 아이콘 **기하**(와 채움), 그리고 그 기하가 담기는 **그릇**(`viewBox` · 선 굵기 · cap/join)을 본다.
그 둘이 안 여는 결정 — 문구 변경 · 구성 변경 · 컨트롤 삭제 — 은 어느 쪽 좌변도 안 움직이므로
**여전히 안 잡힌다.**

그래서 **그 사본에서 그리는 페이지가 사본의 시점을 화면에 적는다** — 갤러리 라우트와
디자인 섹션(`/design/` · `/design/tokens/`) 양쪽이다. 뒤엣것도 같은 사본에서 온다:
foundations 페이지가 `site/vendor/guidelines/` 의 문서들을, 토큰 페이지가 그중 하나를
렌더한다. 변환기가 빌드 시각에
`git log -1 -- site/vendor`(README 제외 — 그것은 절차지 스냅샷이 아니다) 로 날짜를 읽어
생성 트리에 스탬프 모듈 하나를 만들고, 공유 컴포넌트
[`VendorStamp`](../../site/src/components/design/VendorStamp.jsx) 가 페이지 머리에 그린다 —
갤러리는 영어 한 판으로, 디자인 섹션은 페이지의 언어로. 문구가 한 자리인 이유는 그것이 한
사실이기 때문이고, 그 규칙이 자기 스타일시트
([`vendor-stamp.css`](../../site/src/styles/vendor-stamp.css))에 사는 이유도 같다 — 두 영역이
서로 다른 스타일시트를 읽어서, 두면 사본이 둘이 된다.

**문구는 "받아온 날" 이 아니라 "마지막으로 바뀐 날" 이라고 말한다.** `site/vendor/` 는 한
시점의 깨끗한 스냅샷이 아니라 한 번 들여온 뒤 군데군데 기워진 상태이고, 그래서 그 트리의
최신 커밋 날짜는 **마지막 기움의 날짜**다. 그것을 "스냅샷" 이라 적으면 이 표시가 막으려는
바로 그 종류의 거짓이 된다.
**그 날짜를 손으로 적지 않는다** — 손으로 적으면 그것이 또 하나의 낡을 사본이 되고,
하필 사본이 낡는 그 순간에 "최신" 이라고 말한다. git 이 없으면(소스 tarball · shallow
clone) 추측하지 않고 줄 자체를 안 그린다.

## 빌드

```bash
cd site
npm install          # 최초 1 회

npm run build        # prebuild(vendor 변환) 후 astro build -> site/dist/
npm run dev          # 개발 서버
npm run preview      # 빌드 결과 확인
npm run vendor-to-esm  # prebuild 없이 변환만 다시 돌린다

# 번역 파일에 원본 해시 스탬프 (아래 "번역 모델")
npm run stamp content/en/index.md
```

`npm run build` 는 `prebuild` 로 [`site/scripts/vendor-to-esm.mjs`](../../site/scripts/vendor-to-esm.mjs)
를 먼저 돌린다 — `site/vendor/` 를 ES module 과 토큰 CSS 로 바꾸고 로고를 `public/` 에 놓는다.
`npm run vendor-to-esm` 은 같은 변환을 단독으로 돌리는 별칭이다. **재-vendoring 이 아니다** —
`site/vendor/` 를 갱신하지 않고 그것을 읽어 생성 트리만 다시 만든다(원격에서 받아오는 쪽은
위 "vendor 자신은 어디서 오는가").

## URL 구조

| 경로 | 내용 |
|------|------|
| `/` | 영어 랜딩 |
| `/ko/` | 한국어 랜딩 |
| `/guide/**` | 영어 가이드. `site/content/en/**.md` 가 있으면 번역, 없으면 한국어 본문 + 배너 |
| `/ko/guide/**` | 한국어 가이드 (`site/content/**.md`, canonical) |
| `/design/` · `/ko/design/` | 디자인 가이드라인 (브랜드 · 색 · 타이포 · 간격). 크롬만 이중언어고 싣는 문서는 같다 |
| `/design/tokens/` · `/ko/design/tokens/` | 3티어 토큰 문서 |
| `/design/gallery/**` | 레퍼런스 갤러리 12 페이지. 갤러리 자체 크롬을 쓰므로 언어별로 나누지 않는다 |
| `/design/cards/*.html` | 가이드라인 원본 문서. 페이지가 iframe 으로 싣는다 (생성물) |
| `/search-index.json` (en) · `/search-index.ko.json` | 검색 인덱스 |

### 배포 경로 (base)

레포는 GitHub **프로젝트 페이지**로 발행된다 — `https://zilhak.github.io/tasty/` 라 도메인
루트가 아니라 **레포 이름 아래**에서 서빙된다. 그 경로는 [`site/src/lib/base.mjs`](../../site/src/lib/base.mjs)
**한 곳에만** 적고, 나머지는 전부 거기서 파생한다.

| 소비자 | 어떻게 받는가 |
|--------|---------------|
| 페이지·컴포넌트의 모든 내부 링크 | `astro.config.mjs` 의 `base` → `import.meta.env.BASE_URL` → `site/src/lib/site.ts` 의 `url()` |
| 개발 서버 | 같은 `base` — `astro dev` 도 그 경로 아래에 마운트한다 (`/tasty/`). **로컬과 배포가 같은 값으로 돈다** |
| 마크다운 파이프라인의 다이어그램 | 직접 주입받는다. Astro 설정에서 로드돼 `BASE_URL` 이 닿지 않는 컨텍스트라 그 경계에서 한 번 적용한다 |
| `npm run check-links` | 직접 import 한다. 여기서 base 를 모르면 배포가 못 여는 링크를 통과시킨다 |
| vendor 변환기가 만든 킷·갤러리 모듈 | `import.meta.env.BASE_URL` 을 읽는 `BASE` 상수를 헤더에 넣어준다 |
| `public/design/cards/*.html` (정적 파일) | 상대경로. `import.meta.env` 를 못 읽으므로 문서 위치 기준으로 쓴다 |

**어떤 경로도 두 번 적지 않는다.** 커스텀 도메인으로 옮기면 `base.mjs` 의 값을 `""` 로 바꾸는
것이 전부다.

영어가 기본 URL 인 이유는 공개 사이트의 첫 방문자 대부분이 영어권이고 랜딩도 `/` 가 영어라서다.
정본이 한국어인 것과는 별개다 — 작성은 한국어로, 노출 기본은 영어로.

## 콘텐츠 구조

`site/content/` 의 디렉토리가 사이드바 섹션이고, 페이지 순서는 `site/src/lib/guide.ts` 의 `ORDER` 가
정한다. 파일 이름 순이 아니다 — 읽는 순서가 의도이므로 배열로 박는다.

```
site/content/
  index.md                      가이드 홈
  getting-started/  install · first-look
  using/            workspaces · panes-tabs-splits · terminal · files
  customize/        keybindings · settings · themes · scripts
  agents/           cli · claude-codex · tasks · hooks-notifications
  remote/           attach
  plugins/          index
  help/             troubleshooting
  en/               위 트리의 1:1 미러 (번역)
```

- 새 페이지를 추가하면 `ORDER` 에도 넣는다. 빠뜨리면 발행되지 않는다.
- 새 디렉토리(섹션)를 만들면 `SECTIONS` 에 (디렉토리, 한국어 라벨, 영어 라벨) 을 추가한다.
  등록되지 않은 디렉토리에 페이지를 두면 빌드가 실패한다.
- 페이지 첫 줄 `# 제목` 이 사이드바 라벨이다 (괄호 · 대시 이후는 잘린다).

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
- **고아 번역** — 대응하는 한국어 원본이 없는 `en/` 파일은 경고를 내고 발행되지 않는다.

### 번역 절차

```bash
# 1. 원본을 같은 상대경로에 번역해 쓴다 (링크 경로는 그대로, 앵커는 영어 slug)
$EDITOR content/en/using/terminal.md

# 2. 스탬프를 찍는다 (첫 줄에 삽입하거나, 이미 있으면 갱신)
npm run stamp content/en/using/terminal.md
```

`--stamp` 는 **번역을 실제로 손본 직후에만** 실행한다. 갱신 없이 스탬프만 찍으면 stale 표시만
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
변환기가 tier 순서(`primitives` → `semantic` → `components` → `base`)로 이어붙여
생성되는 `styles/tokens.css` 한 파일로 내보내고, 레이아웃이 사이트 스타일보다 **먼저** import 한다.

- 테마 이름은 앱의 것을 그대로 쓴다 — **mocha**(다크, 기본) · **latte**(라이트).
  latte 만 `data-theme="latte"` 를 달고, mocha 는 속성 없는 `:root` 다.
- 코드 하이라이팅도 같은 토큰(`--tasty-color-mauve` · `-green` · `-peach` …)을
  참조하므로 테마 토글에 함께 반응한다.
- `site/src/styles/global.css` 는 **치수만 자기 것을 갖는다.** vendor 스케일은 앱 chrome 용이라
  (14px UI 상한, 조밀한 4/8/12 간격) 웹 본문에 그대로 쓰면 읽히지 않는다. 색은 공유하고
  간격·폰트 크기는 공유하지 않는다.

선택은 `localStorage` 에 남고(`tasty-theme`), 저장값이 없으면 `prefers-color-scheme` 을
따른다. 첫 페인트 전에 인라인 스크립트가 적용해 깜빡임이 없다. 옛 빌드가 저장한
`"light"` · `"dark"` 도 그대로 해석된다.

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

랜딩은 사이트에서 **유일하게 하이드레이트하는 라우트**다 — gzip 94 KB(react-dom 44, 킷과
오버레이 46). 가이드 36 페이지는 여전히 JS 를 하나도 싣지 않는다.

## 디자인 섹션

디자인 시스템 자체를 발행하는 구역이다. 둘 다 `site/vendor/` 가 정본이고 사이트는 소비자다.

- **가이드라인** (`/design/`, `/design/tokens/`) — `vendor/guidelines/*.html`. 각 파일이
  독립 문서이고 첫 줄 `@dsCard` 주석이 그룹 · 이름 · 부제 · 뷰포트를 들고 있다. 클래스
  이름이 일반적(`.c` · `.l` · `.grid`)이라 인라인하면 서로 충돌하므로 파일로 서빙하고
  iframe 으로 싣는다. 선언한 뷰포트는 상한이 아니라 **하한**이다 — 프레임은 컬럼을 채우되
  그 폭 아래로는 안 내려가고, 좁으면 가로로 스크롤한다(두 줄로 접힌 색 램프는 램프가 아니다).
  `data-theme` 을 스스로 박은 카드(Latte 계열)는 그대로 두고, 나머지는 사이트 테마를 따른다.
- **갤러리** (`/design/gallery/**`) — `vendor/gallery/*.jsx` 12 페이지. 살아 있는 표본이라
  호버 · 포커스 · 열고 닫기가 실제로 동작하고, 각 표본이 레이아웃 스펙과 소비하는 토큰을
  함께 적는다. 갤러리는 자기 크롬(좌측 레일 · 상단 토글)을 가진 **애플리케이션**이라 사이트
  문서 레이아웃에 얹지 않는다 — 내비게이션과 스크롤바가 둘씩 생기고, `gallery.css` 가
  `html, body` 를 잡는다. 바꾸는 것은 크롬 주변뿐이다: 내비는 이 사이트 라우트를 가리키고,
  브랜드와 상단 바에 `/design/` 로 돌아가는 길이 있고, 테마 토글은 사이트 키를 쓴다.

## 작업 흐름 가이드의 그림

`using/workflow-layouts`는 페인별 작업 배치와 프리셋 재사용을, `agents/background-tasks`는 숨겨진 PTY의 생성·입력·조회·탭 전환·정리를 안내한다. 두 페이지는 한국어와 영어로 제공한다.

`site/src/components/diagrams/WorkflowLayout.jsx`는 vendor의 TabStrip과 TerminalPane을 조합한 정적 예시다. Markdown의 `tasty-diagram: workflow-layout` 및 `tasty-diagram: background-task` 표식을 통해 렌더한다. 캡션에 실제 앱이 아닌 배치 예시임을 명시하고, 비활성 컨트롤은 inert 처리한다. 좁은 화면에서는 페인을 세로로 배치한다. 문서의 실제 조작 순서와 명령이 그림의 설명을 보완한다.

## 갤러리 개발 서버와 빌드 캐시

개발 서버를 켠 상태에서도 배포 빌드를 실행할 수 있도록 Vite 의 의존성 캐시는 command·mode 별로 분리한다. `site/astro.config.mjs`의 `tasty-isolate-dependency-cache` 플러그인이 이 경로를 설정한다. 개발용 JSX 코드는 React의 `jsxDEV`를 사용하므로 production 의존성 번들을 재사용하면 갤러리 hydration이 실패하고 지연 렌더링하는 Stage가 비게 된다.

갤러리 검증은 HTTP 응답과 pageerror만으로 판정하지 않는다. 콘솔의 hydration 오류, React 아일랜드의 SSR 상태 해제, 첫 Stage의 실제 컨트롤 표시, 아래쪽 Stage의 스크롤 후 표시를 함께 확인한다. 개발 서버를 유지한 채 build한 뒤에도 새 브라우저에서 같은 검사를 수행한다.

가이드 사이드바의 현재 페이지와 본문 목차의 현재 절은 디자인 시스템의 `--tasty-accent-primary`로 강조한다. 선택 배경도 같은 토큰에서 파생하며, 로고의 수박색인 `--brand`를 선택 상태에 사용하지 않는다.

## 랜딩의 혼합 서피스 예시

메인 AppShell은 터미널과 마크다운이 한 탭에 나란히 놓인 혼합 배치로 시작한다. 기존 에이전트 터미널과 README는 별도 탭으로 전환한다. vendor의 MarkdownSurface는 path·children·focused를 받아 주소바와 본문, 포커스 배경을 구분한다. 이 컴포넌트와 layouts 갤러리의 혼합 분할 예시는 같은 디자인 원본에서 가져온다.

사이트 소유 AppShell은 한국어·영어 문서 예시와 현재 CLI 문법을 사용하며, 예시 명령을 실제로 실행하지 않는다. 혼합 배치에서는 터미널이 포커스된 표면이고 검색과 상태바도 해당 표면을 가리킨다. 모바일에서는 두 서피스를 세로로 배치해 문서를 숨기지 않는다.
