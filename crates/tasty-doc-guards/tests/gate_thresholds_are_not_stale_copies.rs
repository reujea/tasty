//! 게이트 임계값을 주장하는 자리가 정본과 같은지 본다.
//!
//! 좌변은 **값이 리터럴로 사는 게이트 전부**다([`GATES`]) — 어느 게이트인지는 그 상수가
//! 정한다. **여기서 다시 세지 않는다**: 한때 이 자리가 넷을 이름으로 열거했고, 다섯째
//! (동결 총합 예산)와 여섯째(계측 편향)가 [`GATES`] 에 들어온 뒤에도 이 문장은 넷만
//! 말하고 있었다 — 이 가드가 잡으려는 낡음이 이 가드의 머리에서 났다.
//! 전부 기계가 읽는 값인데, 그것을 평문으로 다시 적은 자리가 여럿이고 어느 것도
//! 파생되지 않는다. 한쪽만 고치면 게이트는 새 값으로 돌고 문서는 옛 값을 말한다 —
//! **둘 다 초록이다.** 값을 여기 안 적는 것은 이 문장 자신이 그 사본이 되기 때문이다.
//!
//! ## 래칫 둘은 나중에 들어왔다 — 그 사이에 실제로 낡았다 (2026-09-09)
//!
//! `check-shared-walk-ratchet.sh` 의 상한을 한 칸 내린 회차가 **그 수를 인용하던 산문
//! 셋을 안 고쳤다.** 셋 다 현재형 주장이라 그 자리에서 거짓이 됐는데 **게이트는 하나도
//! 안 깨졌다** — 이 가드가 존재하는 이유가 정확히 그 형태다. 그때 채널이 있던 사본은
//! `tests/shared_walk_gate.rs` 의 `CAP=` 치환 하나뿐이었고 그것만 함께 빨개졌다.
//! 형제 `check-allow-reason.sh` 의 `CAP` 도 같은 노출이라 함께 넣었다.
//!
//! 산문 셋 중 둘은 값을 지우는 쪽으로 고쳤다(문장의 요지가 값 없이 성립한다 — 아래
//! `agent.rs` 와 같은 처방). 그래서 이 가드가 세는 좌변은 **남기기로 한 사본**뿐이다:
//! 값을 안 드는 문장은 낡을 수가 없고, 그래서 볼 것도 없다.
//!
//! ## 좌변이 집합이 아니라 스칼라다 — 그래서 "양방향" 의 형태가 다르다
//!
//! `workspace_lint_table_matches_the_manifest` 의 좌변은 (lint, 레벨) **집합**이라
//! 양방향이 곧 집합 상등이었고, 거기서 값진 것은 **부분 사본**(빠진 행)을 잡는 방향이었다.
//! 여기는 우변이 스칼라 하나라 "빠진 행" 이라는 것이 없다. 대신 실패 형태가 둘이다.
//!
//! - **낡은 사본** — 주장 자리가 옛 값을 든다. 소스 값으로 각 자리를 물어 잡는다.
//! - **미분류 신규 자리** — 새 문서가 그 값을 주장하기 시작하는데 아무도 모른다.
//!   이쪽이 조용하다. 그래서 **명부 완전성** 축을 둔다: 레포 전체를 과포함으로 훑어
//!   그 값을 게이트 어휘와 함께 든 파일을 모으고, **전부 명부 아니면 제외목록 안**이어야
//!   한다. 분류되지 않은 파일이 하나라도 있으면 실패다.
//!
//! ## 좌변 선별 — "현재 상태를 주장하는 수" 만 든다
//!
//! `complexity_allowlist_docs_parity` 가 이미 그은 선을 그대로 쓴다: **현재 상태를
//! 주장하는 수**와 **시점 측정·결정 근거·예시·인용**은 다르다. 뒤엣것은 지금 값과
//! 갈라지는 것이 정상이라 대조 대상이 아니다.
//!
//! 실측 선별. 낱말 경계 토큰 전수 → 게이트 어휘 ±2 줄 창 통과 → 손 분류.
//! **파일 단위로 통일해 적는다** — 명부·제외가 파일이므로 어휘창도 파일이어야 같은
//! 줄에 놓을 수 있다.
//!
//! ```text
//!   (2026-09-08 · rev eee485d5c · 이 트리에서 잰 값)
//!               전수(줄)   어휘창(파일)   명부   거짓양성   정밀도
//!    20            488          17          5       12      29.4%
//!    1000          352          26          4       22      15.4%
//! ```
//!
//! 래칫 둘은 뒤에 들어와 따로 쟀다(작업 트리 · 기점 `96235f0a0` + 이 lane 의 산문 수정).
//! 같은 술어를 같은 창(±2 줄)으로 돌린 값이고, 재는 사본은 이 파일의 [`claims_value`]
//! 를 셸로 옮긴 것이라 낱말 경계는 ASCII 다(아래 ★ 의 그 오차를 안 낸다):
//!
//! ```text
//!               전수(줄)   어휘창(파일)   명부   거짓양성   정밀도
//!    CAP(순회)      53           6          2        4       33.3%
//!    CAP(allow)     10           3          2        1       66.7%
//! ```
//!
//! 거짓양성 다섯은 전부 **같은 토큰의 다른 뜻**이다(단축키 항목 수 · 알파 비율 55% ·
//! connecting 최악 ~55초 둘 · 수의 계보 ADR 이 든 상한 수열). 정밀도가 위 둘보다 높은
//! 것은 어휘를 좁혀서가 아니라 좌변이 작아서다 — 어휘는 같은 폭으로 넓게 뒀다.
//!
//! ★ 앞선 판(이 가드를 지은 회차)의 표는 **갱신이 아니라 교체다.** 그 표의 `제외` 칸은
//! 세어진 값이 아니었다 — `어휘창`(줄) 에서 `명부`(파일) 를 뺀 값이라 한 줄 안에 단위가
//! 둘이었고, 두 행이 정확히 합에 맞은 것은 그 뺄셈의 결과지 검산의 결과가 아니다.
//! 그 트리(`942f01027`)에서 다시 재면 어휘창은 줄 21 · **파일 13** 이고, 파일 단위로
//! 갈랐을 때 거짓양성은 15 가 아니라 **8** 이다. 시점 측정을 시점 측정이라는 이유로
//! 보존하는 것(ADR-0139)과, **세어진 적 없는 칸을 보존하는 것**은 다르다.
//!
//! ★★ 위 수는 **이 트리에서만 잰 값이다.** 다른 lane 이 같은 회차에 들여오는 문서는
//! 여기서 원리적으로 안 보인다 — 실제로 통합 자리에서 거짓양성이 셋 나타났고(웹훅
//! 남용차단 ADR 둘 + `CHANGELOG.md`), 그것이 이 가드가 잡으라고 만들어진 형태 그대로다.
//! 두 lane 이 서로 다른 base 에서 자랐고 합친 자리에서만 보였다. 즉 **이 가드의 첫
//! 실측 자리는 lane 이 아니라 통합 회차다.**
//!
//! ## 어휘창을 좁히지 않는다 — 잰 값으로
//!
//! 정밀도가 낮다(29.4% · 15.4%). 좁히면 얼마나 오르는지도 쟀다(놓침은 지금 명부 기준):
//!
//! ```text
//!                                  통과(20/1000)   명부 놓침   정밀도(20/1000)
//!   창 ±2 · 낱말 6/10 (현행)            17 / 26          0       29.4% / 15.4%
//!   창 ±0                               15 / 21          0       33.3% / 19.0%
//!   최소 어휘(부분집합 전수 탐색)          9 /  6          0       55.6% / 66.7%
//!     20 → ["복잡도","complexity"] · 1000 → ["allowlist"]
//! ```
//!
//! **놓치는 것이 0 인데도 안 좁힌다.** 그 0 의 모수가 **지금 명부(다섯 · 넷)** 이기
//! 때문이다 — 이 가드가 막으려는 것은 아직 안 쓰인 문서이고 그것은 어느 모수에도 안
//! 들어 있다. 여유를 재면 얇다: 명부 아홉 자리의 어휘 다중도 중
//! `crates/tasty-cli/src/request/agent.rs` 는 **1**(`복잡도` 하나에만 걸린다) 이고,
//! `.github/workflows/complexity-check.yml` 은 창을 ±0 으로 좁히는 순간 5 에서 **1** 로
//! 떨어진다. 최소 어휘 `["allowlist"]` 는 명부 넷이 **우연히** 그 낱말을 든 결과라
//! 과적합이다 — "파일 SLOC 상한은 1000 이다" 라고만 쓴 새 문서를 통째로 놓친다.
//!
//! 결론을 정하는 것은 비용의 비대칭이다. 거짓양성의 값은 `EXCLUDED` 한 줄 + 사유이고
//! **통합 자리에서 반드시 발화한다.** 거짓음성의 값은 새 주장 자리가 영영 안 보이는
//! 것이고 **아무 데서도 발화하지 않는다.**
//!
//! ## 얇은 두 자리 — 판정을 닫는다 (2026-09-08)
//!
//! 어휘 다중도를 재면 둘이 얇았다. 회차 하나를 미뤄 뒀던 자리이고, 여기서 닫는다.
//!
//! - `.github/workflows/complexity-check.yml` — ±2 에서 **5**, ±0 에서 1. 얇음이
//!   **기각된 좁히기에만 조건적**이다. 창을 안 좁히므로 할 일이 없다. **닫힘.**
//! - `crates/tasty-cli/src/request/agent.rs` — ±2 에서도 **1**(`복잡도` 하나).
//!   **얇음은 증상이었고 원인은 다른 것이었다** — 아래.
//!
//! ## 자리의 **성격**이 처방을 정한다 — 아홉 자리 전수 분류
//!
//! 이 가드는 "값이 낡음" 과 "이 자리가 주장을 멈춤" 을 가른다(축 ①). 그 분해를 명부
//! 아홉 자리에 **전부** 적용해 보면 갈래가 둘이 아니라 셋이다:
//!
//! ```text
//!   ① 정본        값이 여기 산다                 clippy.toml:10 · check-file-size.sh:27
//!   ② 현재 상태    "지금 임계는 20 이다"          값을 고치면 맞는다
//!   ③ 시점 서술    "그때 20 이어서 이렇게 했다"    값을 고치면 **거짓이 된다**
//! ```
//!
//! ③ 은 결정·측정의 산물로 값을 든 자리다 — `clippy.toml:7` 의 rca 등가,
//! `complexity-gate.md:20` 의 발화율 곡선, 같은 문서 `:21` 의 두 달 실측. 임계가
//! 바뀌면 그 문장들은 **값만 고쳐서는 안 되고 측정을 다시 해야** 한다(ADR-0139).
//!
//! **전수로 세면: 값을 오직 ③ 문장에서만 드는 파일은 `agent.rs` 하나다.** 나머지
//! 여덟은 ② 문장을(또는 ① 을) 최소 하나 갖고 있어 명부에 남는 것이 옳다. 세는 방법은
//! 파일마다 값 토큰이 나오는 **모든 줄**을 갈래로 매기는 것이고, 파일 단위 명부로는
//! 이 물음에 답할 수 없다 — 한 파일이 두 갈래를 함께 담기 때문이다.
//!
//! ## 그래서 `agent.rs` 를 명부에서 뺐다 — 자리를 고쳐서
//!
//! 그 줄은 "`agent_command_to_method_params` 의 인지 복잡도 상한(20)을 넘기지 않도록
//! 이 함수를 밖으로 뺐다" 였다. 그것은 **코드가 왜 이렇게 생겼는지**를 말하지 지금
//! 임계가 얼마인지를 말하지 않는다. 임계가 15 로 바뀌면 ② 로 읽는 쪽은 값을 15 로
//! 고치라고 하는데, 그러면 **그때 20 때문에 뺐다는 사실이 지워진다.**
//!
//! 그래서 값을 지웠다(`상한(20)을` → `상한을`). 문장의 요지는 값 없이 성립하고,
//! 넘는지 여부는 clippy 가 `deny` 로 잡는다 — 이 주석이 질 일이 아니다. 값을 안 드니
//! 어휘창 스캔에도 안 걸리고, 그래서 `EXCLUDED` 에도 안 들어간다(명부 5 → 4).
//!
//! ★ 이것은 **계측기의 여유를 위해 대상을 고친 것이 아니다.** 앞 회차가 그 이유로
//! 이 자리를 안 고치기로 했던 것은 옳았다. 고친 이유는 다르다 — 그 문장이 ③ 인데
//! ② 로 분류돼 있었고, 그 오분류가 **틀린 처방을 발행할 수 있는 상태**였다. 다중도 1
//! 은 그 오분류의 증상이었다: 현재 상태를 주장하지 않는 문장이라 게이트 어휘를 얇게
//! 든 것이 자연스럽다.
//!
//! **대신 그 자리가 실제로 얇아졌을 때 이 가드가 무엇을 시키는지를 쟀고, 거기서 결함이
//! 나왔다.** 그 줄의 `복잡도` 를 `함수 크기` 로 바꾸자 실패문이 "지금 값 `20` 을 안
//! 든다 — 그 자리를 고쳐라" 였는데 **그 파일은 `20` 을 들고 있었다.** 따르면 아무것도
//! 안 고쳐지거나 값을 중복해 쓴다. 상태가 둘인데 한 칸에 섞여 있었다:
//!
//! ```text
//!   값 토큰이 파일에 없다      → 낡은 사본이다. "그 자리를 고쳐라" 가 맞다
//!   값은 있는데 어휘가 떨어졌다 → 이 자리가 그 게이트를 말하기를 그만뒀다.
//!                                명부에서 빼거나 문장을 다시 쓰는 것이 맞다
//! ```
//!
//! 한 물음("값 토큰이 파일 어딘가에 있나")이 둘을 가른다. 위 ① 축이 이제 갈라 보고한다.
//!
//! ## 이 설계의 비용 — `EXCLUDED` 는 자란다 (결함이 아니다)
//!
//! 위 선택의 대가를 여기 적어 둔다. 임계 어휘와 함께 `20`·`1000` 을 쓰는 문서가 들어올
//! 때마다 이 목록이 한 줄 는다(통합 한 번에 셋 늘었다). **그것은 recall 여유의 가격이지
//! 결함이 아니다** — 이 문단이 없으면 다음 사람이 목록의 길이를 결함으로 읽고, 그 처방은
//! 어휘를 좁히는 것이 되어 위에서 기각한 자리로 돌아간다. 자라는 것이 문제가 되는
//! 지점은 크기가 아니라 **사유가 비는 것**이고, 그것은
//! `every_excluded_entry_carries_a_reason` 이 막는다.
//!
//! ### 자라기만 하는 것이 아니다 — 정본이 움직이면 **구성이 갈린다** (2026-09-09 · 쟀다)
//!
//! 위 문단은 목록이 **는다**고만 적는다. 그것은 사실상 안 움직이는 정본에 대해 참이고,
//! 나중에 들어온 래칫 둘은 그렇지 않다 — 상한이 움직인 이력이 그 스크립트들에 값으로
//! 남아 있다. 정본이 한 칸 움직이면 이 목록은 **길이가 아니라 구성이** 바뀐다: 그 값을
//! 근거로 든 제외들이 옛 값을 들게 되어 [`no_exclusion_is_dead_weight`] 로 몰리고,
//! 동시에 **내린 값을 이미 들고 있던** 문서가 미분류로 들어온다.
//!
//! 실측(이 트리에서 `sed` 로 변이 → `cp` 원복): 공용 순회 래칫의 정본을 한 칸 내리면
//! 죽은 면제 4 건과 새 미분류 1 건이 한 번에 뜬다. 넷은 그 값을 근거로 든 제외들이고,
//! 하나는 내린 값을 본문에 이미 들고 있던 ADR 이다.
//!
//! **이 비용은 결함이 아니라 설계가 사려는 행동 그 자체다** — 임계가 움직일 때 그 값을
//! 든 자리를 전부 다시 보게 만드는 것. 적어 두는 이유는 처방이 갈리기 때문이다. 위
//! 문단만 읽고 오면 목록이 한 줄 느는 회차를 기대하는데, 래칫이 움직인 회차는 항목이
//! 죽고 새로 들어온다 — 그 빨강을 **회귀**로 읽으면 처방이 "상한을 되돌려라" 가 되고,
//! 그것은 이 레포의 모든 래칫이 금지하는 바로 그 동작이다.
//!
//! ## 줄 수 판정은 `EXCLUDED` 로 안 옮긴다 (2026-09-08 · 세고 정했다)
//!
//! 명부에 [`Claim`] 의 줄 수를 넣고 나서, 같은 것을 제외 쪽에도 넣을지 물었다. 옮기기
//! 전에 셌고, 세어 보니 옮기면 안 되는 것이었다.
//!
//! **그 수를 낳는 술어를 함께 적는다.** 제외 항목의 각 (파일, 값) 짝에 대해
//! [`has_bare_token`] 으로 그 값을 드는 **줄 수**를 세고, 그 수가 **2 이상**인 짝을 센다.
//! 두 조건이 이 술어를 다른 것과 가른다:
//! - **어휘창(`±2 줄`)은 안 본다.** 묻는 것이 "그 게이트에 대한 주장인가" 가 아니라
//!   "줄 수 판정을 걸면 흔들릴 자리인가" 라서다. 어휘창을 켜면 같은 물음에 9 가 나온다.
//! - **낱말 경계는 ASCII 다**([`has_bare_token`] 이 `is_ascii_alphanumeric` 을 쓴다).
//!   그래서 `20여개` 는 `20` 을 드는 줄로 **센다**. 유니코드 낱말 경계로 세면 8 짝이
//!   사라진다 — 아래 ★ 가 그 오차로 한 번 틀린 기록이다.
//!
//! 그 술어로 **19 짝**이고 셋으로 갈린다(갈래는 이제 손 분류가 아니라 [`Kind`] 값이다):
//!
//! ```text
//!   부류(Kind)                 짝   줄 수 판정을 걸면
//!   ── SelfRef  이 가드 자신     2   문구를 한 줄 고칠 때마다 수가 바뀐다(자기참조)
//!   ── OtherMeaning 무관한 수    8   코드 편집마다 바뀐다. 애초에 셀 값이 아니다
//!   ── Dated    시점 기록        9   안 바뀐다. 조용하지만 **얻는 것도 없다**
//! ```
//!
//! **짝 수를 적고 줄 합계는 안 적는다.** 짝 수 19 는 세 트리에서 전부 같았지만
//! (`78bd726c3` · `b134d28e3` · 지금) 줄 합계는 158 · 166 · 168 로 움직였다 — 그 합이
//! 이 파일 자신의 줄을 세기 때문이고, 이 문단을 고쳐 쓰는 것만으로 또 움직인다.
//! 같은 술어라도 **흔들리는 값과 안 흔들리는 값이 갈린다.**
//!
//! ★ **이 자리는 한때 그 19 를 "재현되지 않는다" 며 18 로 철회했고, 그 철회가 틀렸다**
//! (`c448dcc1e` → 되돌림). 원인은 계측기다: 확인에 쓴 python 사본의 `str.isalnum()` 이
//! 유니코드라 `20여개` 의 `여` 를 낱말 안으로 세어 그 줄을 빼 버렸다. 판정기는 ASCII 라
//! 그 줄을 센다. **판정기가 옳았고 사본이 틀렸다** — 그리고 그 차이는 판정기를 실제로
//! 돌리기 전까지 안 보였다(사본만 봤으면 멀쩡한 제외 항목 둘을 지웠을 것이다).
//! 술어를 적는 것으로는 부족하고 **그 술어를 무엇으로 재는지**까지 적어야 한다는 것이
//! 이 기록의 값이다.
//!
//! 앞의 둘(10 짝)은 **순수 소음**이다. `src/view/main/redraw.rs` 의 `20` 아홉 줄은
//! 재시도 상한이고 `crates/tasty-memory/src/tests.rs` 의 아홉 줄은 픽스처 행 수다 —
//! 사유 칸이 이미 그렇게 적고 있다.
//!
//! 셋째(9 짝)는 조용하지만 그래서 더 분명하다 — **ADR 은 정본이 바뀌어도 안 고치는
//! 자리다.** 낡은 사본이 남는 것이 그 자리에서는 정상이고, 그러니 셀 이유가 없다.
//! 갈라지는 지점은 성격이다: **명부는 "값을 고쳐라" 를 시키는 자리, 제외는 안 시키는
//! 자리다.** 줄 수 판정은 그 "고쳐라" 의 정밀도를 올리는 장치라, 안 시키는 자리에는
//! 걸 것이 없다. 그래서 안 옮긴다.
//!
//! 제외의 갈래는 **넷**이고, 갈래는 주석 절이 아니라 각 항목이 드는 **값**이다
//! ([`Kind`]) — 결정 기록·인용 · 예시로 든 수 · 같은 토큰의 다른 뜻 · 이 가드 자신.
//! 갈래가 값이라야 갈래마다 다른 판정을 붙일 수 있고, 항목을 더하는 사람이 하나를
//! 고르게 된다. 사유는 그 위에 따로 적는다.
//!
//! ## 명부는 줄 번호가 아니라 파일이다
//!
//! 좌표를 줄로 잡으면 문단이 하나 늘 때마다 명부가 낡는다(`line_number_citations_do_not_grow`
//! 가 같은 이유로 줄 인용을 래칫한다). 파일 단위로 잡고, 그 파일이 **현재 값을 게이트
//! 어휘와 함께 들고 있는가**로 판정한다.
//!
//! ## 못 잡는 것 (사전 등록)
//!
//! - ~~한 파일이 그 값을 두 번 주장하는데 한 자리만 낡은 경우~~ — **닫혔다(2026-09-08).**
//!   이 자리는 오래 "의도적 교환" 으로 등록돼 있었고, 근거는 "줄 단위로 내리면 명부가
//!   줄이 되고 줄 번호는 낡는다" 였다. 그 교환은 **'줄 단위 = 줄 번호' 라는 전제** 위에
//!   있었다. 명부가 드는 것을 좌표가 아니라 **줄 수**로 바꾸면 전제 밖으로 나간다 —
//!   줄이 밀려도 안 낡고, 파일 안의 사본을 전부 센다([`Claim`]).
//!   구멍은 실측으로 증명하고 닫았다(변이 E: 한 줄만 낡게 하면 옛 판정은 rc 0 이었다).
//! - 게이트 어휘 없이 그 값만 적은 주장. 그런 자리는 읽는 사람도 무엇의 임계인지 모른다.
//! - 값을 **범위**로 적은 서술("1000 안팎"). 토큰 일치만 본다.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// 정본 — (파일, 값이 뒤에 붙는 접두).
const COGNITIVE_SOURCE: (&str, &str) = ("clippy.toml", "cognitive-complexity-threshold = ");
const SLOC_SOURCE: (&str, &str) = ("scripts/check-file-size.sh", "THRESHOLD=");
const SHARED_WALK_SOURCE: (&str, &str) = ("scripts/check-shared-walk-ratchet.sh", "CAP=");
const ALLOW_REASON_SOURCE: (&str, &str) = ("scripts/check-allow-reason.sh", "CAP=");
const FROZEN_SUM_SOURCE: (&str, &str) = (".complexity-file-allowlist", "# frozen-sum-budget: ");
const DOC_BIAS_SOURCE: (&str, &str) = (".complexity-file-allowlist", "# doc-comment-bias: ");

