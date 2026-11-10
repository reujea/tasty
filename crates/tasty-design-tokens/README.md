# tasty-design-tokens

디자인 정본의 DTCG 토큰을 저장하고 Rust 상수와 Theme 접근자를 생성한다.
배율 적용과 색 접근 규칙은 `src/lib.rs`의 크레이트 문서를 따른다.

디자인 정본은 공개 레포 [tasty-page](https://github.com/zilhak/tasty-page)의 `main`에 있는 `vendor/`다.
`dtcg/SOURCE`에는 앱이 따르는 tasty-page 커밋 해시(40자) 한 줄을 적는다.

## 토큰 갱신 절차

1. tasty-page `main`의 커밋 X에서 `vendor/tokens/tasty.tokens.json`을 가져와
   `dtcg/tasty.tokens.json`을 교체하고 `dtcg/SOURCE`에 X를 적는다. 이 JSON은 디자인 측 CSS에서
   생성하므로 누락된 토큰이나 값을 직접 채우지 않는다.
2. 커밋 X로 둔 tasty-page 체크아웃에서 JSON과 토큰 CSS를 비교한다. 이름·값·별칭·Latte 값을
   양방향으로 비교하고 차이가 있으면 1로 끝난다.

   ```sh
   python3 <tasty-page>/scripts/design-tokens/verify_tokens_json.py <tasty-page>/vendor crates/tasty-design-tokens/dtcg/tasty.tokens.json
   ```

   차이가 있으면 갱신을 중단하고 디자인 측에 JSON 재생성을 요청한다.
   요청 절차는 [디자인 변경 워크플로](../../docs/dev-guide/design-change-workflow.md)를 따른다.
3. 생성기를 실행한다: `cargo run -p tasty-design-tokens --bin generate`
4. `cargo test -p tasty-design-tokens`로 검증한다.
   - 토큰 census(1513 = 146/249/1118)가 바뀌면 추가·삭제 항목을 확인하고
     `tests/freshness.rs`의 스냅샷도 갱신한다.
   - `sizing_parity`나 `color_drift`가 실패하면 디자인 변경 내용을 확인한다.
     검사를 통과시키려고 값을 임의로 맞추지 않는다.
5. JSON, `dtcg/SOURCE`, 재생성된 토큰 상수와 `tasty-type-appearance` 접근자, 필요한 시험 변경을
   같은 커밋으로 저장한다.

## JSON 재생성

디자인 측이 토큰 CSS를 바꾸면 같은 tasty-page 커밋에서 JSON도 다시 만든다.
생성·대조 도구는 `tasty-page/scripts/design-tokens/`에 있고 그 레포 문서가 사용법을 설명한다.
값이나 별칭을 손으로 옮겨 적지 않는다.

## 수기 접근자

Latte만 값이 달라지는 새 semantic 역할은 생성기가 접근자를 만들지 못한다.
`tasty-type-appearance`의 `state_ink.rs`에 접근자를 쓰고 `src/dtcg/accessor.rs`의 `SEMANTIC_COLOR_TO_THEME_ACCESSOR`와
`SEMANTIC_COLOR_HAND_WRITTEN`에 추가한다. 대응이 없으면 그 역할을 가리키는 component 접근자는
스킵 사유만 로그에 남기고 생성되지 않는다.

## calc 토큰

원격 CSS가 치수를 `calc(<수> * var(--tasty-이름) + …)`로 정의하면, 생성기는 상수를 만들지 않고
Theme 접근자만 만든다. 접근자는 각 항 토큰의 접근자를 읽어 계수를 곱해 더한다. 항마다 배율과
반올림이 이미 적용된 값을 더하므로, 그 부분들로 그린 실제 폭과 같다. 문서 주석에는 배율 1의 값을 적는다.
뺄셈·백분율·다른 단위 등 이 형태를 벗어난 식은 생성하지 않고 스킵 사유로 남긴다
(`tests/calc_accessor.rs`).
