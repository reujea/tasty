# ADR-0254: 떠 있는 표면의 그림자는 두 값뿐이고, 뷰포트를 점유하는 쪽이 더 크다

- **Status**: Accepted
- **Date**: 2026-09-09
- **Tags**: design-tokens, theme, shadow, modal, popover, scrim, gallery, guards, adr-0139
- **Group**: ui-theme-gallery

## Context

떠 있는 표면(popover · 모달 · tooltip · 드롭다운)이 자기 아래 콘텐츠와 분리돼 보이려면
단차가 필요하다. tasty 의 Rust `Theme` 에는 그 단차가 **하나뿐**이었다 —
`SHADOW_POPOVER`(`crates/tasty-type-appearance/src/theme.rs`). 그 하나를 모든 떠 있는
표면이 재사용하고, "새 그림자 값을 만들지 않는다" 는 정책을
`crates/tasty-type-appearance/src/shadow_policy_guard.rs` 가 소스 스캔으로 집행했다.

그 정책은 모달에 대해서는 답을 갖고 있지 않았다. 호스트 모달(`PopupManager`)과 plugin
popup 셸은 `Frame::new()` 로 그려 **그림자가 아예 없었고**, 그것이 "없어야 한다" 로
결정된 것인지 아무도 정한 적이 없다는 사실이 가드 모듈 문서와
`docs/design/systems/theme.md` 양쪽에 미결로 적혀 있었다. 그 미결이 남긴 자국은 셋이다.

1. **갤러리·문서가 없는 것을 있다고 적었다.** 모달 specimen 여럿이 "modal shadow" 를
   전시 문자열로 내걸었지만 그 specimen 도, 본체도 그림자를 그리지 않았다.
2. **값이 구현 없이 코드로 새어 들어왔다.** 갤러리 tools menu 카드가 토큰이 아닌 raw
   리터럴(`0 8px 28px /.4`)을 전시 문자열에 박아뒀다. 어느 정본에도 없는 값이다.
3. **한 표면이 두 값 사이에서 표류했다.** modifier-hint 는 디자인 토큰이 `shadow-modal`
   인데 구현은 `shadow_popover()` 로 매핑돼 있었고, 그 발산이 매핑 문서에 기록으로
   남아 있었다.

세 자국 모두 "어느 표면이 어느 값을 쓰는가" 라는 물음에 정본이 없어서 생겼다.

## Decision

떠 있는 표면의 그림자를 **두 값**으로 정하고, 어느 쪽을 쓰는지는 표면의 형태가 정한다.

- **anchored + scrim-less**(트리거 옆에 붙어 살아 있는 콘텐츠 위에 뜬다) → `SHADOW_POPOVER`
- **centered**(뷰포트를 점유한다) → `SHADOW_MODAL`
- 위 둘 중 어느 형태도 아닌 떠 있는 표면 → **그림자 없음**

**scrim 유무는 갈래를 가르는 술어가 아니다.** centered 표면 중 실제로 scrim 을 까는 것은
일부이고(`PopupManager` 는 headless 모달 id 세트에만 scrim 을 그린다), 나머지 centered
표면도 같은 `SHADOW_MODAL` 을 받는다. scrim 은 그 값을 popover 보다 크게 잡은 **근거**로
아래에 등장할 뿐이다 — 판정은 "뷰포트를 점유하는가" 로만 한다. 구현의 술어도 같은
모양이다(명부 둘의 여집합: shadowless 도 anchored 도 아니면 modal).

`SHADOW_MODAL` 은 `SHADOW_POPOVER` 보다 **크다**. 그림자의 역할이 "자기가 소유하지 않은
콘텐츠 위에 떠 있음" 을 알리는 것이라면, scrim 이 이미 그 역할을 하는 표면은 그림자를
줄이는 것이 자연스러워 보인다. 그 직관이 틀린 이유는 **scrim 이 엣지를 그리지 않기**
때문이다. scrim 은 바닥 전체를 균일하게 어둡게 할 뿐이라, 어두운 테마에서 어두운 모달이
어두워진 바닥 위에 놓이면 경계 자체가 사라진다 — 1px 보더만으로는 실루엣이 서지 않는다.
scrim 이 지운 대비를 그림자가 되돌려야 하므로 값이 더 커진다.