/// 게이트 어휘 — 이 중 하나가 ±2 줄 창에 있어야 그 자리를 "이 게이트에 대한 언급" 으로 센다.
const COGNITIVE_VOCAB: &[&str] = &[
    "cognitive",
    "복잡도",
    "threshold",
    "임계",
    "clippy.toml",
    "complexity",
];
const SLOC_VOCAB: &[&str] = &[
    "SLOC",
    "sloc",
    "tokei",
    "check-file-size",
    "상한",
    "임계",
    "복잡도",
    "complexity",
    "allowlist",
    "THRESHOLD",
];
const SHARED_WALK_VOCAB: &[&str] = &[
    "read_dir",
    "순회",
    "래칫",
    "상한",
    "임계",
    "shared-walk",
    "shared_walk",
    "CAP",
];
const ALLOW_REASON_VOCAB: &[&str] = &[
    "allow",
    "억제",
    "사유",
    "래칫",
    "상한",
    "임계",
    "allow-reason",
    "allow_reason",
    "CAP",
];
const FROZEN_SUM_VOCAB: &[&str] = &[
    "frozen-sum-budget",
    "frozen_sum",
    "예산",
    "동결",
    "총합",
    "래칫",
    "budget",
];
/// 계측 편향의 어휘.
///
/// ★ **`tokei` 를 일부러 안 넣었다** — 다른 게이트에서는 그 낱말이 recall 을 벌지만
/// 여기서는 값이 `25` 라 흔하고, 그 낱말 하나로 `complexity-gate.md` 의 "25% 큰 파일"
/// 이 창을 통과한다(실측 2026-09-09). 그 파일은 다른 두 게이트의 **명부**에 있어
/// `EXCLUDED` 로 뺄 수가 없고(`no_file_is_both_claimed_and_excluded`), 명부에 넣으면
/// 편향이 움직인 날 "지금 값이 아예 없다 — 그 자리를 고쳐라" 가 **백분율 문장**을
/// 가리킨다. 값이 흔할수록 어휘는 그 개념에 붙은 낱말만 든다.
const DOC_BIAS_VOCAB: &[&str] = &["doc-comment-bias", "doc 주석", "누락", "편향", "bias"];

