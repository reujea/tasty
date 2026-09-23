# 플러그인 관리 창 화면

- **부모 기획**: [../index.md](../index.md)
- **트리거 위치**: [사이드바](../../sidebar/screens/sidebar.md) 하단 **플러그인 버튼**
- **시각 소스**: `site/vendor/ui_kits/terminal/overlays/plugins_window.jsx` — claude design

## 트리거

사이드바 하단 **플러그인 버튼** 클릭 → 플러그인 관리 모달 창이 열린다.

## 레이아웃

```
┌──────────────────────────────────────────┐
│ [Installed]  [Attention]  [Install]       │  탭
├──────────────────────────────────────────┤
│ Installed:                                │
│  ▸ plugin A          [enable ▢]  [⌫]      │  목록 — 토글 / uninstall
│    권한: …(read-only)                      │
│  ▸ plugin B  ⚠health  [enable ▣]          │
│                                            │
│ Install:                                  │
│  경로: …/tasty-plugin.toml                 │
│  매니페스트 미리보기 + 권한 미리보기          │
│  [신뢰 검증] → [ Add ]                      │
└──────────────────────────────────────────┘
```

## UI 요소 인벤토리

- **탭**: Installed / Attention(확인 필요 — 등록 거부·실행 실패 plugin, 개수 danger 배지) / Install.
- **Installed 항목**:
  - enable/disable 토글, health error 인디케이터(오류 플러그인).
  - 권한 표시 — **read-only** (창에서 권한 토글 없음).
  - install dir 열기, **uninstall**.
- **Install 폼**:
  - 디렉터리 경로(`tasty-plugin.toml`).
  - 매니페스트 + 권한 **미리보기**.
  - **신뢰/서명 상태** — Trusted 면 바로 Add, 권한 변경 시 재신뢰(TrustAndInstall) 후 Add.
- **설정(configure)** 은 여기 없음 → [설정 창](../../settings/screens/settings.md) Plugins 탭.

## 상태별 시각

- **health error**: enable 상태인데 오류인 플러그인에 빨간 인디케이터/박스.
- **신뢰 상태**: Trusted / PermissionsChanged 등에 따라 Install 버튼·안내 문구가 달라진다.

## 시각 소스

`site/vendor/ui_kits/terminal/overlays/plugins_window.jsx` — 창 치수·탭·목록·설치 폼 배치의 단일 출처. 스크린샷: `site/vendor/screens/plugins_window-installed.png`, `plugins_window-install.png`, `plugins_window-modal.png`.
