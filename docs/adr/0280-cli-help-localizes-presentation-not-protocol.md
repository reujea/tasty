# ADR-0280: CLI 도움말은 표시 문구를 번역하고 프로토콜 값은 유지한다

- **Status**: Accepted
- **Date**: 2026-09-15
- **Tags**: cli, i18n, plugin, compatibility
- **Group**: i18n

## Context

CLI는 사람이 읽는 제품 표면이다. clap이 만든다는 이유만으로 Usage·Options·파싱 오류
안내 전체를 영어 예외로 두면 명령 설명을 번역해도 사용자가 읽는 안내는 완결되지 않는다.
반면 명령 이름, 플래그, 허용 값과 JSON-RPC 오류는 실행·통신 계약이므로 표시 문구와
구분해야 한다. plugin 매니페스트의 설명 키도 CLI가 소비하지 않으면 UI와 CLI가 갈린다.

## Decision

- host doc comment는 영어 원본으로 유지하고 모든 도움말 slot을 en/ko/ja에 둔다.
  카탈로그와 컴파일된 slot의 집합·영어 값 일치를 검사하여 새 누락과 오래된 키를 잡는다.
- CLI plugin discovery는 매니페스트의 `lang_dir`과 ID로 공용 `plugin_catalog::load`를
  호출한다. 설치 en → effective locale → host 홈의 사용자 plugin 파일 순서다.
  별도 plugin 프로세스를 띄우거나 전역 host namespace에 의존하지 않는다.
- 기존 `description_i18n_key`를 소비하고 인자에 선택적 `help_i18n_key`를 추가한다.
  키 미지정·번역 누락은 기존 description/help를 유지한다. 이전 매니페스트가 유효하다.
- 도움말 heading·생성된 help/version 설명·기본값/허용값 안내는 clap의 빌더·템플릿으로
  번역한다. 명령·옵션·인자 ID와 허용 값 토큰은 바꾸지 않는다. 영어 도움말은 기존
  clap 템플릿을 유지한다. 번역은 표시만 바꾸고 파싱 규칙을 변경하지 않는다.
- 파싱 오류는 clap `ErrorKind`와 `ContextKind`의 구조화된 정보로 로컬에서 표시한다.
  영어 오류 문자열을 검색·치환하여 분류하지 않는다. 분류·인자·값·추천 안내는 번역하며
  원래 입력과 허용 값은 그대로 표시한다. 하위 값 파서의 `Error::source`는 원인이 만든
  원문을 `Parser detail` 항목에 보존한다. 이것이 실제 서드파티 경계이며 Usage나
  Options처럼 제품이 소유할 수 있는 안내를 그 예외에 포함하지 않는다.
  clap 밖에서 반환된 `anyhow::Result`의 최상위 `Error:` 접두는 `main`의 Rust
  `Termination` 출력이다. 이 경계는 유지하며, plugin 인자 숫자 검증처럼 그 안에
  넣는 제품 문구는 기존 카탈로그로 번역한다. 이를 clap 파싱 오류와 혼동하지 않는다.
- JSON-RPC `error.message`의 기존 영어 wire 계약은 이 표시 변경의 대상이 아니다.
  서버가 만든 문구를 CLI 로케일로 치환하거나 새 오류 data 규약을 도입하지 않는다.
  기존 plugin이 돌려주는 오류의 정책도 유지한다.

## Consequences

- **얻은 것**: host·plugin 도움말과 파싱 오류의 로컬 언어, 같은 사용자 override 경로,
  기존 매니페스트 호환성, 설명 키 누락을 잡는 전수 검사.
- **잃은 것**: 비영어 파싱 진단의 서식은 clap 기본 서식과 다르다. 텍스트를 스크립트의
  판별자로 쓰면 안 되며 기존 종료 코드와 구조화된 IPC 응답을 사용해야 한다.
- **운영 비용 / 유지 부담**: 명령 추가 시 세 언어를 함께 갱신한다. clap의 새로운 오류
  종류는 공통 파싱 오류로 표시되므로 업그레이드 시 대표 오류 fixture를 다시 실행한다.
  하위 값 파서의 자유 형식 원문은 영어로 남을 수 있다.

## Alternatives Considered

- clap 전체를 서드파티 영어 예외로 유지: 제품 도움말 요구를 충족하지 못하므로 기각.
- 완성된 영어 출력에 문자열 치환 적용: 사용자 값·plugin 번역까지 바꿀 수 있어 기각.
- plugin 설명을 host 카탈로그에 합치기: 다른 plugin 키와 충돌하고 SDK 경로와 달라져 기각.
- 필수 인자 번역 키: 기존 매니페스트를 깨므로 선택 필드와 fallback을 택했다.
- wire 오류를 서버에서 번역: 원격 소비자·스크립트 계약을 바꾸므로 채택하지 않았다.

## Reconsideration Triggers

**원리적으로 안 붙는 것**

- clap이 공식 로케일 렌더러를 제공한다. 업그레이드 시 해당 API와 기존 fixture의 출력,
  exit code, 파싱 결과를 비교하여 자체 표시 계층을 대체할지 판단한다.
- 하위 값 파서 원문까지 번역해야 하는 구체적 사용자 사례가 생긴다. 입력·ErrorKind·
  source를 기록하여 구조화된 원인 분류가 가능한지 확인한다. 현재 이 요구의 자동 판정은 없다.

## References

- [CLI 구조](../dev-guide/cli-structure.md)
- [국제화](../dev-guide/i18n.md)
- [공용 plugin 카탈로그 결정](0276-plugin-user-catalogs-share-the-host-language-root.md)
- 현재 구현: `crates/tasty-cli/src/help_i18n.rs`의 `slots`,
  `crates/tasty-cli/src/help_frame.rs`의 `localize`,
  `crates/tasty-cli/src/help_error.rs`의 `render`,
  `crates/tasty-cli/src/dynamic/build.rs`의 `discover_plugin_clis`.
- 구현 당시 확인한 clap 4.6.0 소스: `Command::help_template`, `Command::build`,
  `Arg::hide_default_value`, `Arg::hide_possible_values`, `Error::context`.