/// cognitive 임계를 **현재 상태로 주장하는** 파일과, 그 파일이 값을 든 **줄 수**.
///
/// 둘째 칸이 이 명부의 요점이다 — 자세한 것은 [`Claim`].
const COGNITIVE_CLAIMS: &[Claim] = &[
    ("Cargo.toml", 1),
    // 7 = rca 등가(시점 서술) · 10 = 정본
    ("clippy.toml", 2),
    ("docs/dev-guide/clippy-policy.md", 1),
    // 11 = 표(현재 상태) · 17 = rca 등가(시점) · 118 = 조이기 안내(현재 상태)
    ("docs/dev-guide/complexity-gate.md", 3),
];

/// 파일 SLOC 임계를 **현재 상태로 주장하는** 파일과, 그 파일이 값을 든 **줄 수**.
const SLOC_CLAIMS: &[Claim] = &[
    (".github/workflows/complexity-check.yml", 1),
    // 두 lane 이 이 게이트의 시험을 넓히며 임계를 주석으로 인용했다.
    // 인용이 아니라 **주장**으로 분류한다 — 그 시험은 500 줄 프로브가 임계 아래라는
    // 전제로 초록을 단정하므로, 임계가 500 아래로 내려가면 주석만이 아니라 시험이
    // 틀린다. 그러니 정본이 움직일 때 이 자리가 함께 빨개지는 것이 옳다.
    ("tests/file_sloc_gate_fails_loudly.rs", 1),
    ("docs/dev-guide/clippy-policy.md", 1),
    // 12 = 표 · 20 = 발화율 곡선(시점 측정) · 24 = 여유 서술 · 118 = 조이기 안내
    ("docs/dev-guide/complexity-gate.md", 4),
    // 3 = 머리 주석 · 14 = ADR 가리킴 · 27 = 정본
    ("scripts/check-file-size.sh", 3),
];

/// 공용 순회 래칫의 상한을 **현재 상태로 주장하는** 파일과, 그 파일이 값을 든 **줄 수**.
const SHARED_WALK_CLAIMS: &[Claim] = &[
    // 이력의 마지막 줄(시점 서술) + 정본
    ("scripts/check-shared-walk-ratchet.sh", 2),
    // 상한을 바꿔치기하려고 정본 줄의 철자를 든다 — 값이 움직이면 그 치환이 죽는다
    ("tests/shared_walk_gate.rs", 2),
];

/// 사유 없는 `#[allow]` 래칫의 상한을 **현재 상태로 주장하는** 파일과 그 **줄 수**.
const ALLOW_REASON_CLAIMS: &[Claim] = &[
    // 231 = 정본 하나뿐이다. 48·198·199·226·228 도 이 수를 들지만 전부 **날짜와 트리를
    // 박은 계보/시점 서술**이라(`실측 2026-09-08 … base 12bc0f4b2`, `184 → 183 → 182`)
    // 상한이 182 → 181 로 내려가도 그 줄들은 그대로 참이다 — 지금 값으로 고치면 없던
    // 거짓이 새로 생긴다(이 가드의 `TIME_NOTE` 가 시키는 처방). 그래서 좌변이 6 → 1 이다.
    ("scripts/check-allow-reason.sh", 1),
    ("tests/allow_reason_gate.rs", 2),
];

/// 동결 총합 래칫의 **예산**을 현재 상태로 주장하는 파일과 그 **줄 수**.
///
/// 지금 하나뿐이다 — 정본 그 자신. 그 사실이 이 항목을 등록하는 이유다: 사본이 0 인
/// 지금이야말로 명부를 세울 수 있는 때이고, 사본이 생긴 뒤에는 그것이 정본인지 사본인지를
/// 다시 판정해야 한다. 예산은 되돌아 올라가지 않으므로 낡은 사본이 가장 비싼 값이다.
const FROZEN_SUM_CLAIMS: &[Claim] = &[(".complexity-file-allowlist", 1)];

/// 계측 편향의 **고정값**을 현재 상태로 주장하는 파일과 그 **줄 수**.
///
/// 정본 그 자신뿐이다. 이 값은 게이트가 매번 재어 여유 0 으로 고정하므로 산문이 그것을
/// 복제하면 편향이 움직인 날 그 산문이 조용히 거짓이 된다 — 그래서 산문 쪽은 값을 지우고
/// 정본을 가리키게 고쳤다(`complexity-gate.md`). 남은 인용은 전부 시점 측정이라
/// `EXCLUDED` 에 있다.
const DOC_BIAS_CLAIMS: &[Claim] = &[(".complexity-file-allowlist", 1)];

