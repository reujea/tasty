# 리스닝 포트 팝업 (화면)

- **부모 기획**: [../index.md](../index.md)
- **시각 소스**: `site/vendor/ui_kits/terminal/overlays/port_scanner.jsx` (claude design)

## 트리거

사이드바 하단 **Tools 메뉴** → `Listening ports…` 클릭. Window 스코프 팝업으로 열린다.

## UI 요소 인벤토리

- **팝업 프레임**: 660×520 (디자인 canonical), headless.
- **헤더 행**: leading 포트 아이콘 + 제목 "Listening ports" + accent Tag(`{listening} listening` / `scanning…`) + 단일 라인 검색 입력 + 컬럼 chooser 아이콘 버튼 + Refresh 아이콘 버튼(상시 노출) + close(`×`).
- **필터 행**: `전체 보기 (system)` 체크박스 — scope 토글 (Tasty ↔ System).
- **즐겨찾기 섹션**: 필터 행과 테이블 사이, 항상 노출(bounded — 캡션 22px + 리스트 최대 112px). 좌측 "Favorites"(+개수) / 우측 "system-wide". 0개면 흐린 별(37%) + 안내 문구 1행. 1개 이상이면 행마다 별(항상 on, 클릭 시 제거) · `{addr}:{port}`(mono) · `{process} · {pid}`(+workspace) 또는 "not running" · 우측 상태 배지(LISTEN → running+pulse, 그 외 매칭 → waiting, 매칭 없음 → idle+`NONE`). 5행 초과 시 스크롤.
- **컬럼 chooser 팝업**: 헤더 컬럼 아이콘 버튼 클릭 시 열린다. 컬럼별 체크박스로 표시/숨김 토글. Port 는 잠금(항상 표시). leading fav 컬럼은 chooser 대상이 아니다(항상 표시).
- **테이블 (leading fav 컬럼 + 최대 7컬럼)**: 별(28px, 헤더 라벨 없음, 클릭 시 즐겨찾기 토글) + Port / Proto / Address / Process / Workspace / Tab / State (chooser 로 숨긴 컬럼은 제외).
  - 각 컬럼 최소폭 보유 — 보이는 컬럼 최소폭 합 > 본문 폭이면 본문이 **가로 스크롤**(말줄임 대신). sticky 헤더는 본문과 수평 동기 이동.
  - 정렬 가능 헤더(Port/Address/Process/Workspace/Tab) 클릭 시 `▲`/`▼` 인디케이터. Proto/State 헤더는 비정렬.
  - State 셀: 상태 dot(색 + pulse) + 상태 텍스트.
  - 행 클릭: 선택 토글(선택 행 강조). 브라우저 오픈 없음. 별 클릭은 행 선택에 영향을 주지 않는다.
- **footer**: `{shown} of {total} ports` 카운터 + `Copy address`(선택 없으면 disabled) + `Close`.

## 상태별 시각

부모 기획의 스캔 상태 → 화면 표현:

- **Loading**: 본문 중앙 `Spinner` + "Scanning…", footer 에 동일 메시지.
- **Ready**: 테이블 렌더.
- **Failed**: 에러 메시지 (재스캔은 헤더 Refresh 버튼으로).
- **빈 결과**: search_zero / system_empty / tasty_empty 각각의 메시지 (부모 기획의 3분기).

## 시각 소스

`design-system/ui_kits/terminal/overlays/port_scanner.jsx` — 팝업 치수·색·dot·레이아웃 수치의 단일 출처. 스크린샷: `design-system/assets/screens/port_scanner-*.png`. (design-system vendor 후 링크 resolve.)
