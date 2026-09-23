# 테마 추가/관리 (Themes)

- **Status**: Implemented
- **주체**: 로컬 사용자
- **ADR**: 없음
- **코드**: `tasty-themes`(scan/load/apply), `~/.tasty/themes/`
- **화면**: [설정 창](../settings/screens/settings.md) Appearance 탭

## 목적

색상 테마를 **TOML 파일로 추가·관리**하는 사용자 기능. 파일 하나가 한 테마, **파일명 stem = 테마 id**. 테마가 *무엇이고 어떻게 resolve 되는지*(2계층 base/overrides, 도출 overlay, sizing)는 [design/systems/theme](../../design/systems/theme.md) — 여기는 *사용자가 테마를 추가/적용하는 동작* 만.

## 내부 동작

### 위치 & 빌트인

`~/.tasty/themes/<id>.toml`(폴더 고정, 다른 경로 무시). 빌트인은 **앱 소유** — 부팅 시 임베드 정본과 동기화된다([design/systems/theme](../../design/systems/theme.md) "빌트인 테마 정책"):
- `mocha`: 항상 정본 보장(누락/파싱 실패/내용 불일치 시 임베드로 덮어씀).
- `latte`: first-run(빈 폴더)에만 자동 시드, 이후 파일 있으면 동기화·지우면 존중.
- 사용자 테마: 자동 동기화/복구 없음, 로드 실패 시 mocha fallback.

### TOML 포맷

`[palette]`/`[accent]`/`[terminal]`/`[ansi]`/`[surfaces.<id>]` + 선택 `label`·`is_light`. **모든 색 필드 optional** — 일부만 정의하면 누락분은 이전 테마 값 유지(partial 누적). HexColor 는 `#RGB`/`#RRGGBB`/`#RRGGBBAA`(8자리 = alpha). 자동 도출 색(`hover_overlay`/`active_overlay`/`separator`)과 UI sizing 은 TOML 에 없다(공통 SIZING). 상세 키 목록은 [design/systems/theme](../../design/systems/theme.md) "ThemeFile TOML".

> 빌트인(`mocha`/`latte`) 파일을 직접 편집하지 말 것 — 부팅 시 정본으로 되돌아간다. 커스텀은 별 id(`my-theme.toml`)로, 기존 테마 위 색 조정은 settings 의 picker(= `theme_overrides`)로.

### 적용 흐름

테마 변경 시: `theme_base` 에 새 partial 누적 → `theme_overrides`(picker 편집) **클리어**(설계) → 전역 Theme 갱신 → config 저장. 즉 테마를 바꾸면 직전 picker 편집은 사라진다(현재 테마 위 덧칠이라 의미를 잃음). 로드 실패/요청 id 부재 시 mocha fallback + InfoModal 알림.

## 인터페이스

- **사용자**: `~/.tasty/themes/<id>.toml` 추가 + Settings Appearance 에서 선택, picker 로 색 조정(→ overrides).

## 관련

- [design/systems/theme](../../design/systems/theme.md) — Theme 모델/resolve/도출/sizing(시스템) · [settings](../settings/index.md)
- [design/systems/theme › 색 생성 정책](../../design/systems/theme.md#색-생성-정책) — 색 생성 정책