/// 명부 한 항목 — (레포 상대 경로, 그 파일이 임계값을 든 **줄 수**).
///
/// ## 왜 줄 수인가 (2026-09-08)
///
/// 이 판정은 오래 **파일 단위**였다. "그 파일이 현재 값을 게이트 어휘와 함께 드는가"
/// 하나만 물었고, 그래서 **한 파일이 값을 두 번 드는데 한 자리만 낡은 경우**를 못
/// 잡았다. 그 구멍은 모듈 주석에 "못 잡는 것" 으로 **사전 등록돼 있었고**, 교환의
/// 근거는 "줄 단위로 내리면 명부가 줄이 되고 줄 번호는 낡는다" 였다.
///
/// **그 교환은 '줄 단위 = 줄 번호' 라는 전제 위에 있었다.** 줄 수는 그 전제 밖이다 —
/// 좌표를 안 들어서 줄이 밀려도 안 낡고, 그러면서 파일 안의 사본을 전부 센다.
///
/// 구멍은 실측으로 증명했다(변이 E): 정본을 안 건드리고
/// `docs/dev-guide/complexity-gate.md:24` 한 줄만 `1000` → `1500` 으로 바꿨더니
/// 나머지 세 줄이 `1000` 을 들어 가드가 **rc 0** 으로 통과했다. 그 rc 0 이 결과였다.
///
/// ## 비용
///
/// 그 파일에 임계 사본이 늘거나 줄면 이 수를 갱신해야 한다. 그것이 이 설계의 값이다 —
/// 갱신하려면 그 파일의 임계 사본을 **전부 다시 봐야** 하고, 이 가드가 사려는 것이
/// 바로 그 행동이다(`EXCLUDED` 가 자라는 것을 비용으로 받아들인 것과 같은 성격).
type Claim = (&'static str, usize);

/// 제외의 갈래. **주석 절이 아니라 값이다.**
///
/// 한때 이 갈래는 `EXCLUDED` 안의 `// ── ①` 주석 절로만 있었다. 그러면 갈래를 읽으려면
/// 사람이 목록을 눈으로 갈라야 하고, **갈래마다 다른 판정을 붙일 수가 없다** — 판정이
/// 읽을 수 있는 것은 값뿐이다. 항목을 더하는 사람이 갈래를 고르게 만드는 효과도 있다:
/// 주석 절은 새 항목을 아무 절 끝에나 붙여도 조용하지만, 값은 하나를 고르게 한다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// 결정 기록·인용 — ADR 본문·CHANGELOG 는 **결정 시점의 값**을 남긴다. 정본이
    /// 바뀌어도 안 고치는 자리라 낡은 사본이 남는 것이 정상이다.
    Dated,
    /// 예시로 든 수 — 그 문서의 주제가 이 임계가 아니다.
    Example,
    /// 같은 토큰의 다른 뜻 — webhook `threshold: 20` · `wal_autocheckpoint 1000` 등.
    OtherMeaning,
    /// 이 가드 자신 — 명부와 사유를 본문에 들고 있어 스캔에 걸린다.
    SelfRef,
}

/// 어휘창은 통과하지만 **현재 상태 주장이 아닌** 자리 — 사유를 함께 적는다.
///
/// 사유가 없으면 다음 사람이 "왜 여기만 빠졌나" 를 다시 판정해야 하고, 그 재판정은
/// 매번 같은 답을 안 낸다.
const EXCLUDED: &[(&str, Kind, &str)] = &[
    (
        "docs/adr/0037-complexity-gate.md",
        Kind::Dated,
        "두 임계를 결정한 ADR. 결정문의 값이라 현재 값과 갈리면 새 ADR 을 쓴다",
    ),
    (
        "docs/adr/0131-file-sloc-gate-needs-a-firing-trigger.md",
        Kind::Dated,
        "도입 시점 측정(초과 0 건)과 대안 D 의 인용",
    ),
    (
        "docs/adr/0165-the-file-sloc-gate-measures-shipped-lines.md",
        Kind::Dated,
        "대안 A 기각 사유에서 임계를 인용",
    ),
    (
        "docs/adr/0168-the-file-sloc-threshold-is-not-derived-and-the-freeze-ratchets-one-way.md",
        Kind::Dated,
        "임계의 유도 부재를 다루는 ADR — 분포표·발화율 곡선의 축 값이라 시점 측정이다",
    ),
    ("docs/adr/index.md", Kind::Dated, "ADR 제목의 인용"),
    (
        "docs/adr/0124-blank-value-rule-is-load-path-independent.md",
        Kind::Dated,
        "과거 사건 서술 — 그때 게이트를 넘었다",
    ),
    (
        "crates/tasty-design-tokens/src/dtcg/duration_accessor.rs",
        Kind::Dated,
        "과거 사건 서술 — 파일을 가른 이유",
    ),
    (
        "src/source_guards/sloc_gate_skip_proxy.rs",
        Kind::Dated,
        "ADR-0168 의 결론을 비유로 인용",
    ),
    (
        "scripts/check-frozen-sum-ratchet.sh",
        Kind::Dated,
        "옛 어긋남 사건의 기록 · 계측 편향의 시점 재측정",
    ),
    (
        "docs/adr/0183-a-green-check-is-not-evidence-without-a-control.md",
        Kind::OtherMeaning,
        "실패 계수 서술의 수 — 진짜 1 / 오염 30 / 보이는 31 은 연쇄 실패를 세는 값이고 계측 편향과 뜻이 다르다",
    ),
    (
        "CHANGELOG.md",
        Kind::Dated,
        "릴리스 시점의 서술 — 파일 전체가 \"그때 무엇이 바뀌었나\" 라서 현재 상태 주장이 아니다. 그래서 파일 단위로 뺀다",
    ),
    (
        "docs/adr/0139-numbers-in-docs-are-classified-by-lineage-not-by-name.md",
        Kind::Example,
        "수의 계보 분류 ADR 이 임계 몇을 예시로 든다 — `check-allow-reason` 상한의 계보 수열 포함",
    ),
    (
        "docs/adr/0119-agent-semaphore-resize-and-holder-expiry.md",
        Kind::OtherMeaning,
        "20분 — 시간",
    ),
    (
        "docs/features/webhook/index.md",
        Kind::OtherMeaning,
        "webhook 남용 차단 threshold 기본 20",
    ),
    (
        "src/webhook/abuse.rs",
        Kind::OtherMeaning,
        "webhook 남용 차단 threshold 기본 20",
    ),
    (
        "docs/adr/0195-abuse-counting-includes-rejected-tokens.md",
        Kind::OtherMeaning,
        "webhook 남용 차단 threshold 기본 20 — 같은 토큰의 다른 뜻",
    ),
    (
        "docs/adr/0196-abuse-thresholds-and-source-key.md",
        Kind::OtherMeaning,
        "webhook 남용 차단 threshold 기본 20, 그리고 무차별 대입 계산에 든 초당 1000 회",
    ),
    (
        "src/app/event_handler.rs",
        Kind::OtherMeaning,
        "20여개 variant — 개수",
    ),
    (
        "src/adapters/ui/popup/command_palette.rs",
        Kind::OtherMeaning,
        "목록이 상한에 걸린 뒤 높이가 안 움직이는지 보는 시험의 항목 수 1000 — 줄 수가 아니다",
    ),
    (
        "docs/adr/0091-render-stall-watchdog-observation-only.md",
        Kind::OtherMeaning,
        "wgpu FRAME_TIMEOUT_MS = 1000",
    ),
    (
        "docs/adr/0108-egui-mesh-scroll-delivered-in-one-pass.md",
        Kind::OtherMeaning,
        "egui scroll points_per_second 1000",
    ),
    (
        "docs/design/systems/memory.md",
        Kind::OtherMeaning,
        "SQLite wal_autocheckpoint 1000 페이지",
    ),
    (
        "docs/design/systems/storage.md",
        Kind::OtherMeaning,
        "SQLite wal_autocheckpoint 1000 페이지",
    ),
    (
        "crates/tasty-memory/src/lib.rs",
        Kind::OtherMeaning,
        "SQLite wal_autocheckpoint 1000 페이지",
    ),
    (
        "crates/tasty-memory/src/tests.rs",
        Kind::OtherMeaning,
        "테스트 픽스처의 행 수",
    ),
    (
        "crates/tasty-plugin-agent-stream/src/handlers.rs",
        Kind::OtherMeaning,
        "POLL_MAX_LIMIT = 1000",
    ),
    (
        "crates/tasty-plugin-manifest/src/types.rs",
        Kind::OtherMeaning,
        "HOOK_TIMEOUT_MS_MAX = 1000",
    ),
    (
        "crates/tasty-telemetry/src/anomaly.rs",
        Kind::OtherMeaning,
        "CALL_BURST_THRESHOLD = 1000",
    ),
    (
        "src/view/main/redraw.rs",
        Kind::OtherMeaning,
        "재시도 상한 1000",
    ),
    (
        "docs/adr/0205-the-frozen-sum-stays-one-scale.md",
        Kind::OtherMeaning,
        "항목 축 표의 \"54 회\" — 30 일 발화 횟수",
    ),
    (
        "crates/tasty-doc-guards/tests/gate_thresholds_are_not_stale_copies.rs",
        Kind::SelfRef,
        "이 가드 자신의 명부·사유·모듈 주석",
    ),
];

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("읽을 수 없다: {} — {e}", path.display()))
}

/// 정본에서 값을 읽는다. 못 읽으면 **판정 불가로 죽는다** — 0 이나 빈 값을 돌려주면
/// 아래 모든 대조가 조용히 통과한다.
fn source_value(source: (&str, &str)) -> String {
    let (file, prefix) = source;
    parse_source_value(file, prefix, &read(file))
}

