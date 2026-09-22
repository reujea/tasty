# ADR-0015: 원격 접속 프로필 = 범용 typed 레지스트리, attach 는 소비자

- **Status**: Superseded by [ADR-0032](0032-remote-attach-two-layer-split.md) (부분) — 범용 레지스트리 봉투(kind + fields)는 유효하나, "attach = ssh kind 소비" 는 ssh(연결) / tasty-attach(attach) 2-레이어로 대체됨.
- **Date**: 2026-06-19
- **Tags**: remote, profile, registry, attach, ssh, smb, extensibility, plugin, ubiquitous-language
- **Group**: remote-attach

## Context

기존 "SSH 프로필"(`tasty-ssh-profiles`)은 attach 전용 터널 디스크립터였다 — 필드 절반(`remote_tasty`/`port_mode`/`shell`/`detect_failed`)이 "원격 tasty 의 IPC 포트를 발견한다"는 attach 전제에 묶여 있어, 순수 ssh 셸이나 다른 프로토콜(smb/http)을 표현할 수 없었다. 사용자는 ① 동일 주소로 복수 프로필, ② attach 외 소비자(예: browser 플러그인이 http 프로필을 읽음)가 자기 타입의 프로필을 소비하는 모델을 원했다.

## Decision

프로필을 **타입 태그(열린 string) + 자유 필드 봉투**로 일반화하고 **액션을 프로필에 매달지 않는다**(해석은 소비자 몫). `RemoteProfile { name, label, kind, passkey_ref, fields: BTreeMap<String, FieldValue> }`, `FieldValue = Str | List`(TOML 스칼라/배열 네이티브). 저장은 `~/.tasty/remote-profiles.toml`.

- **known 타입 = core 내장(ssh/smb) ∪ 설치 플러그인이 manifest 로 선언한 타입**(런타임 집합). 미등록 타입(오타 `snb`/임의값)도 **등록을 막지 않고** UI 노란 배지 + CLI/IPC 비고로만 경고한다.
- **attach 는 ssh kind 를 소비하는 한 소비자**로 재배치된다 — "주소 저장"과 "attach"가 분리됐다. attach 가 ssh 만 받는 것은 **격리 원칙이 아니라 기능적 필연**(ssh 터널이라 http 로는 못 뚫음)이다.
- **kind 스코프 제약은 플러그인 한정**(manifest 선언 타입만 접근 = 권한 경계). 네이티브 기능은 전체 레지스트리를 자유 조회하고 필요할 때만 kind 를 검증한다.
- 알려진 타입(ssh)은 `SshView` 로 typed 접근, 그 외는 raw fields. 모델은 deps-free.

## Consequences

- **얻은 것**: 다중 프로토콜 / 동일 주소 복수 프로필 / 플러그인 확장(http 등). attach 와 주소 저장의 분리. 자격증명을 [`0016`](0016-passkey-store-path-convergence.md) 으로 외부화해 프로필 TOML 에 비밀 0.
- **잃은 것**: 타입별 강제 스키마가 없다 — 소비 시점 검증(`SshView` 가 누락/형식오류를 기본값/None+warn 으로 흡수)으로 대체.
- **운영 비용**: `SshView` typed view 유지, IPC/CLI alias(`tool.ssh.*`/`ssh.profile.*` → `remote.profile.*`) 한시 유지(다음 minor tag 직전 제거).

## Alternatives Considered

- **닫힌 enum(kind = Ssh|Smb|Http 고정)**: 기각. 새 타입마다 core 수정 필요 + 플러그인이 임의 타입을 정의할 수 없어 "플러그인이 http 프로필 소비" 시나리오가 깨진다.
- **프로필에 액션(attach/mount)을 부착**: 기각. 소비자와 결합되어 한 프로필이 한 용도에 묶인다. 액션은 소비자가 해석한다.

## Reconsideration Triggers

- 미등록 타입 남용으로 데이터 오염이 실제 문제화하거나, 타입별 강제 스키마가 필요해지면 재검토한다.

## References

- [`0016-passkey-store-path-convergence.md`](0016-passkey-store-path-convergence.md) — 자격증명 외부화
- [`0007-attach-targets-remote.md`](0007-attach-targets-remote.md) — attach 는 원격 대상(소비자)
- [`concepts/ubiquitous-language.md`](../concepts/ubiquitous-language.md) — Remote profile / Passkey 용어
- 코드: `crates/tasty-remote-profiles/`(모델·마이그레이션) · `src/adapters/ipc/handler/remote_profile.rs` · `src/adapters/ui/popup/remote_tool.rs` · `crates/tasty-cli/src/commands/remote_profile.rs`(clap 선언, 구 `ssh_profile.rs` — ADR-0032 개명) · `crates/tasty-cli/src/local/remote_profile.rs`(실행)
