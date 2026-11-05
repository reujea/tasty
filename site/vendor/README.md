# 디자인 시스템 vendor

Design System Artifact **Tasty Design System**에서 받아온 디자인 사본이다. 사이트의 랜딩 데모와 가이드 그림은 이 사본의 컴포넌트를 사용한다. 실제 앱 구현과 자동으로 동기화되지는 않는다.

디자인 수정은 원격 킷에 먼저 반영한 뒤 이 사본을 갱신한다. **이 디렉토리의 파일을 직접 고치지 않는다.** 아래 "원본과 다르게 둔 자리"의 예외와 이 README만 직접 수정할 수 있다. `MANIFEST`는 갱신 절차의 스크립트가 쓴다.

## vendor 갱신 절차

사이트 사본은 **릴리스할 때** 갱신한다([릴리스 절차](../../docs/dev-guide/release.md#3-사이트-갱신)). 디자인이 바뀔 때마다 갱신하지 않는다. 빌드와 아래 비교 검사는 일부 차이만 검출하므로, 통과했다고 사본이 최신인 것은 아니다.

앱 토큰 사본의 갱신 절차는 `crates/tasty-design-tokens/README.md`에 있다. 앱은 `tokens/tasty.tokens.json` 한 파일을 가져오지만, 사이트는 원격 파일 목록을 비교하고 트리 전체를 가져온 뒤 로컬 변형을 다시 적용한다. 결정 이유와 대안은 `docs/design/systems/theme.md`에 있다.

1~4단계는 Artifact를 읽을 수 있어야 한다. 주소는 커밋하지 않는 로컬 지침에 있다. 접근할 수 없으면 갱신하지 못한 범위를 보고한다. 5단계부터는 로컬 사본만으로 실행할 수 있다.

1. **파일 목록을 비교한다.** `Artifact`의 `list`(`scope: "files"`)로 받은 `project/` 아래 경로와 `find site/vendor -type f`로 얻은 로컬 목록을 비교한다. 새 파일, 삭제된 파일, 이름이 바뀐 파일을 확인한다. 아래 "원본에서 제외한 것"에 해당하는 경로는 비교에서 뺀다.
2. **변경된 파일을 받는다.** `Artifact`의 `read`에 `url`과 `paths`(`project/…`, 호출당 최대 256개), `out_dir`을 넘긴다. 파일은 `<out_dir>/project/…`에 저장된다. 앞의 `project/`를 뗀 폴더를 4단계에 넘기고, 변경된 파일을 사본의 대응 경로에 쓴다. 원격에서 삭제된 파일은 사본에서도 지운다.
   - 응답에 작은 텍스트 파일 본문이 함께 붙어 대화 문맥을 쓰므로 하위 Agent에 맡기는 편이 낫다. 없는 경로는 그 항목만 실패한다.
   - 사본과 원격의 경로가 다른 곳은 `screens/X`(원격 `assets/screens/X`), `components/<그룹>/X.d.ts`(원격 `components/X/X.d.ts`), `guidelines/brand-logo.html`(원격 `archived/guidelines/brand-logo.html`)이다. 대응은 `site/scripts/vendor-manifest.mjs`의 `remotePathOf`에 있다.
   - 사본에는 이번 갱신 작업을 시작한 뒤 받은 결과만 쓴다.
3. **로컬 변형을 다시 적용한다.** 아래 "원본과 다르게 둔 자리"의 세 가지(로컬 문서 인용, 옛 ADR 번호 인용, 출처 메타데이터 제거)를 적용한다. `cargo test -p tasty-doc-guards --test no_todo_file_citation`으로 로컬 작업 문서 인용이 남지 않았는지 확인한다.
   - 그다음 사본을 원격과 대조한다. 범위는 최소한 디자인 작업 결과(`.DONE`)가 적은 파일과, 그 파일을 쓰는 갤러리 페이지가 불러오는 파일이다. 가능하면 vendor 전체를 원격과 바이트 단위로 비교한다.
   - 다른 곳이 있으면 각 차이가 "원본과 다르게 둔 자리"의 변형인지 줄 단위로 확인한다. 변형으로 설명되지 않는 차이는 옛 판이 남은 것이므로 원격 판으로 다시 받는다.
   - `MANIFEST`에 이유가 적힌 `variant` 파일은 원격이 크게 바뀌어도 4단계 스크립트가 실패하지 않는다. 원격 해시가 달라진 `variant`는 반드시 이번 차이를 직접 읽는다.
4. **`MANIFEST`를 다시 쓴다.** 2단계에서 받은 원본 폴더를 넘겨 스크립트를 실행한다.

   ```sh
   cd site && node scripts/vendor-manifest.mjs --remote <받은 원본 폴더> [--reason <경로>=<이유>]...
   ```

   - 원본 폴더에는 이번 갱신에서 받은 사본 대상 파일이 모두 있어야 한다. 바뀌지 않은 파일도 받아 둔다. 빠진 파일이 있으면 스크립트는 실패하고 아무것도 쓰지 않는다.
   - 로컬과 원격 해시가 같은 파일은 `<경로> TAB <원격 sha256>`으로 적는다. 다른 파일은 `variant` 표시와 로컬 sha256, 이유 한 줄을 함께 적는다. 이유는 기존 `MANIFEST`의 것을 유지하며, 새 변형은 `--reason`으로 넘긴다. 이유가 없는 차이가 있으면 스크립트는 실패한다. 그 차이는 3단계에서 설명하지 못한 옛 판이다.
   - 원본 폴더에만 있는 파일은 경고로 출력한다. "원본에서 제외한 것"에 해당하는지, 새로 가져올 파일인지 확인한다.
   - `crates/tasty-doc-guards/tests/site_vendor_manifest_matches.rs`는 사본의 파일 집합과 해시를 `MANIFEST`와 비교한다. 기본 줄은 원격 해시와, `variant` 줄은 기록한 로컬 해시와 같아야 하고 이유가 비어 있으면 안 된다. 원격은 조회하지 않으므로, 통과는 마지막 갱신 때 기록한 내용 그대로라는 뜻이며 원격이 그 뒤에 바뀌지 않았다는 뜻은 아니다.
5. **토큰과 아이콘을 비교한다.** 다음 검사를 실행한다.

   ```sh
   cargo test -p tasty-doc-guards --test site_vendor_manifest_matches --test site_vendor_tokens_track_the_app_export --test site_vendor_icons_match_the_app_transcription
   ```

   - 토큰 검사는 앱 사본과 사이트 사본의 **토큰 이름 집합** 차이를 명부와 비교한다. 갱신으로 해소된 차이는 명부에서 지운다.
   - 아이콘 검사는 `icons/*.svg`, `components/core/Icon.jsx`의 `ICON_PATHS`, 앱의 `crates/tasty-icons/src/lib.rs`에 있는 기하를 비교한다. 이름 대응은 명부를 사용한다. 킷의 `list`는 앱의 `log`에, `listView`는 앱의 `list`에 해당한다. 아이콘 추가·삭제 시 명부도 갱신한다.
   - `<svg>`의 `viewBox`, 선 굵기, cap/join도 비교한다. 의도적인 색(`stroke`/`fill`) 차이는 사유와 함께 명부에 남기고, 해소되면 지운다.
   - 이 검사들은 토큰·아이콘을 바꾸지 않는 문구·구성·컨트롤 변경을 검출하지 않는다. **최신 여부는 1~4단계에서 원격과 직접 비교한다.**
6. **사이트를 빌드하고 검사한다.** `cd site && npm run build && npm run check-links && npm run check-contrast`. 사이트는 사본을 변환 없이 import하므로 사본의 JSX는 ES 모듈이어야 한다.
7. `site/vendor/` 변경과 `MANIFEST`, 필요한 명부 갱신을 같은 커밋에 담는다.

## 구성

| 경로 | 내용 |
|------|------|
| `styles.css` | 토큰 진입점. 사용하는 쪽은 이 파일만 링크한다 |
| `tokens/` | 3단계 토큰: `primitives` → `semantic` → `components`, 공통 `base` |
| `components/` | 공용 위젯(Button, Tab, Kbd, Badge, Table 등) |
| `ui_kits/terminal/` | 앱 셸(`app`, `chrome`, `work`, `titlebar/`)과 오버레이 |
| `icons/` | 아이콘 SVG |
| `screens/` | mocha UI 스크린샷. 라이트 테마 대응본은 없음 |
| `guidelines/` | 독립 HTML 디자인 문서. 첫 줄 `@dsCard` 주석에 그룹·이름·뷰포트를 기록함 |
| `gallery/` | 컴포넌트 갤러리. 페이지마다 `<페이지>/index.jsx`가 있고 공용 부품은 `parts/`에 있다 |

## 원본에서 제외한 것

- **웹폰트(D2Coding, 8.3MB)**: `styles.css`의 `@import url("tokens/fonts.css")`를 제외한다. sans는 시스템 폰트를 사용하고 mono는 `"SF Mono"`, `"Cascadia Code"` 등의 대체 폰트를 사용한다. 브랜드 글꼴 통일이 필요하면 woff2 서브셋 도입을 검토한다.
- **`components/bundle.js`, `components/bundle.css`, `components/index.d.ts`**: 브라우저에서 직접 실행하는 전역 번들과 합본이다. 사이트는 `components/`를 직접 번들하므로 필요 없다.
- **`tokens.css`, `tokens.json`, `design-system.json`, `assets/notes/`, `archived/` 중 사본 경로에 없는 파일**: Artifact 형식 파일과 보존용 자료다. 1단계 토큰은 사본에서 `tokens/primitives.css`에 함께 둔다.
- **`components/<이름>/README.md`, `components/<이름>/preview.html`**: 디자인 시스템 페이지의 안내와 미리보기다.
- **`icons.json`**: `icons/*.svg`에서 생성한 아이콘 이름·그룹·역할·`paths`·`fill` 명부다. 사이트는 `ICON_PATHS`를 사용하므로 추가 사본을 두지 않는다. 앱 아이콘은 이 명부를 참고해 옮긴 것이며, `crates/tasty-doc-guards/tests/site_vendor_icons_match_the_app_transcription.rs`가 `ICON_PATHS`와 앱 구현을 비교한다.
- **킷 `README.md`, 컴포넌트 번들**: `ui_kits/terminal/README.md`와 `components/bundle.js`는 디자인 도구용이다. 사이트는 `.jsx`를 직접 import한다.
- **갤러리 보조 파일**: `gallery/coverage.md`와 `gallery/contract-audit.js`는 디자인 검토용이다. 사이트의 검사는 `scripts/check-contrast.mjs`다.
- **`tokens/tasty.tokens.json`**: DTCG 원본이다. 앱이 가져오는 사본은 `crates/tasty-design-tokens/dtcg/tasty.tokens.json`에 두고(갱신 절차는 `crates/tasty-design-tokens/README.md`), 사이트는 CSS 토큰만 쓴다. `tokens/fonts.css`는 위 웹폰트 항목으로 제외한다.
- **컴포넌트 보조 파일**: `components/**/*.prompt.md`(디자인 도구용 사용 안내)와 `components/*/*.card.html`(디자인 시스템 패널 카드)은 사이트가 쓰지 않는다.
- **`changelog/`**: 원격 디자인 결정문이다. 저장소의 결정은 `docs/`와 ADR에 둔다.
- **루트 문서와 도구 파일**: `README.md`, `FEATURES.md`, `CLAUDE.md`, `SKILL.md`, `TOKENS.md`, `Canvas.dc.html`, `thumbnail.html`, `.thumbnail`, `support.js`, `_ds_manifest.json`, `_adherence.oxlintrc.json`. 원격 프로젝트 운영용이며 사이트가 쓰지 않는다. 진입점 `styles.css`만 가져온다.
- **작업 폴더**: `design-request/`, `design-tasks/`, `explorations/`, `assets/explorations/`, `assets/scraps/`, `assets/screenshots/`, `assets/uploads/`. 요청·작업 기록과 시안 초안이다.
- **`fonts/`, `assets/icons/`**: 폰트 원본은 웹폰트 항목으로 제외한다. `assets/icons/`는 앱 아이콘 원본(`.icns`·`.ico`·PNG·멜론 SVG)과 보조 SVG라 사이트가 쓰지 않는다. `assets/screens/`는 `screens/`로 가져온다.

## 원본과 다르게 둔 자리

기본적으로 원격 파일을 그대로 가져온다. 갱신할 때는 다음 예외를 다시 적용한다. 변형한 파일은 `MANIFEST`에 `variant`로 기록되며, 파일별 이유는 그 줄에 있다.

- **`tokens/primitives.css`**: 원격은 1단계 값을 `tokens.css`로 분리하고 `@import`한다. 사본은 값을 이 파일 한 곳에 둔다. 갱신할 때 두 원격 파일과 값이 같은지 비교한다.
- **커밋되지 않는 로컬 문서 인용**: 원격 주석·노트에 있는 로컬 작업 폴더와 티켓 인용은 뜻만 남기고 고친다. `crates/tasty-doc-guards/tests/no_todo_file_citation.rs`가 검사한다. 렌더링 구조와 값은 유지한다.
  - `tokens/semantic.css`: `--tasty-text-disabled` 주석 끝의 원격 changelog 인용("see changelog/…")을 지운다.
  - `tokens/components.css`: 주석의 원격 요청문서 인용(`design-request/…`)을 지운다. 날짜는 남긴다.
  - `tokens/ground-ink-rules.json`: `measure.boundary`의 원격 changelog 인용을 지운다.
- **출처 메타데이터**: 렌더링에 쓰지 않는 base64 C2PA 매니페스트가 사본 크기를 늘리므로 `<metadata>` 요소와 `xmlns:c2pa` 속성을 제거한다. 원격에서 이 메타데이터가 있는 아이콘은 일부(`icons/` 아래 agent·arrowRight·filePlus·fit·folderPlus·globe·globeOff·info·link·sun)다. 렌더링 구조와 값은 유지한다. `grep -rl c2pa site/vendor/ --exclude=README.md`의 출력이 없어야 한다. README와 `MANIFEST`는 사본을 설명하는 파일이므로 이 검사와 사본 수정일 계산에서 제외한다.

## 통합 시 주의

`ui_kits/terminal/app.jsx`는 마운트할 때 `document.documentElement.dataset.theme`을 자신의 state로 덮어쓴다. 사이트는 별도 래퍼에서 테마를 props로 전달해 사이트 테마 토글과 충돌하지 않도록 한다.

## 로컬 인용만 정정할 때

허용된 옛 ADR 인용 정정은 렌더링 값·구조를 바꾸지 않는 로컬 변형이다. 새 원격 사본을 받지 않고 이 변형만 적용할 때는 기존 원격 해시를 유지한 채 다음 도구로 지정 파일의 로컬 해시와 이유만 갱신한다. 이는 원격의 최신 여부를 확인하는 명령이 아니다.

```sh
cd site
node scripts/vendor-manifest.mjs --local-variant tokens/primitives.css=Current-document-reference
```

여러 파일은 옵션을 반복한다. `--check`는 기록된 로컬 해시와의 일치만 검사하며 `--remote`와 함께 사용할 수 없다. 새 파일·원격 내용 변경은 기존 전체 갱신 절차를 따른다.
