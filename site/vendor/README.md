# 디자인 시스템 vendor

Claude Design 프로젝트 **Tasty Design System** 에서 받아온 사본이다. 사이트가 앱 UI 를
그림이 아니라 **실제 컴포넌트**로 보여주기 위해 쓴다 — 랜딩의 제품 창, 가이드의 UI 설명이
모두 여기서 온다.

사이트가 정본이 아니다. 킷이 바뀌면 여기를 갱신하고, 반대 방향으로는 흐르지 않는다.
**이 디렉토리의 파일을 손으로 고치지 않는다** — 고칠 곳은 원격 킷이고, 여기는 받아오는 자리다.
(예외는 아래 "원본과 다르게 둔 자리" 둘과 이 README 자신뿐이다.)

## vendor 갱신 절차

디자인 결정이 착지하면 이 사본도 **같이** 따라와야 한다. 따라오지 않으면 코드가 맞아도
공개 사이트는 결정 이전 UI 를 현재형으로 전시하고, 빌드도 시험도 CI 도 전부 초록이다 —
낡았다는 사실이 아무 값으로도 안 남는다. 그래서 절차를 적는다.

앱 쪽 토큰 사본의 같은 절차는 `crates/tasty-design-tokens/README.md` 에 있다. 형태는 같지만
**모수가 다르다** — 저쪽은 파일 하나(`tokens/tasty.tokens.json`)이고 여기는 트리 전체다.
그 차이가 아래 1~3 단계(목록 회수 · 개별 수신 · 로컬 변형 재적용)를 만든다.

이 절차를 세운 근거·대안·재검토 조건은 `docs/adr/0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md`.

> **★ 여기서부터 1~3 단계는 DesignSync 세션이 필요하다.** 원격 Claude Design 프로젝트에
> 접근할 수 없는 세션에서는 이 단계를 돌 수 없다. 그때는 **조용히 건너뛰지 말고** 그 사실을
> 보고에 남긴다 — 능력 없음과 완료는 다르다. 4 단계부터는 세션 권한 없이 돈다.

1. **원격 파일 목록을 회수한다.** `DesignSync.list_files` 로 프로젝트 전체 경로를 받는다.
   여기는 파일 하나가 아니라 트리라서, **먼저 목록을 받아 구조 차분을 낸다** — 사본에 없는
   새 파일, 원격에서 사라진 파일, 이름이 바뀐 파일이 이 차분에서만 드러난다. 사본 쪽 목록은
   `find site/vendor -type f` 로 낸다.
   - 아래 "원본에서 제외한 것" 에 해당하는 경로(웹폰트 · `_ds_bundle.js` · 프리뷰
     `index.html` · 킷 `README.md`)는 차분에서 뺀다. 그것들은 원격에만 있는 것이 정상이다.
2. **파일을 개별로 받아 덮는다.** `DesignSync.get_file` 은 경로 하나씩이라, 1 단계 차분에
   오른 파일과 변경된 파일을 **하나씩** 받아 같은 상대 경로에 쓴다. `get_file` 은 256 KiB
   상한이 있으므로, 넘는 파일이 있으면 그 사실을 적고 그 파일만 따로 처리한다.
   - 원격에서 사라진 파일은 사본에서도 지운다. 남겨 두면 사이트가 원격에 없는 화면을
     계속 전시한다.
3. **로컬 변형 2 곳을 다시 적용한다.** 아래 "원본과 다르게 둔 자리" 의 두 파일은 덮어쓰면
   변형이 날아간다. 덮은 뒤 반드시 다시 적용하고, `cargo test -p tasty-doc-guards --test
   no_todo_file_citation` 으로 확인한다 — 이 검사가 그 변형이 존재하는 이유다.