/// 이미 읽어 온 정본 텍스트에서 값을 판독한다 — 파일시스템을 안 읽는다.
///
/// `source_value` 에서 갈라 둔 이유는 이 두 실패문(`정확히 1 번` · `숫자가 없다`)이
/// **합성 입력으로 재질 수 있어야** 하기 때문이다. 정본을 실제로 망가뜨려
/// 문구를 읽어 볼 수는 없다 — 그러면 레포의 게이트가 통째로 멈춘다.
///
/// ## 이 판독기 하나에 판정 셋이 매달려 있다 (실측 2026-09-08, 병합 트리 592/0 기준)
///
/// 변이 넷으로 갈랐다. ✕ 가 죽은 것이다.
///
/// | 변이 | ① 파서 픽스처 | ② 사본 판정 | ③ 죽은 면제 |
/// |---|---|---|---|
/// | 이 함수의 값 추출을 한 글자 민다 | ✕ | ✕ | ✕ |
/// | 정본(`clippy.toml`) 값만 20 → 21 | 산다 | ✕ | ✕ |
/// | `EXCLUDED` 에 값을 안 드는 파일 추가 | 산다 | 산다 | ✕ |
/// | 명부의 사본 수만 2 → 3 | 산다 | ✕ | 산다 |
///
/// ① `judgment_wording::a_well_formed_source_line_yields_the_value`
/// ② `the_two_gate_thresholds_have_no_stale_copies`
/// ③ `no_exclusion_is_dead_weight` (병합 트리에만 있다)
///
/// **셋은 곁가지가 아니다** — 뒤 두 변이가 ②③ 을 하나씩만 죽인다. 서로 다른 물음이다.
/// 그런데 **셋 다 이 함수 하나를 거쳐 값을 얻는다.** 그래서 읽는 법이 이렇다:
///
/// - 셋이 **함께** 빨개지면 원인은 이 판독기 하나다. 세 자리를 각각 고치려 들지 마라.
/// - 하나만 빨개지면 그 자리 고유의 회귀다.
/// - ① 은 디스크를 안 읽는다(합성 리터럴이 입력이다). 정본이 사라져도 답하는 **유일한**
///   판사라, "판독기가 옳은데 좌변이 비었다" 와 "판독기가 틀렸다" 를 갈라 준다.
fn parse_source_value(file: &str, prefix: &str, text: &str) -> String {
    let hits = text.match_indices(prefix).count();
    assert_eq!(
        hits, 1,
        "`{file}` 에서 `{prefix}` 가 {hits} 번 나온다 — 정확히 1 번이어야 정본을 읽을 수 있다"
    );
    let rest = &text[text.find(prefix).expect("위에서 1 번을 확인했다") + prefix.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    assert!(
        !digits.is_empty(),
        "`{file}` 의 `{prefix}` 뒤에 숫자가 없다 — 형태가 바뀌었으면 이 판독기를 고쳐라"
    );
    digits
}

/// 낱말 경계 토큰인가 — `2026` 이나 `1000ms` 를 `20`/`1000` 으로 세지 않기 위해서다.
///
/// 순수 함수다 — 파일 없이 변이로 찌를 수 있어야 한다.
fn claims_value(text: &str, value: &str, vocab: &[&str]) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if !has_bare_token(line, value) {
            continue;
        }
        let lo = i.saturating_sub(2);
        let hi = (i + 3).min(lines.len());
        if lines[lo..hi]
            .iter()
            .any(|w| vocab.iter().any(|v| w.contains(v)))
        {
            return true;
        }
    }
    false
}

/// `line` 안에 `token` 이 낱말 경계로 나오는가.
///
/// `.` 을 무조건 경계 밖으로 두면 **문장 끝의 `1000.` 을 못 센다** — 이 판독기가 처음
/// 그랬고 변이 `the_vocabulary_window_reaches_two_lines` 가 그것을 물었다. 그렇다고
/// 경계로 두면 `1.1000.0` 을 센다. 그래서 `.` 은 **숫자에 붙어 있을 때만** 경계 밖이다.
fn has_bare_token(line: &str, token: &str) -> bool {
    let alnum = |c: char| c.is_ascii_alphanumeric() || c == '_';
    line.match_indices(token).any(|(at, _)| {
        let head = &line[..at];
        let tail = &line[at + token.len()..];
        let before = match head.chars().next_back() {
            Some(c) if alnum(c) => true,
            // `1.1000` — 앞의 `.` 이 숫자에 붙어 있으면 그 수의 일부다.
            Some('.') => head[..head.len() - 1]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_digit()),
            _ => false,
        };
        let after = match tail.chars().next() {
            Some(c) if alnum(c) => true,
            Some('.') => tail[1..].chars().next().is_some_and(|c| c.is_ascii_digit()),
            _ => false,
        };
        !before && !after
    })
}

/// 추적 파일 중 텍스트로 훑을 것.
fn tracked_text_files() -> Vec<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root())
        .args(["ls-files"])
        .output()
        .unwrap_or_else(|e| panic!("`git ls-files` 를 실행할 수 없다 — {e}"));
    assert!(
        out.status.success(),
        "`git ls-files` 가 실패했다: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    let exts = [".md", ".rs", ".toml", ".sh", ".yml", ".yaml"];
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|f| exts.iter().any(|e| f.ends_with(e)))
        .map(str::to_string)
        .collect()
}

/// 어휘창을 통과한 파일 전체(과포함 스캔).
fn mentioning_files(value: &str, vocab: &[&str]) -> BTreeSet<String> {
    let root = repo_root();
    let mut out = BTreeSet::new();
    let files = tracked_text_files();
    if let Some(note) = scan_floor_note(files.len()) {
        panic!("{note}");
    }
    for rel in files {
        let path = root.join(rel.split('/').collect::<PathBuf>());
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if claims_value(&text, value, vocab) {
            out.insert(rel);
        }
    }
    out
}

/// 게이트의 **좁은 주제어** — 이 게이트 말고 다른 뜻으로 읽힐 수 없는 낱말.
///
/// 어휘(`*_VOCAB`)와 목적이 반대다. 어휘는 **recall** 을 위해 넓다(`threshold`·`임계`)
/// — 그래서 webhook 의 `threshold: 20` 도 어휘창을 통과하고, 그것이 `EXCLUDED` 가
/// 필요한 이유다. 주제어는 **precision** 을 위해 좁다: 이 낱말이 값 옆에 있으면 그
/// 자리는 다른 뜻일 수가 없다.
/// ★ **린트 이름은 주제어가 아니다.** 첫 판에 넣었다가 실측으로 뺐다:
/// `src/app/event_handler.rs` 의 `App::user_event` 는 그 린트를 그 자리에서 끄면서 같은
/// 줄 사유 주석에 "20여개 variant" 를 달고 있어, 린트 이름을 주제어로 두면 값 옆에
/// 주제어가 선다. 그 자리는 임계에 대한 주장이 아니라 **린트를 끄는 것**이고, 처방을
/// 따라가 보면 둘 다 나쁘다 — 명부로 옮기면 variant 개수를 임계로 세게 되고
/// (임계가 움직이면 거짓 위반), 문장을 고치면 멀쩡한 억제 사유를 낱말 때문에 다시 쓰게
/// 된다. 그래서 주제어는 린트 이름이 아니라 **임계가 사는 자리의 이름**만 든다.
///
/// 이 doc 이 린트 억제의 **형태**(대괄호 속성)를 그대로 적지 않는 것도 그래서다 —
/// 억제를 세는 게이트가 문서 안의 인용까지 세고, 그 좌변은 여유가 0 이다.
const COGNITIVE_SUBJECT: &[&str] = &["cognitive-complexity-threshold", "clippy.toml"];
const SLOC_SUBJECT: &[&str] = &["check-file-size", "THRESHOLD=", "tokei"];
const SHARED_WALK_SUBJECT: &[&str] = &["check-shared-walk-ratchet", "shared_walk_gate"];
const ALLOW_REASON_SUBJECT: &[&str] = &["check-allow-reason", "allow_reason_gate"];
const FROZEN_SUM_SUBJECT: &[&str] = &[
    "frozen-sum-budget",
    "check-frozen-sum-ratchet",
    "frozen_sum_ratchet_gate",
];
const DOC_BIAS_SUBJECT: &[&str] = &["doc-comment-bias", "계측 편향"];

/// 게이트 하나 — 정본·명부·어휘·주제어를 한 값으로 묶는다.
///
/// 한때 이 넷은 각각 상수 짝이었고, 그것을 소비하는 판정 셋이 **손으로 짝지은 배열**을
/// 각자 들고 있었다. 게이트를 하나 더할 때 그 배열 셋을 전부 고쳐야 하고, 하나를
/// 빠뜨리면 그 판정만 새 게이트를 안 본다 — **그 누락은 초록이다.** 값으로 묶으면
/// 좌변이 하나라 빠뜨릴 자리가 없다.
struct Gate {
    label: &'static str,
    source: (&'static str, &'static str),
    claims: &'static [Claim],
    vocab: &'static [&'static str],
    subject: &'static [&'static str],
}

/// 값이 **리터럴로 사는** 게이트 전부.
///
/// ## 좌변이 `scripts/` 만이 아닌 이유 (2026-09-09)
///
/// 한때 이 목록은 `scripts/` 에서 `^[A-Z_]+=<숫자>` 로 세어 넷이었고, 나머지는 "값을 다른
/// 자리에서 유도하니 복제할 수가 없다" 로 넘겼다. **그 서술이 동결 총합 래칫에서 틀렸다.**
/// `check-frozen-sum-ratchet.sh` 가 유도하는 것은 띠(`SLACK` ← `THRESHOLD`)뿐이고,
/// **예산은 유도되지 않는다** — `.complexity-file-allowlist` 의 `# frozen-sum-budget:` 줄에
/// 리터럴로 산다. 사는 파일이 `scripts/` 밖이고 문법이 `NAME=<수>` 가 아니라서 그 세는 법에
/// 안 잡혔을 뿐이다. 즉 그 값은 다른 넷과 똑같이 복제될 수 있다.
///
/// 등록 시점의 사본 수는 **0 이다**(전수 2026-09-09: 값 `35504` 를 드는 자리는 정본과
/// **이 문장 자신** 둘이고 — 이 파일은 `EXCLUDED` 에 `Kind::SelfRef` 로 있어 자기 명부·사유·
/// 모듈 주석이 스캔에서 빠진다 — 옛 값 `36374` 를 드는 자리도 **둘**이다: 스크립트 머리의
/// 시점 서술과 **이 문장 자신**. 둘 다 낡는 것이 정상이다). 처음 적을 때 "정본 하나뿐" 이라고
/// 쓴 것은 세는 자리에서 **세는 문장 자신을 빼먹은 것**이고, 그것을 고친 회차가 **같은 문장
/// 안 옆의 옛 값에 같은 누락을 남겼다** — 이 파일이 `SelfRef` 라 가드는 두 번 다 조용했다.
/// 자기참조를 세는 자리에서는 문장이 값마다 자기를 다시 세야 한다.
///
/// **사본이 아직 0 인 지금** 등록하는 것이 미루지 않는 이유다 — 사본이 생긴 뒤에 등록하면
/// 어느 쪽이 정본인지를 사람이 다시 판정해야 한다. 그리고 예산은 **한 방향으로만 움직여** 되돌아 올라가지 않으므로
/// (ADR-0168), 낡은 사본이 가장 비싼 값이다.
///
/// 자기참조를 세는 자리에서는 이 형태가 반복된다 — 전수를 적는 문장은 자기가 그 전수의
/// 한 항목이 된다. `SelfRef` 가 있는 이유가 그것이고, 그래서 이 자리의 수는 "정본 + 자기" 로
/// 적는다.
const GATES: &[Gate] = &[
    Gate {
        label: "cognitive",
        source: COGNITIVE_SOURCE,
        claims: COGNITIVE_CLAIMS,
        vocab: COGNITIVE_VOCAB,
        subject: COGNITIVE_SUBJECT,
    },
    Gate {
        label: "파일 SLOC",
        source: SLOC_SOURCE,
        claims: SLOC_CLAIMS,
        vocab: SLOC_VOCAB,
        subject: SLOC_SUBJECT,
    },
    Gate {
        label: "공용 순회 래칫",
        source: SHARED_WALK_SOURCE,
        claims: SHARED_WALK_CLAIMS,
        vocab: SHARED_WALK_VOCAB,
        subject: SHARED_WALK_SUBJECT,
    },
    Gate {
        label: "사유 없는 allow 래칫",
        source: ALLOW_REASON_SOURCE,
        claims: ALLOW_REASON_CLAIMS,
        vocab: ALLOW_REASON_VOCAB,
        subject: ALLOW_REASON_SUBJECT,
    },
    Gate {
        label: "동결 총합 예산",
        source: FROZEN_SUM_SOURCE,
        claims: FROZEN_SUM_CLAIMS,
        vocab: FROZEN_SUM_VOCAB,
        subject: FROZEN_SUM_SUBJECT,
    },
    Gate {
        label: "계측 편향",
        source: DOC_BIAS_SOURCE,
        claims: DOC_BIAS_CLAIMS,
        vocab: DOC_BIAS_VOCAB,
        subject: DOC_BIAS_SUBJECT,
    },
];

