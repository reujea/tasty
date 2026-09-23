# ADR-0010: Plugin marketplace 는 보류 — 로컬 path install 유지

- **Status**: Deferred
- **Date**: 2026-06-17
- **Tags**: plugin, marketplace, registry, trust, distribution, deferred
- **Group**: plugin-system

## Context

현재 plugin 설치는 `tasty plugin install <path>`(로컬 디렉터리 복사 + 매니페스트 권한 자동 grant) + 수동 git clone 뿐이다(`src/app/plugin_glue/lifecycle.rs`). 번들 plugin 8 종은 first-party, 외부(third-party) plugin 0 개. marketplace 가 주는 *발견 가능성*·*설치 편의*·*신뢰성* 세 가치 모두 외부 plugin 0 시점에선 필요도가 낮고, 인프라 유지 비용만 선부담하는 음수 가치다.

## Decision

**0.x 동안 marketplace(registry/install-by-id/trust/signature)를 도입하지 않는다.** `tasty plugin install <path>` + 수동 clone 을 유지한다. 본 ADR 이 도입 시 평가할 비용 trade-off 와 순서의 기록이다.

## Consequences

- **얻은 것**: registry 서버·publisher 인증·서명 인프라 운영 부담 0. 기존 install 흐름(검증/복사/등록/enable)이 그대로 재사용 가능한 자산으로 남음.
- **잃은 것**: 외부 plugin 발견·설치 편의 없음(수동 clone 필요). 외부 생태계 형성 시 재평가 필요.
- **운영 비용**: 도입 시 호스트 측 추정 ~490 LOC + registry 서버 운영 — 0.x plugin 시스템 대비 10% 미만이라 *trigger 발동 후 점진 도입* 부담은 작다.

## Decision (도입 시 순서·형태)

trigger 발동 시 권고 순서: ① **git-tap** 정의(`install <id>` 가 git URL 추론 + clone) → ② `plugin update <id>`(version compare + atomic swap) → ③ **권한 grant prompt**(install 시 항상 prompt, auto-grant 폐지) → ④ **OS-level sandbox 강제**(marketplace 출처는 `os-strict`, [ADR-0009](0009-plugin-sandbox-deferred.md) §OS-level 전제) → ⑤ GH OAuth publisher → ⑥ JSON index server(외부 plugin 20+) → ⑦ signature(checksum→minisign→Sigstore 점진) → ⑧ revocation(opt-in online check). registry 형식은 **git-tap(비용 0) → 단일 JSON index** 점진.

## Alternatives Considered

- **단일 JSON index 즉시** — 검색/필터 쉬우나 인프라 운영 선부담. 외부 plugin 20+ 시점으로 미룸.
- **DNS/IPFS 분산** — 검열 저항은 tasty 가치 아님. 외부 의존성 비용 초과.
- **수동 review(VS Code 식)** — paid publisher + 운영 인력 모델, single-org 로 재현 불가. 자동 lint + 권한 risk score(C1+C2)만.
- **현 상태 유지(채택)** — 외부 plugin 0 시점 음수 가치.

## Reconsideration Triggers

- 첫 외부 plugin 출시 / 외부 plugin 5+ 자생 / 수동 설치·업데이트 불편 반복 보고
- 첫 외부 plugin 출시 후 6개월 자동 재검토(사례 부재여도 시간 기준 1회)
- **핵심**: marketplace 도입 = sandbox 강제 + auto-grant 폐지 + grant prompt 신설 = **묶음**. [ADR-0009](0009-plugin-sandbox-deferred.md) 와 상호 trigger.

## References

- [dev-guide/plugin-packaging 생태계 정책](../dev-guide/plugin-packaging.md#생태계-정책--자동-upgrade--호환성-분류) §정책(배포·신뢰) · [plugin-packaging](../dev-guide/plugin-packaging.md)(서명) · [plugin-permissions](../dev-guide/plugin-permissions.md)
- [ADR-0009](0009-plugin-sandbox-deferred.md) — sandbox 보류(묶음)
- 코드: `src/app/plugin_glue/lifecycle.rs`(`plugin_install`) · `crates/tasty-plugin-manifest/src/validators.rs`(plugin id)
