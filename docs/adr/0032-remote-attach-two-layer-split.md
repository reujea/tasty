# ADR-0032: 원격 프로필을 ssh(연결) / tasty-attach(attach) 2-레이어로 분리

- **Status**: Accepted
- **Date**: 2026-07-01
- **Tags**: remote, profile, attach, ssh, two-layer, ref, port-file, cli
- **Group**: remote-attach

## Context

[ADR-0015](0015-remote-profiles-typed-registry.md) 는 원격 프로필을 타입 태그 + 자유 필드 봉투로 일반화하고 attach 를 **ssh kind 의 소비자**로 배치했다. 그 결과 하나의 `ssh` 프로필이 **순수 연결 정보**(host/user/port/identity/extra_options/shell)와 **attach 전용 정보**(`remote_tasty`/`port_mode`)를 함께 들고 있었다.

문제:
- 한 원격 호스트에 대해 "그냥 ssh 접속" 과 "tasty attach" 가 같은 레코드에 뒤섞였다. ssh 접속만 원하는 사용자에게 attach 필드는 잡음이고, 반대로 한 ssh 호스트에 여러 attach 구성(다른 원격 tasty 경로/포트 파일)을 두기 어려웠다.
- 원격에 전역 `tasty` 명령이나 표준 데이터 디렉터리가 없으면 attach 가 불가능했다 — port 파일 위치가 관례(`~/.tasty/tasty.port`)로 하드코딩되어 있었다.
- 사용자는 "원격 접속 프로필에 **tasty-attach 라는 타입**을 두고, ssh 정보를 **참조하거나 인라인**으로 갖게 하라" 고 요구했다. 하위호환은 불필요(기존 필드는 버림)로 확정했다.

## Decision

프로필을 두 kind 로 분리한다. 둘 다 같은 레지스트리(`~/.tasty/remote-profiles.toml`, ADR-0015 봉투)를 쓰되 역할이 갈린다.

- **`ssh` kind** = 순수 연결 정보만: `host`/`user`/`port`/`extra_options`/`passkey_ref` + `shell`/`detect_failed`(셸 감지 상태). `remote_tasty`/`port_mode` 를 **제거**한다. `SshView` 는 이 필드만 노출.
- **`tasty-attach` kind**(신규) = attach 스펙. 연결 정보는 **참조** 또는 **인라인**:
  - `ssh_ref = <ssh 프로필 name>` — 참조. resolve 시점에 name 으로 **매번 재로드**해 참조 ssh 프로필의 변경을 그대로 따라간다(라이브 팔로우).
  - 인라인 — `ssh_ref` 가 없으면 자기 fields 에 ssh 연결 정보를 직접 보유(`SshView` 로직 재사용).
  - attach 전용: `remote_tasty`(원격 tasty 바이너리 경로, 기본 `tasty`), `port_mode`(포트 발견 모드, 기본 `auto`), **`port_file`**(원격 port 파일의 명시 경로, 신규). `AttachView` 로 typed 접근.
- **attach 는 tasty-attach kind 를 소비한다.** `--profile`/`tool attach` 대상은 이제 tasty-attach. resolve 는 ref/inline 을 결선하고, dangling ref·비활성 ssh 소스는 명확한 에러.
- **port 발견 우선순위**: `port_file`(명시) > `port_mode` 체인. `port_file` 이 있으면 그 경로를 직접 읽어(`cat`→`type` 순, 셸 미상 대비) 관례 경로를 건너뛴다 → 전역 `tasty` 나 표준 디렉터리 없이도 attach.
- **detect-split**: ssh detect = 셸 **도달성** 판정(`shell`/`detect_failed` 만 갱신, port_mode 를 ssh 에 저장하지 않음). port_mode 도출은 attach 레이어 — attach 가 명시하지 않으면(`auto`) ssh 소스의 `shell` 에서 `shell_to_port_mode` 로 도출한다.
- **하위호환 제거**: 구 `ssh-profiles.toml → remote-profiles.toml` 마이그레이션 모듈·부팅 호출부를 삭제한다. 기존 프로필에 남은 `remote_tasty`/`port_mode` 는 열린 스키마라 무시되며 크래시하지 않는다(사용자가 tasty-attach 프로필을 새로 생성).