/// 값 옆(±2 줄, [`claims_value`] 와 **같은 창**)에 좁은 주제어가 있으면 그 자리를 돌려준다.
///
/// 창을 어휘 판정과 같게 두는 것이 중요하다 — 창이 다르면 "어휘창은 통과했는데 주제어
/// 창은 안 통과" 같은 자리가 생기고, 그 차이는 규칙이 아니라 상수의 우연이 된다.
fn subject_near_value(text: &str, value: &str, subject: &[&str]) -> Option<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if !has_bare_token(line, value) {
            continue;
        }
        let lo = i.saturating_sub(2);
        let hi = (i + 3).min(lines.len());
        for w in &lines[lo..hi] {
            if let Some(s) = subject.iter().find(|s| w.contains(**s)) {
                return Some((i + 1, (*s).to_string()));
            }
        }
    }
    None
}

/// "다른 뜻" 이라는 주장이 틀렸을 때의 판정문.
///
/// 파일시스템을 안 읽는다.
fn mislabeled_meaning_note(
    file: &str,
    label: &str,
    subject: &str,
    line: usize,
    why: &str,
) -> String {
    format!(
        "`{file}:{line}` 는 갈래 `OtherMeaning`(사유 \"{why}\") 인데 {label} 게이트의 \
         주제어 `{subject}` 를 값 바로 옆에 두고 있다 — 그 자리는 **다른 뜻일 수가 없다**.\n  \
         [왜 이것이 위반인가] 미분류 자리의 처방은 \"명부에 넣거나 사유와 함께 제외에 \
         넣어라\" 로 **두 통을 함께 내민다.** 어느 통이 맞는지는 아무것도 안 보고 있었다. \
         잘못 든 통은 조용하다 — 명부였어야 할 자리가 제외에 앉으면 그 값이 낡아도 \
         아무 말이 없다.\n  \
         [처방] 이 자리가 현재 상태를 주장하면 `*_CLAIMS` 로 옮겨라. 정말 다른 뜻이면 \
         주제어가 값 옆에 오지 않게 문장을 다시 써라 — 갈래를 바꾸는 것은 처방이 아니다."
    )
}

/// 스캔에 더 이상 안 걸리는 제외 항목에 대한 판정문.
///
/// 파일시스템을 안 읽는다 — 호출자가 판정 결과를 넘긴다.
///
/// 왜 이것이 위반인가: 제외는 **면제를 발행하는 자리**다. 그 자리가 더 이상 스캔에 안
/// 걸리면 면제는 아무것도 안 막고 있고, 그 파일이 나중에 **진짜 낡은 사본을 얻어도
/// 조용히 통과한다.** 즉 쓰이지 않는 면제는 중립이 아니라 **앞으로 열릴 구멍**이다.
/// 사유 칸은 그때 도움이 안 된다 — 사유는 왜 뺐는지를 말할 뿐 지금도 빼야 하는지를
/// 말하지 않는다.
fn dead_exclusion_note(file: &str, kind: Kind, why: &str) -> String {
    format!(
        "`{file}` 는 제외 목록에 있지만 두 임계 어느 쪽도 어휘창 안에서 안 든다 \
         (갈래 {kind:?} · 사유 \"{why}\").\n  \
         [처방] 이 항목을 지워라. 남겨 두면 그 파일이 나중에 진짜 낡은 사본을 얻어도 \
         면제가 먼저 걸려 조용히 통과한다 — 제외는 중립이 아니라 발행된 면제다.\n  \
         [지우면 안 되는 경우] 어휘가 그 자리에서 잠깐 빠졌을 뿐이라면 그 문장을 다시 \
         쓰는 것이 먼저다. 판정 기준은 사유가 아니라 **지금 스캔에 걸리는가**이고, \
         안 걸리는 자리는 제외가 아니라 그냥 무관한 파일이다."
    )
}

/// 명부 한 자리에 대한 축 ① 판정문. 문제가 없으면 `None`.
///
/// 파일시스템을 안 읽는다 — `text` 는 호출자가 읽어 넘긴다. 세 갈래가 여기서
/// 갈리고, 셋의 처방이 서로 다르다:
/// - 어휘와 떨어졌다 → 문장을 다시 쓰거나 명부에서 뺀다. **값은 건드리지 않는다**
/// - 값이 아예 없다 → 낡은 값을 들고 있는 것이니 그 자리를 고친다
/// - 사본 줄 수가 다르다 → [`stale_copy_note`]
fn claim_note(
    rel: &str,
    label: &str,
    value: &str,
    source_file: &str,
    text: &str,
    vocab: &[&str],
    copies: usize,
) -> Option<String> {
    if claims_value(text, value, vocab) {
        // 파일 단위로는 한 줄만 맞아도 통과하고, 그러면 같은 파일의 낡은 사본이
        // 조용히 남는다(변이 E 로 실측).
        let seen = text.lines().filter(|l| has_bare_token(l, value)).count();
        return stale_copy_note(rel, label, value, seen, copies);
    }
    // 값 토큰 자체가 파일에 있나 — 이 한 물음이 두 상태를 가른다.
    if text.lines().any(|l| has_bare_token(l, value)) {
        Some(format!(
            "  {rel}: {label} 임계값 `{value}` 은 있는데 게이트 어휘와 **떨어져** 있다 \
             — 이 자리가 그 게이트를 말하기를 그만뒀거나, 문구가 바뀌어 어휘창 밖으로 \
             나갔다. 여전히 주장하는 자리면 그 문장이 무엇의 임계인지 드러나게 쓰고, \
             더는 주장하지 않으면 `*_CLAIMS` 에서 빼라. **값을 다시 적지 마라 — \
             값은 이미 맞다**"
        ))
    } else {
        Some(format!(
            "  {rel}: {label} 임계를 주장하는 자리인데 지금 값 `{value}` 이 아예 없다 \
             — 정본은 `{source_file}` 이다. 낡은 값을 들고 있으면 그 자리를 고쳐라"
        ))
    }
}

/// 축 ② — 스캔에 걸렸는데 명부에도 제외에도 없는 자리.
fn unclassified_note(rel: &str, label: &str, value: &str) -> String {
    format!(
        "  {rel}: {label} 임계값 `{value}` 을 게이트 어휘와 함께 드는데 분류가 없다. \
         현재 상태를 주장하면 명부(`*_CLAIMS`)에, 시점 측정·인용·다른 뜻이면 \
         사유와 함께 `EXCLUDED` 에 넣어라"
    )
}

/// 스캔 모수의 바닥. 넘으면 `None`.
///
/// 이 하한은 **미분류 0 이 초록인 이유가 둘**이라 필요하다 — 정말 없거나, 안 봤거나.
fn scan_floor_note(seen: usize) -> Option<String> {
    (seen <= 500).then(|| {
        format!("추적 텍스트 파일을 {seen} 개만 읽었다 — 모수가 무너지면 미분류 0 도 초록이 된다")
    })
}

/// 사본 줄 수가 명부와 다를 때의 판정문. 같으면 `None`.
///
/// `check` 에서 갈라 둔 이유는 이 문구가 **파일시스템 없이 재질 수 있어야** 하기
/// 때문이다 — 문구를 읽으려고 레포의 문서를 고쳐 볼 수는 없다.
fn stale_copy_note(
    rel: &str,
    label: &str,
    value: &str,
    seen: usize,
    expected: usize,
) -> Option<String> {
    if seen == expected {
        return None;
    }
    Some(format!(
        "  {rel}: {label} 임계값 `{value}` 을 든 줄이 {seen} 개다(명부 {expected}).\n  \
         줄었으면 **그중 하나가 낡은 값으로 남아 있을 수 있다** — 이 파일에서 `{value}` 을 \
         든 줄과 다른 수를 든 줄을 함께 훑어라. 늘었으면 새 사본이 생긴 것이니 그 줄이 \
         현재 상태를 주장하는지 확인해라.\n  \
         [시점] 짚힌 줄이 '그때 그 값이어서 이렇게 했다' 는 서술이면 **값을 고치지 마라** \
         — 지금 값으로 고치면 없던 거짓이 새로 생긴다. 그때는 그 줄을 그대로 두고 이 수를 \
         맞춰라. 라벨의 뜻은 `ci_channel_claims_match_workflows.rs` 의 `TIME_NOTE` 가 \
         정한다(같은 개념에 이름을 새로 만들지 않는다)."
    ))
}