4. **두 대조를 돌린다.** `cargo test -p tasty-doc-guards --test
   site_vendor_tokens_track_the_app_export --test
   site_vendor_icons_match_the_app_transcription`.
   - 앞엣것은 앱 사본과 이 사본의 **토큰 이름 집합** 차이를 명부와 대조한다. 재-vendoring
     으로 따라온 만큼 그 명부에서 지워야 하고, 다 따라왔으면 명부가 빈다.
   - 뒤엣것은 **아이콘 기하**를 본다 — 이 사본 안의 두 자리(`icons/*.svg` 와
     `components/core/Icon.jsx` 의 `ICON_PATHS`)끼리, 그리고 앱 전사본
     (`crates/tasty-icons/src/lib.rs`)과. 짝은 이름에서 도출하지 않고 명부로 적는다
     (이 킷의 `list` 는 앱의 `log` 이고 `listView` 가 앱의 `list` 다). 글리프가 늘거나
     줄면 그 명부를 함께 옮긴다.
   - 같은 대조가 기하가 담기는 **그릇**도 본다 — 여는 `<svg …>` 의 `viewBox` · 선 굵기 ·
     cap/join 이다. 그 한 글자가 글리프 전부를 다르게 그리는데 기하 대조만으로는 안 보인다.
     색(`stroke`/`fill`)은 갈리는 것이 정상이라 판정기의 명부에 사유와 함께 적혀 있고,
     그 갈림이 사라지면 명부에서 지워야 한다.
   - **이 검사들이 초록이라고 "사본이 최신" 이 되는 것은 아니다.** 토큰도 아이콘(기하·
     그릇)도 안 건드리는 결정 — 문구 변경 · 구성 변경 · 컨트롤 삭제 — 은 양쪽 좌변을
     똑같이 남겨두므로 안 잡힌다. 그 층을 닫는 것은 1~3 단계뿐이다.
5. **사이트를 빌드해 확인한다.** `cd site && npm run build`. 변환기
   (`scripts/vendor-to-esm.mjs`)가 새 파일을 모르는 형태면 여기서 걸린다.
6. `site/vendor/` 변경 + (필요하면) 위 명부 갱신 + 변환기 수정을 **같은 커밋**으로
   커밋한다. 생성 트리(`site/src/{ds,kit,gallery}/` 등)는 커밋 대상이 아니다.

## 구성

| 경로 | 내용 |
|------|------|
| `styles.css` | 토큰 진입점. 소비자는 이 파일만 링크한다 |
| `tokens/` | 3티어 토큰 — `primitives` → `semantic` → `components` + `base` |
| `components/` | 공용 위젯 (Button · Tab · Kbd · Badge · Table …) |
| `ui_kits/terminal/` | 앱 셸 (`app` · `chrome` · `work` · `titlebar/`) 과 오버레이 |
| `icons/` | 아이콘 SVG |
| `screens/` | UI 스크린샷 (mocha 전용 — 라이트 테마 대응본 없음) |
| `guidelines/` | 디자인 가이드라인 문서. 각 파일이 독립 HTML 이고 첫 줄 `@dsCard` 주석이 그 문서의 그룹·이름·뷰포트를 들고 있다 |
| `gallery/` | 레퍼런스 갤러리. 킷과 같은 방식(전역으로 쓴 모듈 그래프)이라 같은 변환을 거쳐 생성 트리로 나온다. `gallery.css` 는 갤러리 자체의 크롬 스타일이라 갤러리 라우트에서만 로드한다 |

## 원본에서 제외한 것

- **웹폰트 (D2Coding, 8.3MB)** — `styles.css` 의 `@import url("tokens/fonts.css")` 를 뺐다.
  sans 는 원래 시스템 폰트 스택이고 mono 도 폴백 체인(`"SF Mono"` · `"Cascadia Code"` …)이
  있어 빠져도 렌더가 깨지지 않는다. 브랜드 통일이 필요해지면 woff2 서브셋으로 되돌린다.
