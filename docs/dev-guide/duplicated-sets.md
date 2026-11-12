# 중복 목록 관리

같은 사실을 여러 코드·문서에서 유지할 때 무엇을 합치고 무엇을 비교할지 정한다.
현재 문장을 고치는 일반 규칙은 [문서 작성 규칙](../documentation-model.md)에 있다.

## 중복 목록을 정리할 때 확인할 것

같은 모양의 목록이라도 소비자가 다른 질문에 답하면 값을 같게 만들지 않는다.
같은 목록을 사용자 문서, 플랫폼별 코드, 매니페스트가 각각 필요로 한다면 각 형식은 유지하고
누락과 불필요한 항목을 양방향으로 비교한다. 항목 개수만 같다는 것은 충분하지 않다.

비교 전에 식별자, 번역할 자리표시자, 설명 문장을 구분한다. `<경로>`와 `<path>`를
같게 만들라고 요구하면 올바른 번역을 막는다. 접두사나 glob 규칙은 이름을 하나씩 적은
목록과 다르므로, 유한한 대상으로 펼칠 수 없으면 포함 관계만 검사한다.
같은 파일의 cfg별 배열이나 끊어진 논리 표도 별개 표현일 수 있으므로 이름과 헤더로
대상을 정확히 선택하고 옆의 다른 표를 읽지 않는지 확인한다.

여러 사본이 같다는 사실만으로 모두 옳다고 단정하지 않는다. 실제 매니페스트나 구현과
비교하고, 두 사본만 다르면 어느 것이 기준인지 근거로 정한다.
파서 유지 비용이 규칙보다 크면 검사가 없다는 사실과 수동 확인 방법을 남긴다.

## 검사를 작성할 때

상수 배열, 표의 한 열, 설정 키 목록처럼 항목을 이름으로 열거한 부분을 비교한다.
런타임에만 만들어지는 값이나 산문의 예시는 같은 방식으로 셀 수 없다.
비교할 영역은 헤더와 상수 이름으로 정하고, 그 영역만 읽는지도 검사한다.

내용 개수가 그대로인 채 항목 하나를 바꿔 검사가 실패하는지 확인한다.
파일이나 빌드 산출물 자체가 근거라면 다른 사본끼리만 비교하지 말고 그 실물과 대조한다.
필요한 사본은 독자에게 맞는 형식으로 유지한다.
생성할 수 있는 단순 복사본은 원본에서 생성해 수동 수정 지점을 줄인다.

예외를 분류할 새 타입은 대상이 많다는 이유만으로 만들지 않는다.
서로 다른 이유가 섞여 검사가 잘못된 근거를 적용할 때 필요한 분류를 드러낸다.

## 라벨 글꼴의 언어 집합

`font-label`이 UI sans를 읽는 언어(`ja`, `zh`)는 디자인 정본 `vendor/tokens/base.css`의 `:lang()` 규칙과 `crates/tasty-ui-widgets/src/label_font.rs`의 상수 두 곳에 있다. 내장 언어 목록의 부분 집합이므로 `BUILTIN_CODES`와 같은 집합인지 보는 `tests/i18n_language_lists.rs`에는 넣지 않는다. 대조는 `label_font_languages_match_the_design_source`가 한다.

조회는 언어 코드만 받고 `BUILTIN_CODES`를 읽지 않으므로 언어를 등록해도 이 코드를 고치지 않는다. 대신 `a_language_the_mono_face_cannot_hold_reads_the_ui_font`가 내장 목록을 돌며, 카탈로그에 D2Coding에 없는 글자가 있는데 라벨이 고정폭인 언어를 찾는다.

## macOS 원격 HTTP viewer 하한

원격 HTTP(S) viewer를 선언하는 macOS 하한은 `src/host_api/webview/macos/http.rs`의 `FIRST_MEASURED`(macOS 메이저 버전)다. 문서는 이 값을 지원 표기(`macOS 27 이상`)와 미지원 범위(`macOS 11–26`)로 여러 곳에 산문으로 적는다. `macos_http_viewer_floor_matches_the_source`가 docs 전체의 `macOS 11–N` 범위 상한을 상수보다 1 작은 값과 대조한다. 그래서 docs 안에서 `macOS 11–N` 형태는 이 미지원 범위에만 쓰고, 다른 사실(per-store proxy API가 없는 버전 등)은 "macOS 14 이전"처럼 적는다. 지원 표기와 `macOS 14–26` 같은 다른 범위는 검사하지 않으므로 하한을 바꿀 때 `git grep -nE "macOS (1[1-9]|2[0-9])"`로 함께 확인한다.

## 상태 모델의 상태 목록

[상태 모델](../concepts/state-model/index.md)은 코드의 상태 목록을 표 두 개로 옮겨 적는다. surface 문서의 활성화 단계 전이 표는 `crates/tasty-core/src/evolve/lifecycle.rs`의 `phase_transition`과 `crates/tasty-core/src/operation.rs`의 `ActivationPhase`, workspace 문서의 연결 상태 표는 `src/app/auto_attach/row_use.rs`의 `row_state_name`과 `src/runtime/engine_read.rs`의 `remote_row_state`가 실물이다. `state_model_lists_match_the_source`가 두 표를 집합으로 대조한다. 전이 하나 삭제, 단계 추가, 상태 문자열 변경, 사이드바 대응 변경을 코드와 문서 양쪽에서 바꿔 검출하는 것을 확인했다. 시험은 대상 파일 전체에서 주석과 리터럴(일반·바이트·C 문자열, 해시 개수를 맞춘 원시·바이트 원시·C 원시 문자열, 문자·바이트 문자 리터럴) 내용을 공백으로 가린 같은 길이의 사본을 만들고, 함수 선언과 범위·`ActivationPhase` 선언과 변형 이름(속성 제외)·첫 `match` 블록·괄호 깊이 0의 쉼표로 나눈 갈래·화살표를 모두 이 사본에서 찾은 뒤 문자열 값만 같은 위치의 원본에서 읽는다. 그래서 여러 줄로 나뉜 OR 패턴(`Some("a" | "b")`, `Some("a") | Some("b")`)도 모든 문자열을 읽고, 주석 속 화살표는 갈래로 읽지 않는다. 선언은 키워드(`fn`·`enum`)와 이름으로 찾고 스코프는 따지지 않으므로, 가시성·제네릭·수명 매개변수와 상관없이 같은 이름의 함수나 `ActivationPhase` enum 선언이 파일에 둘 이상이면 시험이 실패한다. 화살표 없는 갈래, `remote_row_state`의 가드 갈래·문자열 없는 패턴·`S::<이름>`이 아닌 값, `row_state_name`의 문자열 리터럴 하나가 아닌 값은 시험을 실패시킨다. 두 함수 밖이나 두 번째 `match`의 갈래는 읽지 않는다. 산문 문장에 적은 목록(`ChildState`·`SessionState`·`RetiringView`의 값)은 검사하지 않으므로 그 enum을 바꿀 때 `docs/concepts/state-model/`을 함께 검색한다.
