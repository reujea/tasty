# ADR-0276: plugin 사용자 번역은 host 언어 루트에서 같은 순서로 읽는다

- **Status**: Accepted
- **Date**: 2026-09-15
- **Tags**: i18n, plugin, language-pack, compatibility
- **Group**: i18n

## Context

본체 언어팩은 host 카탈로그를 덮지만 plugin 프로세스는 설치본 lang만 읽었다.
설치 자산은 업그레이드 시 교체되므로 그 파일을 고치는 방식은 사용자 번역의 저장소가
될 수 없다. host 라벨과 SDK UI가 서로 다른 로더를 쓰면 같은 파일의 우선순위도 갈린다.

## Decision

내장 코드는 `lang/plugins/<plugin-id>/<code>.toml`, 새 언어팩은
`lang/<code>/plugins/<plugin-id>.toml`을 읽는다. 경로는 host 데이터 루트 기준이다.
기존 단일 파일 override와 디렉터리 pack 형상을 유지하고 plugin 자산은 수정하지 않는다.
host와 SDK가 공용 catalog loader로 설치본 영어 → 설치본 선택 언어 → 사용자 파일을
읽는다. 빈 overlay 값은 직전 층을 유지하고 사용자 파일에는 기존 2 MiB 상한을 적용한다.

SDK는 host가 이미 제공하는 `TASTY_PARENT_HOME`만 사용자 루트로 쓴다. 미주입 시
SDK 자신의 홈이나 cwd로 폴백하지 않는다. 언어 코드 문법과 기존 plugin ID 문법을
재사용하며 파일 경로에는 정상 컴포넌트만 허용한다. 매니페스트 ID 문법 자체를 좁히거나
host IPC 권한 모델을 바꾸지 않는다. 기존 host 카탈로그 우선 조회도 유지한다. plugin 키를
본체 팩과 plugin 파일에 중복하면 host 카탈로그가 이기므로 plugin 파일 한 곳에 모은다.

폰트 파일 읽기·검증은 `tasty_i18n::font::read_validated`로 공유한다. 부팅 resolve와
UI append가 같은 파서를 사용하고, append는 검증한 바이트를 그대로 삽입한다.
폰트 family resolve·폴백 순서·RTL 범위는 기존 계약을 유지한다.

## Consequences

- **얻은 것**: plugin 업그레이드와 분리된 번역 파일, host/SDK의 같은 fallback 순서,
  부모 홈 격리, 렌더러 없이 재사용할 수 있는 폰트 검사.
- **잃은 것**: plugin이 SDK 기본 로더 대신 자체 로더를 구현하면 자동 적용되지 않는다.
  plugin 전용 키를 본체 카탈로그에도 중복해서 넣는 것은 지원 경로가 아니다.
- **운영 비용 / 유지 부담**: 파일 배치와 크기/빈 값 규칙을 사용자 가이드에 설명한다.
  파일명 검사는 OS sandbox 또는 심볼릭 링크 격리를 보장하지 않는다.

## Alternatives Considered

- 설치본 lang 수정: 업그레이드가 사용자 변경을 지우므로 기각.
- SDK의 TASTY_HOME 사용: 부모 host와 다른 루트일 수 있어 기각.
- 호스트/SDK에서 각각 구현: 파일·fallback 판정이 갈라질 수 있어 공용 로더를 택했다.
- 전역 host namespace 우선순위 변경: 기존 카탈로그 충돌 동작을 깨므로 채택하지 않았다.

## Reconsideration Triggers

**원리적으로 안 붙는 것**

- plugin 번역을 본체 팩과 동시에 정의해야 하는 구체적 사용자 요구가 생긴다.
  재는 법: 사용자가 제공한 두 파일과 host/SDK의 실제 lookup 결과를 대조하여
  기존 본체 우선 정책을 유지할지 결정한다. 현재 그 요구를 판정하는 자동 채널은 없다.
- OS plugin sandbox 도입으로 부모 언어 파일 읽기가 막힌다.
  재는 법: 해당 sandbox에서 SDK 기본 로더로 자기 override 파일을 읽고 권한 실패를
  기록한다. 현재 OS sandbox는 제공되지 않아 이 실행 채널도 없다.

## References

- [i18n 개발 규칙](../dev-guide/i18n.md)
- [언어팩 기능](../features/language-packs/index.md)
- [plugin 권한](../dev-guide/plugin-permissions.md)
- 현재 구현: `crates/tasty-i18n/src/plugin_catalog.rs`의 `load`,
  `crates/tasty-plugin-sdk/src/i18n.rs`의 `Translator::from_plugin_env`.
