# 탭 스트립 화면

- **부모 기획**: [../index.md](../index.md)
- **시각 소스**: `site/vendor/ui_kits/terminal/work.jsx` (탭 바 부분) — claude design

[작업 영역](../../work-area/screens/work-area.md) 안, 각 Pane 머리의 탭 바. 동작은 부모 기획, 여기선 시각.

## 트리거

Pane 이 존재하면 항상 그 위에 표시(Pane 마다 하나).

## UI 요소 인벤토리

```
┌ 탭 스트립 (Pane 하나) ─────────────────────────────────┐
│ [◀] [⬡ tab1 ●][⬡ tab2  ✕][⬡ tab3 ⚠] … [+] │ [⊟][🔍] [▶]│
└────────────────────────────────────────────────────────┘
  ◀▶ 스크롤   ⬡ kind 아이콘  ● busy  ✕ close  ⚠ 알림  + 추가  ⊟ split  🔍 search
```

- **탭** — leading **kind 아이콘** + 표시명. 상태 표지: **busy 녹색 점**, **알림 노란 라벨**. **활성 탭** 강조, **포커스 Pane** 여부로 스트립 배경(surface0 vs mantle) 구분.
- **close 버튼**(✕) — 활성 탭 또는 hover 시 우측에 노출.
- **`+` 추가 버튼** — 새 탭. 우클릭 = 프리셋 생성 메뉴.
- **스크롤 화살표**(◀▶) — 탭이 폭을 넘칠 때만.
- **우측 액션** — split(⊟) / search(🔍) 아이콘 → 해당 Pane 분할 / 활성 surface 검색.
- 탭 너비·라벨 폰트 크기는 **사용자 옵션**.

## 상태별 시각

- **활성 vs 비활성 탭** / **포커스 Pane vs 비포커스** — 배경·강조 차이.
- **busy / 알림** — 녹색 점 / 노란 라벨.
- **오버플로** — 스크롤 화살표 노출 + 가로 스크롤.
- **드래그 중** — 드래그 탭 overlay + drop 위치 표시.
- **close 노출** — 활성 또는 hover 시에만.

## 시각 소스

`design-system/ui_kits/terminal/work.jsx` 의 탭 바 — 탭 치수·아이콘·표지·간격의 단일 출처. (design-system vendor 후 링크 resolve.)
</content>