fn check(label: &str, source: (&str, &str), claims: &[Claim], vocab: &[&str]) -> Vec<String> {
    let value = source_value(source);
    let mut wrong = Vec::new();

    // ① 명부의 각 자리가 지금 값을 드는가.
    //
    // 안 들 때 **상태가 둘**이라 갈라 보고한다. 한 칸에 섞으면 실패문이 한쪽에게 틀린
    // 처방을 준다 — 실측: 어휘 다중도 1 인 자리에서 그 낱말 하나만 다른 말로 바꿨더니
    // "지금 값을 안 든다, 그 자리를 고쳐라" 가 나왔는데 **그 파일은 그 값을 들고
    // 있었다.** 따르면 아무것도 안 고쳐지거나 중복을 쓴다.
    for (rel, copies) in claims {
        if let Some(note) = claim_note(rel, label, &value, source.0, &read(rel), vocab, *copies) {
            wrong.push(note);
        }
    }

    // ② 미분류 신규 자리 — 스캔 생존자가 전부 명부/제외 안인가.
    let known: BTreeSet<&str> = claims
        .iter()
        .map(|(f, _)| *f)
        .chain(EXCLUDED.iter().map(|(f, _, _)| *f))
        .collect();
    for rel in mentioning_files(&value, vocab) {
        if !known.contains(rel.as_str()) {
            wrong.push(unclassified_note(&rel, label, &value));
        }
    }
    wrong
}