세 번째 갈래("그림자 없음")는 누락이 아니라 결정이다. 두 형태 어디에도 안 들어가는
표면 — 예컨대 타이틀바를 갖고 사용자가 옮기는 알림 패널 — 은 창처럼 동작하지 popover
처럼 뜨지 않는다. 그런 표면에 둘 중 하나를 골라 주는 대신 그림자를 주지 않는다.

`ShadowToken.spread` 는 이 결정과 함께 **음수를 표현하는 계약**을 갖는다(CSS
`box-shadow` 의 spread 와 같은 의미). 디자인 `--tasty-titlebar-csd-shadow` 의 `-8px` 는
장식이 아니라 falloff 를 리사이즈 엣지 밴드 안으로 묶는 기능이라, 근사값으로 대체할 수
없다. egui 0.31 의 `epaint::Shadow::spread` 는 `u8` 이라 음수를 담지 못하므로 —
`as u8` 캐스트는 음수를 조용히 0 으로 saturate 한다 — 음수 spread 를 쓰는 표면은
**근사하지 않고 미구현으로 둔다.** 그 왜곡이 조용하지 않도록 `to_egui()` 가 debug 빌드
에서 단언으로 터진다.

## Consequences

- **얻은 것**: "어느 표면이 어느 그림자를 쓰는가" 에 정본이 생겼다. 갤러리 전시 문자열과
  본체 구현이 같은 규칙을 인용하므로, 둘이 갈리면 그것이 규칙 위반으로 읽힌다. 모달이
  어두운 테마에서 바닥과 분리된다.
- **얻은 것**: 음수 spread 의 부재가 "지원하지 않는다" 는 **기록된 결정**이 됐다. 전에는
  `as u8` 이 조용히 0 으로 만들어, 누가 음수 토큰을 들이면 다른 그림자가 그려지고도
  아무 신호가 없었다.
- **잃은 것**: 그림자 값이 하나에서 둘이 됐다. "새 그림자 시스템을 만들지 않는다" 는
  옛 정책의 단순함은 사라지고, 대신 값마다 어느 표면이 쓰는지를 규칙이 답해야 한다.
- **운영 비용**: 새 떠 있는 표면을 만들 때 세 갈래 중 하나를 고르는 판단이 붙는다. 그
  판단은 디자인의 몫이고 구현이 임의로 정하지 않는다. `shadow_policy_guard` 는
  인식하는 그림자 리터럴의 생성 위치·접근자에서 받은 변수의 기하 재대입·접근자 명부를
  검사한다. **egui 기본 그림자 매핑과 어느 표면이 어느 값을 쓰는가는 검사하지 않는다.**
  줄 단위 텍스트 검사로 표면의 형태(트리거에 앵커되는가 · 뷰포트를 점유하는가)를
  판정하지 않으며, 그 축은 리뷰가 지킨다.

## Alternatives Considered

- **popover 값을 모달에도 재사용한다** — 값이 하나로 유지되어 정책이 단순하다. 하지만
  scrim 이 지운 대비를 popover 단차(`0 6px 18px`)로는 되돌리지 못한다. scrim 위에서
  그림자가 거의 안 보여, 모달만 유독 바닥에 눌어붙는다.
- **모달은 그림자 없이 scrim 과 1px 보더로만 분리한다** — 코드 변경이 가장 적고, 실제로
  결정 전의 현상이었다. 어두운 테마에서 어두운 모달 + 어두워진 바닥 + 1px 보더 조합이
  실루엣을 잃는 것이 이 안을 기각한 이유다. scrim 은 균일하게 어둡게 할 뿐 경계를 그리지
  않는다.
- **표면마다 전용 그림자 토큰을 둔다** — 디자인이 각 컴포넌트에 정확한 값을 줄 수 있다.
  기각: 값이 늘어나는 만큼 "왜 이 표면만 다른가" 를 답할 근거가 매번 필요해지고, 그
  근거가 없으면 값이 표류한다(modifier-hint 가 두 값 사이에서 표류한 것이 그 형태였다).