- **`_ds_bundle.js`** — 브라우저 직접 실행용 전역 번들. 사이트는 `components/` 를 직접
  번들하므로 필요 없다.
- **`icons.json`** — `icons/*.svg` 에서 생성된 machine-readable 매니페스트(이름 · 그룹 ·
  역할 · `paths` · `fill`). 기하의 **셋째 사본**이라 들이면 맞춰야 할 자리가 하나 늘어나는데,
  사이트는 그것을 소비하지 않는다(`components/core/Icon.jsx` 의 `ICON_PATHS` 를 번들한다).
  앱 쪽 `crates/tasty-icons` 가 이 매니페스트를 보고 **손으로 전사**한 것이지만, 그 대조는
  사본을 하나 더 두지 않고 `ICON_PATHS` 를 좌변으로 삼아
  `crates/tasty-doc-guards/tests/site_vendor_icons_match_the_app_transcription.rs` 가 한다.
- **프리뷰 `index.html`** — 위 번들에 의존하는 킷 자체 미리보기. 원본에만 둔다.
  `ui_kits/terminal/overlays/*.html` · `titlebar/*.html` 의 컴포넌트별 단독 미리보기와
  킷 `README.md` 도 같은 이유로 가져오지 않는다 — 사이트는 `.jsx` 만 변환해 쓴다.

## 원본과 다르게 둔 자리

사본은 원격 파일을 그대로 옮기는 것이 기본이다. 아래만 예외로, 갱신할 때 원격 파일로
덮은 뒤 다시 적용한다.

- **커밋되지 않는 로컬 문서를 가리키는 문구** — 원격 킷의 주석·노트 문자열에 이 레포에
  커밋되지 않는 로컬 작업 폴더나 그 안의 티켓을 가리키는 자리가 있다. 레포는 그런 언급을
  추적 파일에 들이지 않으므로(`crates/tasty-doc-guards/tests/no_todo_file_citation.rs`)
  사본에서는 뜻만 남기고 문구를 바꾼다. 렌더되는 구조와 값은 건드리지 않는다.
  - `ui_kits/terminal/overlays/settings_window.jsx` — Hook Handlers 서브탭 설명 주석의
    "see … todo" → "not yet built".
  - `gallery/components.jsx` — AutoComplete 노트의 "tracked as a separate implementation
    TODO" → "tracked as separate implementation work".
- **출처 메타데이터(`<metadata>` 안의 C2PA 매니페스트)** — 원격에서 Claude 가 다시 저장한
  파일에는 서명된 출처 매니페스트가 base64 로 실려 온다. 그것은 디자인 내용이 아니라 그
  파일이 어떻게 만들어졌는지의 기록이고, 사이트는 렌더에 쓰지 않는다. 크기 차가 커서 그냥
  둘 수 없다 — `icons/sun.svg` 의 경우 매니페스트가 붙은 원본이 **15.9 KB** 인데 `icons/`
  **65 개 전부의 합이 17.3 KB** 다. 한 아이콘이 트리 전체만 해지고 그대로 공개 사이트로
  나간다. 그래서 받아올 때 `<metadata>` 요소와 그것을 위한 `xmlns:c2pa` 속성만 벗기고
  나머지는 그대로 옮긴다. **렌더되는 구조와 값은 건드리지 않는다.**
  - 벗겼는지 확인: `grep -rl c2pa site/vendor/ --exclude=README.md` 가 아무것도 안
    내놔야 한다. 이 README 를 빼는 이유는 시점 표시가 `git log -1 -- site/vendor` 에서
    README 를 빼는 이유와 같다 — 절차를 적은 문서는 사본의 일부가 아니다.

## 통합 시 주의

`ui_kits/terminal/app.jsx` 는 마운트할 때 `document.documentElement.dataset.theme` 을
자기 state 로 덮어쓴다. 사이트의 테마 토글과 충돌하므로, 사이트에 얹을 때는 테마를
props 로 주입받도록 감싼다.
