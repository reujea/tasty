# ADR-0016: Passkey 저장소 — path 수렴 · 파일권한 위임 · 참조 모델

- **Status**: Accepted
- **Date**: 2026-06-19
- **Tags**: passkey, secret, security, file-permission, trust-boundary, remote-profile
- **Group**: permission-security

## Context

원격 접속 프로필([`0015`](0015-remote-profiles-typed-registry.md))에 자격증명이 필요하나 tasty 는 비밀을 "보관"하지 않는다([`0005`](0005-memory-secret-not-a-vault.md) — vault 아님). 사용자는 자격증명을 **별도 named 저장소**로 두고 프로필이 이름으로 참조하길(여러 프로필이 한 키 공유), inline 입력 편의도 원했다. 한편 SMB/HTTP 같은 비-ssh 타입은 ssh-agent 위임이 없어 키 위치를 명시해야 한다.

## Decision

Passkey 를 별도 저장소(`~/.tasty/passkeys.toml`, 0600)로 분리하고 **모든 자격증명을 at-rest 에서 파일 경로로 수렴**한다. `Passkey { name, kind, path }`, `kind = path | inline`(열린 string).

- **`path`** = 사용자 소유 기존 파일 참조(tasty 는 경로만, 수명 미관여).
- **`inline`** = 사용자 입력 문자열 → tasty 가 `~/.tasty/passkeys/<name>`(0600, 디렉토리 0700)로 materialize 해 소유(passkey 삭제 시 파일도 삭제). → **toml 엔 비밀 값이 없다**(경로뿐).
- **이름 정책 2종**: 대화형 등록은 영숫자/`-`/`_` 화이트리스트 **거부**(name 이 파일명이라 path traversal 차단), 마이그레이션 자동생성은 거부 불가이므로 비허용 문자 **치환**.
- **보호는 암호화가 아니라 OS 파일권한 위임**([`0004`](0004-ipc-transport-tcp.md)/[`0005`](0005-memory-secret-not-a-vault.md) 와 일관). 같은 OS 유저의 FS read 는 신뢰모델상 범위 밖.
- **값 마스킹**: IPC/CLI 는 passkey 값을 마스킹하고 파일 내용을 영구 미반환. 내용 열람은 **로컬 GUI + 설치 승인 플러그인의 선언 타입 한정**. 후자도 보안 경계가 아니라 설치 시점 신뢰 기반 편의 경로다(플러그인은 sandbox 없이 파일 직접 read 가능 — [`0009`](0009-plugin-sandbox-deferred.md)).

## Consequences

- **얻은 것**: 프로필/passkey toml 에 비밀 0(표시·백업·로그 유출표면 축소). 소비자는 항상 단일 path 로 resolve(분기 없음). 수명 명확(inline 관리 파일은 passkey 와 함께 삭제). ssh `identity_file` 모델과 정합.
- **잃은 것**: 로컬 同 OS 유저 agent 의 FS read 는 막지 못한다 — 정직하게 보장하지 않는다.
- **운영 비용**: inline 파일 라이프사이클 관리(materialize/삭제/이름 sanitize).

## Alternatives Considered

- **toml 에 inline 평문 저장**: 기각. 비밀이 config 파일에 섞여 표시·백업·로그 유출표면이 커진다. 파일로 빼면 보안 천장은 같되 유출표면이 준다.
- **keyring / 마스터 패스프레이즈 암호화**: 보류. keyring 키는 同 OS 유저가 우회 가능([`0005`](0005-memory-secret-not-a-vault.md) 가 버린 설계)이라 가짜 보안이고, 진짜 보호(마스터 패스프레이즈 → 메모리 전용 키)는 headless 자동 attach 와 충돌하며 크로스플랫폼 비용이 크다.

## Reconsideration Triggers

- 플러그인 sandbox([`0009`](0009-plugin-sandbox-deferred.md)) 도입 / 공유 머신·multi-tenant([`0004`](0004-ipc-transport-tcp.md) 트리거) / inline 비밀의 진짜 at-rest 보호 요구가 생기면 마스터 패스프레이즈 모델을 재검토한다.

## References

- [`0015-remote-profiles-typed-registry.md`](0015-remote-profiles-typed-registry.md) — 프로필이 passkey 를 이름으로 참조
- [`0005-memory-secret-not-a-vault.md`](0005-memory-secret-not-a-vault.md) · [`0004-ipc-transport-tcp.md`](0004-ipc-transport-tcp.md) — 신뢰 모델
- 코드: `crates/tasty-remote-profiles/src/passkey.rs` · `src/adapters/ipc/handler/passkey.rs` · `src/adapters/ui/popup/remote_tool.rs`(Reveal)
