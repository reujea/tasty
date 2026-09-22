# tasty-design-tokens

디자인 시스템(claude design 산출물)의 W3C DTCG 토큰 export 를 vendor 하고,
치수 계열 토큰을 Rust const 로 생성하는 crate. 구조·역할은 `src/lib.rs` 의
crate 문서 주석 참조.

## vendor 갱신 절차

디자인 산출물은 원격 Claude Design 프로젝트가 SoT 다. 로컬로 내려받은 폴더를
찾을 필요는 없다 — **DesignSync MCP(A경로 직접 접근)로 원격 파일을 바로 회수한다**
(projectId·인증은 커밋되지 않는 로컬 전용 지침 참조).

1. **원격에서 직접 회수한다.** `DesignSync.get_file` 로 `tokens/tasty.tokens.json`
   을 읽어 `dtcg/tasty.tokens.json` 을 교체한다. (사용자에게 폴더 위치를 묻거나
   옛 로컬 경로를 재사용하지 않는다.)
   - json 은 디자인 측에서 CSS 3파일(`primitives/semantic/components.css`)로부터
     **재생성되는 산출물**이다 — 수동 편집 금지 (디자인 측 안내).
2. **CSS ↔ JSON parity 를 먼저 검증한다.** json 이 CSS 의 1:1 미러라는 것은 디자인
   측 계약(`TOKENS.md`)일 뿐 자동으로 보장되지 않는다 — 디자인 측이 CSS 만 고치고
   export 를 재생성하지 않으면 json 은 조용히 뒤처진다. 회수 직후 확인한다:

   ```sh
   # 원격 tokens/components.css 를 함께 회수해 두고
   python3 - <<'EOF'
   import json, re
   css  = open('/tmp/components.css', encoding='utf-8').read()
   names = set(re.findall(r'^\s*--tasty-([a-z0-9-]+)\s*:', css, re.M))
   comp  = {k for k in json.load(open('dtcg/tasty.tokens.json'))['component']
            if not k.startswith('$')}
   print('css', len(names), 'json', len(comp))
   print('json 에 없는 CSS 토큰:', sorted(names - comp))
   EOF
   ```

   차이가 나오면 **여기서 멈춘다.** json 을 손으로 채우지 말고(디자인 측 생성물이다)
   디자인 측에 export 재생성을 요청한다 — `design-request/` 인박스에 요청문서를
   올리는 절차는 `docs/dev-guide/design-change-workflow.md`.
3. 생성기를 재실행한다: `cargo run -p tasty-design-tokens --bin generate`
4. 테스트로 확인한다: `cargo test -p tasty-design-tokens`
   - 토큰 census(832 = 123/143/566)가 바뀌었으면 `tests/freshness.rs` 의
     스냅샷을 의식적으로 갱신한다.
   - `sizing_parity` / `color_drift` 실패는 소스 ↔ 디자인 드리프트 신호 —
     값을 임의로 맞추지 말고 디자인 판정을 먼저 확인한다.
5. vendor json + 재생성된 `src/generated/*.rs` + (필요시) 테스트 스냅샷을
   **같은 커밋**으로 커밋한다.