CLI 도 역할에 맞춰 재편한다: `tool ssh <profile>`(단순 ssh 접속 실행) / `tool remote-profile`(ssh+tasty-attach 통합 CRUD, `add-ssh`/`add-attach`) / `tool attach`(attach 실행, `--list` 는 tasty-attach 목록).

## Consequences

- **얻은 것**: 연결과 attach 의 관심사 분리. 한 ssh 호스트에 여러 attach 구성(ref 재사용). `port_file`/`remote_tasty` 명시로 전역 tasty·표준 디렉터리 없는 원격도 attach. ref 라이브 팔로우로 ssh 정보 변경이 attach 에 자동 반영.
- **잃은 것**: attach 하려면 프로필을 2개(ssh + tasty-attach) 만들어야 할 수 있다(ref 방식). 기존 4개 프로필의 attach 필드는 버려져 재생성 필요(하위호환 포기의 대가).
- **운영 비용 / 유지 부담**: `SshView`/`AttachView` 두 typed view 유지. GUI(remote_tool 팝업)의 3탭(Remote profiles · Attach · Passkeys) 반영은 디자인 수령 후 별도 진행(gallery-first). 마이그레이션 제거로 구 `ssh-profiles.toml` 자동 이관은 더 이상 없다.

## Alternatives Considered

- **attach 를 별도 파일 `~/.tasty/attaches.toml` 에 저장**(수령 디자인 제안): 기각. 사용자가 "원격 접속 프로필에 tasty-attach 타입" 으로 지시 → 동일 레지스트리의 새 kind 로 간다. 저장 파일 분리는 향후 필요 시 재검토.
- **기존 remote_tasty 를 인라인 attach 로 옮기는 마이그레이션 제공**(수령 디자인 제안): 기각. 사용자가 "migration 제거, 기존 필드 버림" 확정.
- **ssh 프로필에 attach 필드 유지(ADR-0015 현상)**: 기각. 연결/attach 혼재가 위 문제의 근원.
- **참조를 name 이 아니라 스냅샷 복사**: 기각. 라이브 팔로우(참조 ssh 변경이 attach 에 반영) 요구를 못 지킨다.

## Reconsideration Triggers

- attach 프로필 수가 ssh 프로필과 사실상 1:1 로 항상 함께 생성돼 분리 이득이 없어지면 재검토.
- 저장 파일 분리(`attaches.toml`)가 실제로 필요해지면(예: 외부 도구가 attach 만 소비) 재검토.
- `port_file` 직접 읽기(`cat`/`type` 순차 시도)가 특정 원격 셸에서 오작동하면 셸 명시 분기 도입을 재검토.

## References

- [`0015-remote-profiles-typed-registry.md`](0015-remote-profiles-typed-registry.md) — 본 ADR 이 supersede 하는 단일 kind 모델(범용 레지스트리 봉투 자체는 유지)
- [`0007-attach-targets-remote.md`](0007-attach-targets-remote.md) — attach 는 원격 대상(소비자)
- [`0016-passkey-store-path-convergence.md`](0016-passkey-store-path-convergence.md) — 자격증명 외부화
- [`../features/remote-profiles/index.md`](../features/remote-profiles/index.md) · [`../features/remote-attach/index.md`](../features/remote-attach/index.md)
- 코드: `crates/tasty-remote-profiles/src/profile.rs`(`SshView`/`AttachView`) · `crates/tasty-ssh/src/lib.rs`(resolve/discover) · `crates/tasty-cli/src/commands/remote_profile.rs`(clap 선언) · `crates/tasty-cli/src/local/remote_profile.rs`(실행) · `src/adapters/ipc/handler/remote_profile.rs`
