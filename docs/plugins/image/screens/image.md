# Image surface 화면

- **부모 기획**: [../index.md](../index.md)
- **시각 소스**: plugin egui-mesh 자가 렌더 (비트맵=egui 텍스처) — `design-system/` 의 image surface 디자인(있으면), vendor 예정.

[작업 영역](../../../features/work-area/screens/work-area.md) 타일 안에 열리는 이미지 뷰어 / 그림판 surface.

## 트리거

이미지 파일 열기, `image` surface 생성/전환, 또는 빈 캔버스.

## UI 요소 인벤토리

- **이미지 뷰** — 로드된 이미지 표시(맞춤/확대 등).
- **빈 캔버스** — 파일 없이 시작한 그림판.
- 탭 표시명은 파일명(빈 캔버스면 기본 "Image").

## 상태별 시각

- 로드됨 / 빈 캔버스 / 로드 실패.

## 디자인 토큰 매핑

시각 수치·토큰의 단일 출처는 `design-system/` 이다 — [시각 소스](#시각-소스).

## 갤러리 specimen

`crates/tasty-gallery/src/catalog/components/image_viewer.rs` — Layouts › `Content viewers` ›
`Image surface / canvas`. viewer(그림 fit) / no-image(fallback glyph) 두 상태를 토큰으로 전사.
3자 매핑: [design-gallery-mapping.md](../../../design/systems/design-gallery-mapping.md#surface-viewers-plugins).

## 시각 소스

plugin 이 host 가 forward 한 Theme 토큰으로 자가 렌더. design-system vendor 후 링크로 교체.