#[test]
fn the_gate_thresholds_have_no_stale_copies() {
    let mut wrong = Vec::new();
    for g in GATES {
        wrong.extend(check(g.label, g.source, g.claims, g.vocab));
    }
    let sources: Vec<String> = GATES
        .iter()
        .map(|g| {
            format!(
                "`{}` 의 `{}` = {}",
                g.source.0,
                g.source.1.trim_end_matches(" = ").trim_end_matches('='),
                source_value(g.source)
            )
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "게이트 임계값의 사본이 정본과 갈렸거나, 분류되지 않은 주장 자리가 생겼다.\n\
         정본: {}\n{}",
        sources.join(" · "),
        wrong.join("\n")
    );
}

/// 판정기가 실제로 무는지 확인하는 변이 — 파일은 안 고친다.
/// "다른 뜻" 이라는 갈래 주장을 검사한다.
///
/// `Kind::OtherMeaning` 은 **검사 가능한 주장**이다 — 다른 뜻이라면 이 게이트의 좁은
/// 주제어가 값 옆에 있을 리 없다. 이 판정이 없는 동안 그 주장을 보는 것은 아무것도
/// 없었고, 미분류 자리의 처방이 통 둘을 함께 내밀고 있었다([`unclassified_note`]).
///
/// ★ **빈 좌변의 초록을 막는다.** 검사한 (파일, 값) 짝이 0 이면 그 자체로 실패다 —
/// 위반 0 과 볼 것이 0 은 화면에서 같고, 갈래 이름이 바뀌거나 목록이 비면 이 판정은
/// 조용히 아무것도 안 보게 된다.
#[test]
fn a_different_meaning_exclusion_does_not_carry_the_gate_subject() {
    let values: Vec<String> = GATES.iter().map(|g| source_value(g.source)).collect();
    let mut checked = 0usize;
    let mut wrong = Vec::new();
    for (file, kind, why) in EXCLUDED {
        if *kind != Kind::OtherMeaning {
            continue;
        }
        let text = read(file);
        for (g, value) in GATES.iter().zip(&values) {
            if !claims_value(&text, value, g.vocab) {
                continue;
            }
            checked += 1;
            if let Some((line, hit)) = subject_near_value(&text, value, g.subject) {
                wrong.push(mislabeled_meaning_note(file, g.label, &hit, line, why));
            }
        }
    }
    assert!(
        checked > 0,
        "`Kind::OtherMeaning` 으로 검사한 (파일, 값) 짝이 0 이다 — 이 판정은 빈 좌변 \
         위에서 초록이 된다. 갈래 이름이 바뀌었거나 어휘 판정이 죽었다."
    );
    assert!(
        wrong.is_empty(),
        "\"다른 뜻\" 이라는 제외 사유가 {} 짝 중 {} 곳에서 틀렸다.\n{}",
        checked,
        wrong.len(),
        wrong.join("\n")
    );
}

/// 제외가 **아직 무언가를 막고 있는가**. 안 막고 있으면 지워야 한다.
///
/// **오늘 걸리는 자리는 0 이다**(제외 29 · 2026-09-08 · `b134d28e3`+). 그래서 이 판정은
/// 실제 사건을 되짚어 넣은 것이 아니라 **면제 목록이 자란다는 이 파일 자신의 서술**
/// ("`EXCLUDED` 는 자란다 — 결함이 아니다")에 붙는 반대쪽 래칫이다: 자라는 목록에
/// 물러나는 길이 없으면 면제만 쌓인다. 사유 칸은 그 길이 못 된다 — 사유는 왜 뺐는지를
/// 말하지 **지금도 빼야 하는지**를 말하지 않는다.
///
/// ★ 이 자리에 한때 "제외 29 중 둘이 죽어 있었다" 고 적으려 했다. 그 둘
/// (`docs/adr/0119-…` 의 "20분", `src/app/event_handler.rs` 의 "20여개")은 **죽지 않았다** —
/// 그렇게 읽은 계측기가 python 이었고 python 의 `str.isalnum()` 은 유니코드라 `20여개` 의
/// `여` 를 낱말 안으로 셌다. [`has_bare_token`] 은 `is_ascii_alphanumeric` 이라 거기서
/// 낱말이 끊긴다. 판정기가 옳았고 사본이 틀렸다.
#[test]
fn no_exclusion_is_dead_weight() {
    let values: Vec<String> = GATES.iter().map(|g| source_value(g.source)).collect();
    let mut dead = Vec::new();
    for (file, kind, why) in EXCLUDED {
        let text = read(file);
        if !GATES
            .iter()
            .zip(&values)
            .any(|(g, value)| claims_value(&text, value, g.vocab))
        {
            dead.push(dead_exclusion_note(file, *kind, why));
        }
    }
    assert!(
        dead.is_empty(),
        "제외 목록에 죽은 면제가 {} 개 있다 — 제외는 발행된 면제라 안 쓰이면 지워야 한다.\n{}",
        dead.len(),
        dead.join("\n")
    );
}

mod threshold_mutations {
    use super::*;

    /// 두 상태가 실제로 갈리는가 — 값이 없는 것과, 값은 있는데 어휘가 떨어진 것.
    #[test]
    fn the_two_failure_states_are_told_apart() {
        // 값은 있는데 어휘가 창 밖 — 어휘가 있어야 주장으로 센다.
        let drifted = "함수 크기 상한(20) 을 넘기지 않도록.\n";
        assert!(!claims_value(drifted, "20", COGNITIVE_VOCAB));
        assert!(drifted.lines().any(|l| has_bare_token(l, "20")));
        // 값 자체가 없다.
        let gone = "인지 복잡도 상한(25) 을 넘기지 않도록.\n";
        assert!(!claims_value(gone, "20", COGNITIVE_VOCAB));
        assert!(!gone.lines().any(|l| has_bare_token(l, "20")));
    }

    #[test]
    fn a_stale_copy_is_caught() {
        let doc = "임계 20 이다.\n";
        assert!(claims_value(doc, "20", COGNITIVE_VOCAB));
        assert!(
            !claims_value(doc, "25", COGNITIVE_VOCAB),
            "옛 값을 새 값으로 셌다"
        );
    }

    #[test]
    fn a_number_without_gate_vocabulary_is_not_a_claim() {
        assert!(!claims_value("포트 1000 을 연다.\n", "1000", SLOC_VOCAB));
    }

    #[test]
    fn the_vocabulary_window_reaches_two_lines() {
        let md = "파일 SLOC 상한을 말한다.\n중간 줄.\n값은 1000.\n";
        assert!(claims_value(md, "1000", SLOC_VOCAB));
        let far = "파일 SLOC 상한을 말한다.\n한 줄.\n두 줄.\n세 줄.\n값은 1000.\n";
        assert!(!claims_value(far, "1000", SLOC_VOCAB), "창 밖까지 셌다");
    }

    #[test]
    fn a_glued_token_is_not_counted() {
        assert!(!has_bare_token("연도는 2026 이다", "20"));
        assert!(
            has_bare_token("임계는 1000.", "1000"),
            "문장 끝의 마침표를 경계로 안 봤다"
        );
        assert!(!has_bare_token("타임아웃 1000ms", "1000"));
        assert!(!has_bare_token("버전 1.1000.0", "1000"));
        assert!(has_bare_token("임계 1000 이다", "1000"));
    }

    #[test]
    fn every_excluded_entry_carries_a_reason() {
        for (file, _, why) in EXCLUDED {
            assert!(!why.trim().is_empty(), "{file} 의 제외 사유가 비었다");
        }
    }

    #[test]
    fn no_file_is_both_claimed_and_excluded() {
        let excluded: BTreeSet<&str> = EXCLUDED.iter().map(|(f, _, _)| *f).collect();
        for (rel, _) in GATES.iter().flat_map(|g| g.claims.iter()) {
            assert!(
                !excluded.contains(rel),
                "{rel} 가 명부와 제외목록에 둘 다 있다"
            );
        }
    }
}

/// 사본 줄 수 판정의 **문구** 양성 대조. 파일시스템을 안 읽는다.
///
/// 명부의 실제 수(`COGNITIVE_CLAIMS` 등)를 여기 안 쓴다 — 자기가 재려는 값으로 자기를
/// 지으면 그 값에 대해 항진명제가 된다. 여기 값은 메커니즘을 발화시키기 위한
/// 리터럴이고, 실측 수가 맞는지는 본 시험이 잰다.
///
/// ## 이 픽스처들이 실제로 무는가 (실측 2026-09-08, 내 트리 · 매 변이 패키지 전체 569)
///
/// | 변이 | rc | 죽은 시험 |
/// |---|---|---|
/// | `stale_copy_note` 에서 같을 때의 조용한 갈래 삭제 | 101 | `a_matching_count_says_nothing` + `judgment_wording::a_healthy_claim_says_nothing` + 본 판정 |
/// | 〃 의 줄어든 쪽 처방 문구 교체 | 101 | `a_shrunk_count_warns_that_one_copy_may_be_stale` — 이것만 |
/// | 〃 의 시점 라벨 정의 자리 인용 제거 | 101 | `the_note_forbids_editing_a_dated_line_and_points_at_the_existing_label` — 이것만 |
///
/// 첫 변이가 셋을 함께 죽이는 것이 옳다 — 조용한 갈래를 지우면 건강한 주장까지 위반이
/// 되므로 세 자리가 같은 회귀를 각자의 문장으로 말한다.
#[cfg(test)]
mod copy_count_wording {
    use super::stale_copy_note;

    #[test]
    fn a_matching_count_says_nothing() {
        assert!(stale_copy_note("a.md", "파일 SLOC", "1000", 3, 3).is_none());
    }

    #[test]
    fn a_shrunk_count_warns_that_one_copy_may_be_stale() {
        let m = stale_copy_note("a.md", "파일 SLOC", "1000", 2, 3).expect("발화해야 한다");
        assert!(m.contains("든 줄이 2 개다(명부 3)"), "{m}");
        assert!(m.contains("낡은 값으로 남아 있을 수 있다"), "{m}");
        assert!(m.contains("a.md"), "좌표가 없다: {m}");
    }

    /// 늘어난 쪽의 처방은 다르다 — 새 사본이 현재 상태를 주장하는지 확인하는 것이다.
    #[test]
    fn a_grown_count_asks_whether_the_new_line_claims_the_present() {
        let m = stale_copy_note("a.md", "cognitive", "20", 4, 3).expect("발화해야 한다");
        assert!(m.contains("든 줄이 4 개다(명부 3)"), "{m}");
        assert!(m.contains("새 사본이 생긴 것"), "{m}");
    }

    /// ③ 시점 서술을 지금 값으로 고치면 없던 거짓이 생긴다 — 그 경고가 문구에 있어야
    /// 하고, 라벨의 정의는 **이미 있는 것을 가리킨다**(새로 만들지 않는다).
    #[test]
    fn the_note_forbids_editing_a_dated_line_and_points_at_the_existing_label() {
        let m = stale_copy_note("a.md", "파일 SLOC", "1000", 2, 3).expect("발화해야 한다");
        assert!(m.contains("[시점]"), "{m}");
        assert!(m.contains("값을 고치지 마라"), "{m}");
        assert!(m.contains("TIME_NOTE"), "라벨 정의를 안 가리킨다: {m}");
    }
}

/// 나머지 판정 다섯의 **문구** 양성 대조. 파일시스템을 안 읽는다.
///
/// 이 모듈이 쓰는 값은 전부 자기 리터럴이다 — 가드의 명부·하한 상수에서 뽑아 쓰면
/// 그 상수에 대해 항진명제가 된다.
///
/// ## 이 픽스처들이 실제로 무는가 (실측 2026-09-08, 내 트리 · 매 변이 패키지 전체 569)
///
/// | 변이 | rc | 죽은 시험 |
/// |---|---|---|
/// | `parse_source_value` 의 "정확히 1 번" 무력화 | 101 | `a_source_prefix_that_appears_twice_refuses_to_guess` — 이것만 |
/// | 〃 의 "숫자가 없다" 단정 삭제 | 101 | `a_source_prefix_without_digits_says_the_reader_is_stale` — 이것만 |
/// | 〃 의 값 추출을 한 글자 밀기 | 101 | `a_well_formed_source_line_yields_the_value` + 본 판정 + `no_exclusion_is_dead_weight` |
/// | `scan_floor_note` 의 하한 500 → 2 | 101 | `a_collapsed_scan_says_zero_would_be_green` — 이것만 |
/// | `unclassified_note` 에서 제외 명부 이름 제거 | 101 | `an_unclassified_site_is_offered_both_bins` — 이것만 |
/// | `claim_note` 의 사본 계수를 하나 부풀림 | 101 | `a_healthy_claim_says_nothing` + 본 판정 |
///
/// 다섯 중 넷이 배타적이고, 나머지는 본 판정과 함께 죽는다 — 그 둘은 판정 능력이 겹치는
/// 것이 아니라 **처방이 갈리는** 자리다(본 판정은 "어느 파일이 낡았다", 이쪽은 "판독기가
/// 깨졌다").
///
/// ★ 세 번째 줄의 `no_exclusion_is_dead_weight` 는 **병합된 트리에서만** 함께 죽는다.
/// 그 시험은 `source_value` → [`parse_source_value`] 를 소비하므로, 값 추출이 밀리면
/// 임계값 대조가 전부 거짓이 되어 `EXCLUDED` 전 항목이 죽은 면제로 몰린다. 값 추출을
/// **느슨하게** 하는 위 두 변이는 실물 정본이 그 갈래를 안 밟아 그 시험을 안 죽인다 —
/// 셋 다 병합 트리에서 실측했다(위 표의 나머지 여섯은 두 트리에서 같은 답이 나왔다).
#[cfg(test)]
mod judgment_wording {
    use super::{
        claim_note, mislabeled_meaning_note, parse_source_value, scan_floor_note,
        subject_near_value, unclassified_note,
    };

    const VOCAB: &[&str] = &["임계", "복잡도"];

    /// 주제어가 창 안에 있으면 그 줄을 집는다 — 값과 같은 줄이 아니어도 된다.
    ///
    /// 리터럴로 짓는다: 가드의 `*_SUBJECT` 를 갖다 쓰면 그 상수에 대해 항진명제가
    /// 된다. 여기서 재는 것은 **창과 낱말 경계의 메커니즘**이다.
    #[test]
    fn a_subject_two_lines_above_the_value_is_found() {
        let text = "check-file-size 를 설명한다\n\n상한은 1000 줄이다\n";
        let hit = subject_near_value(text, "1000", &["check-file-size"]);
        assert_eq!(hit, Some((3, "check-file-size".to_string())));
    }

    /// 창은 ±2 줄이다 — 세 줄 떨어지면 안 집는다. 창을 넓히는 변경이 이 시험을 깨운다.
    #[test]
    fn a_subject_three_lines_away_is_out_of_the_window() {
        let text = "check-file-size 를 설명한다\n\n\n\n상한은 1000 줄이다\n";
        assert_eq!(subject_near_value(text, "1000", &["check-file-size"]), None);
    }

    /// 값이 낱말의 일부면 자리로 안 센다 — `1000ms` 옆의 주제어는 위반이 아니다.
    #[test]
    fn a_glued_value_is_not_a_site_even_next_to_the_subject() {
        let text = "check-file-size\n타임아웃 1000ms\n";
        assert_eq!(subject_near_value(text, "1000", &["check-file-size"]), None);
    }

    /// 판정문은 **두 처방을 갈라** 싣는다 — 갈래를 바꾸는 것은 처방이 아니라고 못 박는다.
    #[test]
    fn the_note_offers_the_roster_or_a_rewrite_but_not_a_relabel() {
        let m = mislabeled_meaning_note("a/b.md", "파일 SLOC", "check-file-size", 7, "사유");
        assert!(m.contains("a/b.md:7"), "{m}");
        assert!(m.contains("CLAIMS"), "명부로 옮기라는 갈래가 없다: {m}");
        assert!(
            m.contains("문장을 다시 써라"),
            "다시 쓰라는 갈래가 없다: {m}"
        );
        assert!(
            m.contains("갈래를 바꾸는 것은 처방이 아니다"),
            "가장 싼 수선을 금지하는 문장이 없다: {m}"
        );
    }

    /// 값이 있고 어휘도 붙어 있고 줄 수도 맞으면 아무 말도 안 한다.
    #[test]
    fn a_healthy_claim_says_nothing() {
        let text = "이 게이트의 임계는 20 이다.\n";
        assert!(claim_note("a.md", "cognitive", "20", "clippy.toml", text, VOCAB, 1).is_none());
    }

    /// 값은 있는데 어휘가 멀면 처방이 "값을 고쳐라" 가 **아니다** — 그 자리를 고치거나
    /// 명부에서 빼는 것이다. 이 갈래를 안 나눠 거짓 처방이 나간 적이 있다.
    #[test]
    fn a_value_far_from_the_vocabulary_is_told_not_to_rewrite_the_value() {
        let text = "복잡도를 말한다.\n한 줄.\n두 줄.\n세 줄.\n무관한 문장 20 개.\n";
        let m = claim_note("a.md", "cognitive", "20", "clippy.toml", text, VOCAB, 1)
            .expect("발화해야 한다");
        assert!(m.contains("떨어져"), "{m}");
        assert!(m.contains("값을 다시 적지 마라"), "{m}");
    }

    /// 값이 아예 없으면 그때는 "그 자리를 고쳐라" 가 맞고, 정본이 어디인지 함께 준다.
    #[test]
    fn a_missing_value_names_the_source_of_truth() {
        let text = "이 게이트의 임계는 15 이다.\n";
        let m = claim_note("a.md", "cognitive", "20", "clippy.toml", text, VOCAB, 1)
            .expect("발화해야 한다");
        assert!(m.contains("아예 없다"), "{m}");
        assert!(m.contains("clippy.toml"), "정본을 안 가리킨다: {m}");
    }

    #[test]
    fn an_unclassified_site_is_offered_both_bins() {
        let m = unclassified_note("a.md", "파일 SLOC", "1000");
        assert!(m.contains("분류가 없다"), "{m}");
        assert!(m.contains("_CLAIMS"), "{m}");
        assert!(m.contains("EXCLUDED"), "{m}");
    }

    #[test]
    fn a_collapsed_scan_says_zero_would_be_green() {
        assert!(scan_floor_note(9000).is_none());
        let m = scan_floor_note(3).expect("발화해야 한다");
        assert!(m.contains("3 개만 읽었다"), "{m}");
        assert!(m.contains("미분류 0 도 초록"), "{m}");
    }

    #[test]
    #[should_panic(expected = "정확히 1 번이어야")]
    fn a_source_prefix_that_appears_twice_refuses_to_guess() {
        parse_source_value("clippy.toml", "T = ", "T = 20\nT = 30\n");
    }

    #[test]
    #[should_panic(expected = "숫자가 없다")]
    fn a_source_prefix_without_digits_says_the_reader_is_stale() {
        parse_source_value("clippy.toml", "T = ", "T = abc\n");
    }

    #[test]
    fn a_well_formed_source_line_yields_the_value() {
        assert_eq!(parse_source_value("clippy.toml", "T = ", "T = 20\n"), "20");
    }
}