- **음수 spread 를 0 으로 근사해 CSD 그림자를 지금 그린다** — 타이틀바가 그림자를 갖게
  된다. 기각: `-8px` 는 falloff 를 8px 밴드 안에 묶는 **기능**이라, 0 으로 만들면 그림자가
  리사이즈 엣지 밖으로 새어 다른 그림을 그린다. 디자인이 명시적으로 금지했다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `Theme` 가 노출하는 `shadow_*()` 접근자가 둘이 아니게 된다. 세 번째 값이 생겼다면
  SCOPE RULE 의 두 갈래로는 표면을 다 가르지 못한다는 뜻이다.
  `crates/tasty-type-appearance/src/shadow_policy_guard.rs` 의
  `shadow_accessor_list_matches_theme_accessors` 가 접근자 명부를 theme.rs 에서 다시
  읽어 대조하므로, 접근자가 늘면 그 시험이 먼저 실패한다.
- 정본에 **세 번째 raw 그림자 값**이 들어오거나, 기존 alias 토큰이 명부 밖 값을 가리키게
  된다 — `crates/tasty-design-tokens/tests/shadow_parity.rs` 의
  `raw_shadow_token_roster_is_complete` · `alias_shadow_tokens_land_on_a_listed_raw_value`
  가 vendor json 을 훑어 잡는다. 그 시험은 같은 파일에서 두 상수의 전사(기하·알파)도
  정본과 대조하므로, 값이 갈리는 것과 값이 늘어나는 것을 한 자리에서 본다.
- 출하되는 그림자 토큰 중 하나가 음수 `spread` 를 갖게 된다 —
  `no_shipped_shadow_token_uses_negative_spread`(theme.rs 유닛 시험)가 잡는다. 그때는
  "미구현으로 둔다" 는 이 결정의 조항이 더 이상 유지되지 않는다는 뜻이므로, egui 밖
  렌더 경로를 함께 결정해야 한다.
  그 시험은 **손으로 적은 토큰 명부**를 도는데, 명부가 낡으면 새 토큰을 안 보고 초록이
  된다 — 그 갈래를 `shipped_shadow_token_roster_is_complete` 가 닫는다(명부를 theme.rs
  소스에서 다시 읽어 `pub const …: ShadowToken` 전부와 대조). 즉 채널이 성립하는 것은
  두 시험이 함께 있을 때다: 명부 대조가 새 토큰의 **등재**를 강제하고, 음수 잠금이 그
  등재된 토큰의 `spread` 를 본다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- scrim 이 엣지를 그리게 바뀌면(예: scrim 에 vignette·blur 경계가 들어오면) "scrim 이
  지운 대비를 그림자가 되돌린다" 는 이 결정의 근거가 사라진다. 재는 법: mocha 테마에서
  모달을 띄우고 그림자를 끈 스크린샷과 켠 스크린샷을 비교해, 보더 바깥 1~2px 의 명도
  차가 눈에 남는지 본다.
- 두 갈래 어디에도 안 들어가는 떠 있는 표면이 늘어나 "그림자 없음" 이 예외가 아니라
  다수가 되면, 세 번째 값이 필요하다는 신호다. 재는 법: `popup::defs::all_defs()` 의
  항목을 세 갈래로 분류해 세 번째 갈래의 비중을 센다.

## References

- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-type-appearance/src/theme.rs` 의
  `SHADOW_POPOVER` · `SHADOW_MODAL` · `ShadowToken::to_egui`,
  `crates/tasty-type-appearance/src/shadow_policy_guard.rs`,
  `crates/tasty-design-tokens/tests/shadow_parity.rs`(정본 ↔ 상수 전사 대조),
  `crates/tasty-egui-theme/src/lib.rs`(egui 기본 그림자 두 필드의 매핑)
- 운영 상태 서술: [`design/systems/theme.md`](../design/systems/theme.md) "떠 있는 표면의 그림자"
- 토큰 매핑: [`design/systems/design-token-mapping.md`](../design/systems/design-token-mapping.md)
- 수치를 문서 본문에 적지 않는 이유: [ADR-0139](0139-numbers-in-docs-are-classified-by-lineage-not-by-name.md)
