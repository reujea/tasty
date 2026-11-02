# tasty-design-tokens

Claude Design의 DTCG 토큰을 저장하고 Rust 상수와 Theme 접근자를 생성한다.
배율 적용과 색 접근 규칙은 `src/lib.rs`의 크레이트 문서를 따른다.

## vendor 갱신 절차

디자인 원본은 원격 Claude Design 프로젝트에서 관리한다. projectId와 인증 방법은
커밋하지 않는 로컬 지침을 확인한다.

1. DesignSync의 `get_file`로 `tokens/tasty.tokens.json`을 받아
   `dtcg/tasty.tokens.json`을 교체한다. 이 JSON은 디자인 측 CSS에서 생성하므로
   누락된 토큰이나 값을 직접 채우지 않는다.
2. 함께 받은 CSS와 JSON을 비교한다. CSS만 수정되고 JSON이 재생성되지 않았을 수 있다.
   아래 예제는 JSON에 빠진 component 이름을 찾는다. 값·별칭의 일치나 반대 방향의
   누락까지 검사하지는 않으므로 해당 변경도 별도로 대조한다.

   ```sh
   # 원격 tokens/components.css를 /tmp/components.css에 저장한 뒤 실행한다.
   python3 - <<'EOF'
   import json, re
   css = open('/tmp/components.css', encoding='utf-8').read()
   names = set(re.findall(r'^\s*--tasty-([a-z0-9-]+)\s*:', css, re.M))
   comp = {k for k in json.load(open('dtcg/tasty.tokens.json'))['component']
           if not k.startswith('$')}
   print('css', len(names), 'json', len(comp))
   print('JSON에 없는 CSS 토큰:', sorted(names - comp))
   EOF
   ```

   차이가 있으면 갱신을 중단하고 디자인 측에 JSON 재생성을 요청한다.
   요청 절차는 [디자인 변경 워크플로](../../docs/dev-guide/design-change-workflow.md)를 따른다.
3. 생성기를 실행한다: `cargo run -p tasty-design-tokens --bin generate`
4. `cargo test -p tasty-design-tokens`로 검증한다.
   - 토큰 census(1350 = 140/175/1035)가 바뀌면 추가·삭제 항목을 확인하고
     `tests/freshness.rs`의 스냅샷도 갱신한다.
   - `sizing_parity`나 `color_drift`가 실패하면 디자인 변경 내용을 확인한다.
     검사를 통과시키려고 값을 임의로 맞추지 않는다.
5. JSON, 재생성된 토큰 상수와 `tasty-type-appearance` 접근자, 필요한 시험 변경을
   같은 커밋으로 저장한다.

## calc 토큰

원격 CSS가 치수를 `calc(<수> * var(--tasty-이름) + …)`로 정의하면, 생성기는 상수를 만들지 않고
Theme 접근자만 만든다. 접근자는 각 항 토큰의 접근자를 읽어 계수를 곱해 더한다. 항마다 배율과
반올림이 이미 적용된 값을 더하므로, 그 부분들로 그린 실제 폭과 같다. 문서 주석에는 배율 1의 값을 적는다.
뺄셈·백분율·다른 단위 등 이 형태를 벗어난 식은 생성하지 않고 스킵 사유로 남긴다
(`tests/calc_accessor.rs`).
