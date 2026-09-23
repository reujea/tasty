# 명령 팔레트 화면

- **부모 기획**: [../index.md](../index.md)
- **트리거 위치**: `toggle_command_palette` 단축키 · [도구 메뉴](../../tools-menu/screens/tools-menu.md) `Command palette`
- **시각 소스**: `site/vendor/ui_kits/terminal/overlays/command_palette.jsx` — claude design

## 트리거

단축키(`toggle_command_palette`) 또는 도구 메뉴 항목 → 화면 중앙에 팔레트 popup.

## 레이아웃

```
┌──────────────────────────────────┐
│ 🔍 (검색 입력)                     │
├──────────────────────────────────┤
│ ▸ New workspace       [Alt]+[N]   │  후보 행: 아이콘 + 라벨 + 키캡
│   Split pane          [Alt]+[E]   │
│   Toggle settings                 │
│   …                               │
├──────────────────────────────────┤
│ ↑↓ navigate  ↵ run  esc close     │  footer: 목록 바로 아래
└──────────────────────────────────┘
```

## UI 요소 인벤토리

- **검색 입력** (상단): 쿼리 입력. 즉시 필터.
- **후보 리스트**: 각 행 = leading 아이콘(디자인 명시 명령은 전용 아이콘, 나머지는 fallback) + 명령 라벨 + 우측 키캡. 선택 행 강조, `↑/↓` 이동.
- **키캡**: 첫 바인딩을 키 하나마다 별도 키캡으로 그리고 사이에 `+` 를 둔다 — 상태바·메뉴와 같은 공용 `Kbd` 위젯이라 치수·색이 한 곳에서 나온다.
- **footer**: 네비게이션 힌트 한 줄. 목록 바로 아래에 붙는다.

## 상태별 시각

- **빈 쿼리**: 전체 후보. **검색 중**: 점수순 필터. **결과 0**: 빈 목록.
- **카드 높이**: 표시 항목 수를 따라 매 프레임 달라진다 — 쿼리로 후보가 줄면 카드도 줄고 footer 가 마지막 행 바로 아래로 올라온다. 목록이 상한에 닿으면 그 위는 스크롤이다. 카드 위쪽 가장자리는 열 때 정한 자리에 고정돼, 타이핑 중에 검색창이 움직이지 않는다.

## 시각 소스

`design-system/ui_kits/terminal/overlays/command_palette.jsx` — 팔레트 치수·행·아이콘·바인딩 표기의 단일 출처. 스크린샷: `design-system/assets/screens/statusbar-palette-chip.png`. (vendor 후 resolve.)
