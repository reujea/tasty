# 윈도우 크롬 화면 (CSD 타이틀바)

- **부모 기획**: [../index.md](../index.md)
- **시각 소스**: `site/vendor/ui_kits/terminal/` (`chrome.jsx`, `titlebar/titlebar_linux.jsx`) — claude design

[MainView](../../main-view/screens/main-view.md) 최상단의 OS별 타이틀바. 동작은 부모 기획, 여기선 OS별 시각 배치.

## 트리거

윈도우가 열리면 항상 최상단에 표시(`top_inset` 만큼 작업 영역을 밀어냄).

## UI 요소 인벤토리 (OS별)

```
macOS  :  ●●●                          (OS 신호등, 좌상단 고정 · 나머지 드래그 영역)
Linux  :                    [ _ ] [ ▢ ] [ ✕ ]   (DE 가변 캡션, tasty 가 그림 · 우측 기본)
Windows:                    [ _ ] [ ▢ ] [ ✕ ]   (캡션 버튼 tasty, OS 캡션 제거 + 드롭섀도)
```

- **드래그 영역** — 타이틀바 빈 공간(이동/더블클릭 maximize). macOS 는 좌측 신호등 폭만큼 carve-out.
- **OS 컨트롤**:
  - macOS — **OS 네이티브 신호등**(tasty 가 그리지 않음).
  - Linux — tasty 가 그리는 DE 가변 버튼(기본 우측 min·max·close).
  - Windows — tasty 가 그리는 캡션 버튼(min/max/restore/close).
- **하단 1px 보더**(`titlebar_border` 토큰).
- 중앙 타이틀 텍스트 **없음**.

## 상태별 시각

- **활성 / 비활성 창** — `titlebar_bg` vs `titlebar_bg_inactive`, 버튼 글리프도 `titlebar_fg`(inactive) 차이.
- **maximize 상태** — Windows/Linux maximize 버튼 글리프가 restore 형태로 바뀜.
- **hover** — 캡션 버튼 hover 배경(close 는 강조색).

## 시각 소스

`design-system/ui_kits/terminal/chrome.jsx`(공통)·`titlebar_linux.jsx`(Linux 캡션) — 타이틀바 높이·색 토큰·버튼 배치의 단일 출처. macOS 신호등 geometry 는 OS 고정(테마 토큰 아님). (design-system vendor 후 링크 resolve.)
</content>
