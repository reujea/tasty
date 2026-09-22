# ADR-0051: release plugin 서명키를 영구 신뢰 루트가 아닌 매 빌드 로컬 자동생성으로 전환

- **Status**: Accepted
- **Date**: 2026-07-14
- **Tags**: plugin-signing, release-ci, security, ed25519, trust-store, self-hosted-runner
- **Group**: build-release

## Context

builtin plugin 매니페스트(`tasty-plugin.toml`)는 Ed25519 서명으로 confused-deputy(권한 변조)를 막는다(`crates/tasty-host-plugin/src/bundle_sig.rs::TRUSTED_PUBKEYS`). 원래 설계는 "release" 슬롯을 운영자가 1회 발급하는 영구 신뢰 루트로 두는 것이었다 — `tasty-host-plugin` 의 `keys/release-pubkey.bin` 을 git에 영구 커밋하고, 대응 개인키를 GitHub Secret `TASTY_RELEASE_SIGN_KEY` 로 등록해 `.github/workflows/release.yml` 이 매 release 빌드마다 이 secret 을 디코딩해 서명하는 흐름이었다.

이 절차는 실제로 한 번도 완성되지 않았다 — `release-pubkey.bin` 이 계속 all-zero placeholder 로 커밋돼 있었고, `TASTY_RELEASE_SIGN_KEY` secret 도 등록된 적이 없었다(`gh secret list` 확인 결과 0개). 그 결과 release CI 는 tag push 때마다 "Decode release signing key" step 에서 곧바로 fail 했다 — 오랫동안 release 파이프라인이 아예 동작하지 않은 원인 중 하나.

## Decision

release plugin 서명키를 **영구 보관하지 않고, 빌드마다 로컬에서 자동생성**한다. 이미 존재하던 dev key 메커니즘(`scripts/gen-dev-key.sh`, `scripts/ensure-sign-key.sh`)을 release 빌드에도 그대로 적용한다 — `~/.tasty-keys/release.pem` 이 없으면(4대 self-hosted 러너 전부 없음) 그 자리에서 새 Ed25519 keypair 를 만들어 서명하고, 그 개인키는 해당 머신에만 남긴다. 그 `keys/release-pubkey.bin` 은 git 추적을 중단하고(`dev-pubkey.bin` 과 동일하게 `.gitignore`), 항상 all-zero placeholder 로 남는 걸 정상 상태로 취급한다. 실질적인 서명 검증은 dev 슬롯이 담당한다.

이 결정의 근거는 `crates/tasty-host-plugin/src/builtin.rs::install_builtins_if_needed()` 확인 결과다 — builtin plugin 은 항상 그 앱 바이너리와 **같은 설치 번들 안에서 로컬로 복사**되며, 앱 버전과 독립적으로 원격에서 개별 업데이트되는 경로가 없다. 따라서 "구버전 바이너리가 신버전 키로 서명된 plugin 을 검증해야 하는" 상황 자체가 존재하지 않는다 — 매 릴리스가 자기 안에서 완결되는 신뢰 단위이므로, 릴리스마다 다른 키를 써도 안전하다.

## Consequences

- **얻은 것**: GitHub Secret 등록·4대 self-hosted 러너에 개인키 수동 배치라는 운영 부담이 완전히 사라진다. `.github/workflows/release.yml` 이 secret 유무와 무관하게 항상 동작한다. 키 유출 리스크가 그 빌드 1 회로 국한된다(영구 키가 없으니 유출될 영구 자산도 없음).
- **잃은 것**: "이 서명이 특정 발급자가 발급했다"는 장기 정체성 보증이 없다 — 서로 다른 release 빌드(v0.9.4 vs v0.9.5, 또는 macOS 빌드 vs Windows 빌드)가 서로 다른 키로 서명된다. 이 프로젝트가 실제로 필요로 하는 보증(confused-deputy 방지, 매니페스트 변조 탐지)에는 영향 없다 — 애초에 발급자 신원 증명이 목표가 아니었다.
- **운영 비용 / 유지 부담**: 없음. 기존 dev key 자동생성 메커니즘을 그대로 재사용하므로 신규 스크립트나 절차가 추가되지 않는다.

## Alternatives Considered

- **A: 원래 계획대로 완성 (GitHub Secret + 4대 러너 수동 배치)** — 매 릴리스 수동 키 배포 부담이 있고, 유출 시 그 키로 서명된 모든 과거·미래 release 가 전부 영향받는다. 여기까지 완성해도 얻는 보증(발급자 신원)이 이 프로젝트가 실제로 필요로 하는 것보다 크다.
- **B: 키 회전 정책 (`TRUSTED_PUBKEYS` 에 새 키 prepend, 옛 키 2 minor 유지)** — 원래 `docs/dev-guide/plugin-packaging.md` 에 문서화돼 있었으나, 영구 신뢰 루트가 존재한다는 전제 하의 정책이라 전제 자체가 없어지며 함께 폐기.
- **C: 채택안 (매 빌드 로컬 자동생성)** — 위 Decision.

## Reconsideration Triggers

- builtin plugin 이 앱 버전과 독립적으로 원격에서 배포/업데이트되는 마켓플레이스 모델이 도입되면 — 그 경우 서로 다른 시점에 배포된 plugin 과 이미 설치된 구버전 바이너리 사이의 신뢰 검증이 필요해지므로, 영구 신뢰 루트를 다시 도입해야 한다.
- 외부 서드파티가 tasty 의 release 서명을 신뢰의 근거로 삼는 통합(예: 서명 검증 기반 자동 업데이트 채널)이 생기면.

## References

- [plugin-packaging.md](../dev-guide/plugin-packaging.md) "서명" 섹션
- [release.md](../dev-guide/release.md)
