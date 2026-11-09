# macOS 가상머신 시험 — Tart 로 전체 시험 돌리기

호스트의 도구·설정·포커스와 섞이지 않는 macOS 에서 셸 게이트 · `cargo test --workspace` · GUI 시험을
돌린다. 호스트에 깔린 도구·설정과 앞 포커스가 결과를 흔들지 않게 매번 같은 macOS 에서 잰다.

진입점은 `scripts/macos-vm-test.sh` 하나다.

| 명령 | 하는 일 |
|---|---|
| `prepare` | 이미지를 받아 가상머신을 만들고(새로 만들면 디스크를 `TASTY_VM_DISK_GB` 로 늘린다) rustup · shellcheck · tokei · bash 5 · mingw-w64 를 깐다. 화면 기록 권한을 넣고, 입력 권한을 확인한다. 다시 돌려도 된다 |
| `test [all·unit·gates·gui·windows·pkg]` | 필요하면 가상머신의 빌드 캐시를 지우고 디스크 여유를 잰 뒤(§3) 작업 트리를 옮겨 고른 범위를 돌린다. 한 단계가 실패해도 다음 단계로 간다. `windows`(§2.2) · `pkg`(바뀐 패키지만, 아래) 는 `all` 에 안 들고 따로 부른다 |
| `shell` | 가상머신에 들어간다(ssh, `exec` 길이면 `tart exec`) |
| `stop` / `destroy` | 끈다(디스크·빌드 캐시는 남는다) / 지운다 |

종료 코드는 0 전부 통과 · 1 시험 실패 · 2 준비가 안 됐다(도구 · 가상머신 · 권한).

## 1. 설치 (한 번)

Tart 의 Homebrew tap(`cirruslabs/cli`)은 2026-10-15 에 지금 Homebrew 와 안 맞아 설치가 실패했다
(`depends_on macos:` 오류). GitHub 릴리스에서 받는다:

```bash
gh release download -R openai/tart --pattern 'tart*'      # tart.tar.gz 와 체크섬
shasum -a 256 tart.tar.gz                                  # 체크섬 파일과 견준다
tar xzf tart.tar.gz
spctl -a -vv tart.app                                      # "Notarized Developer ID" 여야 한다
mv tart.app ~/Applications/
ln -s ~/Applications/tart.app/Contents/MacOS/tart ~/.local/bin/tart   # PATH 위의 아무 곳
brew install sshpass
```

가상머신의 sshd 는 맞는 비밀번호를 가끔 거절한다(실측 2026-10-21: 연달아 20 번 붙으면 2–4 번, sshpass 의 비밀번호
전달 방식과 무관). 스크립트는 인증 거부(sshpass 종료 코드 5 — 명령은 돌지 않았다)일 때만 게스트 명령을 3 번까지 다시 하고,
옮기기(rsync)는 실패하면 3 번까지 다시 한다. 명령 자신의 실패는 다시 하지 않는다.

## 2. 쓰는 법

```bash
bash scripts/macos-vm-test.sh prepare     # 처음 한 번. 이미지 31 GB 를 받는다
bash scripts/macos-vm-test.sh test        # all — gates → unit → gui
bash scripts/macos-vm-test.sh test unit   # 범위 하나만
```

로그는 `target/macos-vm/<단계>.log` 에 남는다(`TASTY_VM_OUT` 로 옮길 수 있다). 가상머신 이름과
이미지는 `TASTY_VM_NAME` · `TASTY_VM_IMAGE` 로 덮는다.

구간마다 걸린 시간은 같은 자리의 `timings.tsv` 에 `phase` · `seconds` · `rc` 세 열로 남고, 끝에 분·초
요약이 찍힌다. 구간은 `boot`(가상머신 기동) · `clean`(캐시 판정·정리와 디스크 여유) · `sync`(작업 트리
옮기기) 와 실행한 단계들이다. `unit` 범위는 `unit-build`(`cargo build --workspace`) 와 `unit`(`cargo test`)
두 단계로 나뉘어 빌드 시간과 시험 시간을 따로 보며, 빌드가 실패하면 시험 단계를 건너뛰고 `unit` 도
실패로 센다. 한 구간이 끝날 때마다 한 줄씩 더하므로 도중에 멈춘 실행도 끝난 구간까지는 남는다. 실행을
새로 시작하면 파일을 비운다 — 실행끼리 비교하려면 `TASTY_VM_OUT` 을 실행마다 달리 준다.

`pkg` 범위는 PR 브랜치처럼 일부만 바꾼 트리를 빨리 잴 때 쓴다. 전체 `unit` 은 PR 들을 모은 통합 브랜치에서
돌린다. 먼저 `unit-build`(`cargo build --workspace`) 로 워크스페이스를 다 지어 바꾼 API 를 쓰는 다른
패키지의 컴파일 깨짐을 잡고, 시험은 고른 패키지만 돌린다. 다른 패키지의 시험이 바뀐 동작에 깨지는지는
여기서 안 보인다 — 통합 브랜치의 `unit` 이 본다. 패키지는 `TASTY_VM_PKG` 로 직접 주거나, 그것이 없으면
`TASTY_VM_BASE` 와의 분기점 뒤로 바뀐 파일(커밋 안 한 것 포함)에서 고른다.

| 바뀐 자리 | 고르는 패키지 |
|---|---|
| `crates/<dir>/` | 그 `Cargo.toml` 의 `name` — workspace `exclude` 의 crate 는 안 고른다 |
| `src/` · `tests/` · `lang/` · `build.rs` | `tasty` |
| `docs/` · `site/` · `*.md` | `tasty-doc-guards` |
| 루트 `Cargo.toml` · `Cargo.lock` · `scripts/` 등 | 안 고른다 — `--locked` 빌드가 본다 |

고른 패키지가 없으면 빌드만 하고 통과한다. 루트 패키지 `tasty` 의 시험이 전체 시험 실행 시간의 대부분을
차지하므로, 시간이 크게 주는 것은 루트를 안 건드린 브랜치다.

```bash
TASTY_VM_BASE=origin/main bash scripts/macos-vm-test.sh test pkg
```

| 변수 | 값 | 기본 |
|---|---|---|
| `TASTY_VM_CLEAN` | `auto` §3 의 문턱을 넘을 때만 빌드 캐시를 지운다 · `1` 늘 지운다 · `0` 늘 남긴다 | `auto` |
| `TASTY_VM_CLEAN_MAX_TARGET_GB` | `auto` 에서 가상머신의 `target` 이 이보다 크면(GB) 지운다 | `24` |
| `TASTY_VM_CLEAN_DIRTY_PCT` | `auto` 에서 지난 빌드 뒤 바뀐 크레이트가 전체의 이 비율(%)을 넘으면 지운다 | `50` |
| `TASTY_VM_DISK_GB` | `prepare` 가 새로 만든 가상머신의 디스크 크기(GB). 이미지는 50 GB 다 | `100` |
| `TASTY_VM_MIN_FREE_GB` | 처음부터 빌드할 때 남아야 하는 디스크 여유(GB). 캐시를 남기면 그 크기만큼 덜 요구한다. 모자라면 옮기기 전에 rc 2 로 멈춘다 | `12` |
| `TASTY_VM_KEEP_MIN_FREE_GB` | 캐시를 남길 때 요구하는 디스크 여유의 하한(GB) | `3` |
| `TASTY_VM_PKG` | `pkg` 범위에서 시험할 패키지(공백으로 나눈다) | 없음 |
| `TASTY_VM_BASE` | `pkg` 범위에서 `TASTY_VM_PKG` 가 없을 때 바뀐 파일을 가를 기준 커밋 | 없음 |

실측 2026-10-15(M5 Pro 호스트, macOS 26.6.2 게스트): `prepare` 30 초, 첫 `test unit` 529 초(빌드
포함 — 지금은 `timings.tsv` 의 `unit-build` · `unit` 으로 나뉜다), `test gui` 약 160 초. 두 번째부터는 가상머신 안의 빌드 캐시를 쓴다.

### 2.1 네트워크 — Cloudflare WARP 를 쓰는 호스트

기본 네트워크(`nat`)는 호스트에 DNS 프록시(mDNSResponder, `192.168.64.1:53`)를 띄운다. Cloudflare WARP 의
DNS 프록시 모드는 그것을 다른 프로세스의 DNS 가로채기로 보고 연결을 거부한다(`CF_DNS_PROXY_FAILURE`) —
가상머신이 떠 있는 동안 WARP 가 안 붙는다. 그런 호스트에서는 브리지로 띄운다:

```bash
TASTY_VM_NET=bridged:en5 bash scripts/macos-vm-test.sh test     # en5 = 호스트가 랜에 붙은 인터페이스
```

| 변수 | 값 | 기본 |
|---|---|---|
| `TASTY_VM_NET` | `nat` · `bridged:<인터페이스>` | `nat` |
| `TASTY_VM_TRANSPORT` | `ssh` · `exec` | `nat` 이면 `ssh`, 브리지면 `exec` |

- 인터페이스 이름은 `tart run --net-bridged=list <가상머신>` 이 보여 준다(쓸 수 없는 이름을 주면 그 목록을 찍는다).
- **브리지된 가상머신은 그 랜의 DHCP 에서 주소를 받는다.** 호스트 주소가 그 대역 밖이면 호스트에서 ssh 가
  안 닿는다. 그래서 브리지의 기본 길은 `exec` — Tart 게스트 에이전트(`tart exec`, 네트워크를 안 거친다)로
  명령을 보내고 `rsync -e 'tart exec -i'` 로 트리를 옮긴다.
- 이미 떠 있는 가상머신은 다시 띄우지 않는다 — 네트워크를 바꾸려면 먼저 `stop` 한다. `exec` 길은 어느
  네트워크로 떠 있든 닿는다.
- 실측 2026-10-26(호스트 en5 고정 주소 `192.0.2.10`, WARP 켬): `nat` 으로 뜨면 WARP 가 `Unable`, 끄면 바로
  `Connected`. `bridged:en5` 로 뜨면 브리지에 주소가 없어 DNS 프록시가 안 뜨고 WARP 는 내내 `Connected`
  였다. 가상머신은 `192.0.2.11` 를 받아 호스트에서 ping·ssh 가 안 닿았고(다른 대역), 가상머신 안에서
  인터넷은 닿았다. 그 상태로 `test gates` 가 `exec` 길로 통과했고, `test gui` 는 33 중 32 였다(§4 의 알려진
  실패 하나). 두 시험 동안 WARP 는 내내 `Connected` 였다.

### 2.2 Windows 대상 컴파일 — `test windows`

```bash
TASTY_VM_NET=bridged:en5 bash scripts/macos-vm-test.sh test windows
```

가상머신에서 `x86_64-pc-windows-gnu` 대상으로 `cargo check --workspace --all-targets --keep-going` 과
`cargo clippy … -- -D clippy::correctness` 를 돈다. **실행은 안 한다** — 컴파일 정합성만 잰다. 그 대상의
C 를 짓는 build.rs 가 mingw-w64 컴파일러를 부르므로 `prepare` 가 그것을 깐다(없으면 rc 2 로 멈추고
`prepare` 를 가리킨다). Windows 대상 의존을 따로 지어 시간과 디스크가 두 배 가까이 들어 `all` 에 안 넣는다.

## 3. 옮기는 것과 안 옮기는 것

- 스크립트는 **실행해서** 쓴다(`bash scripts/macos-vm-test.sh …`). 다른 셸에서 source 하면 레포 위치를
  못 구한다 — 그때는 옮기기 전에 rc 2 로 멈춘다.
- **디스크는 100 GB 로 늘려 쓴다.** 이미지 디스크는 50 GB 라 빈 캐시 기준 여유가 12 GB 남짓이고, 빌드 캐시
  하나(10 GB 남짓)로 거의 찬다(실측 2026-10-27: `~/tasty/target` 11 GB, 여유 2.2 GB). `prepare` 가 새로 만든
  가상머신은 첫 부팅 전에 `TASTY_VM_DISK_GB` 로 늘린다. 이미 있는 가상머신은 손으로 늘린다 — 디스크는 끈
  채로만 바꾼다(tart 디스크는 sparse 라 호스트에서는 쓴 만큼만 차지한다).

  ```bash
  tart stop tasty-macos && tart set tasty-macos --disk-size 100
  bash scripts/macos-vm-test.sh shell   # 부팅 뒤 안에서:
  sudo diskutil apfs resizeContainer disk0s2 0   # 부팅이 이미 넓혔으면 -69743(크기가 같다)
  ```

  실측 2026-11-02: 100 GB 로 늘린 뒤 빈 캐시 기준 여유 59 GB.
- **빌드 캐시는 넘칠 때만 지운다.** 모자란 채로 빌드하면 수십 분 뒤 rustc 가 디스크 부족으로 죽고, 그
  실패는 시험 실패처럼 보인다. 반대로 늘 지우면 외부 의존까지 새로 지어 `test unit` 이 늘 첫 실행
  시간이 든다(§2 의 실측). 그래서 `test` 는 옮기기 **전에** 아래 중 하나라도 맞으면 `~/tasty` 에서
  `cargo clean` 을 하고 단계 로그를 지운다(기본 `TASTY_VM_CLEAN=auto`). 지운 까닭은 진행 줄에 남는다.
  1. 디스크 여유가 필요한 여유보다 적다. 필요한 여유는 `TASTY_VM_MIN_FREE_GB` 에서 남길 캐시 크기를 뺀
     값이고 `TASTY_VM_KEEP_MIN_FREE_GB` 아래로는 안 내린다 — 남긴 캐시는 빌드 몫을 이미 차지하고, 다시 지을
     때는 바뀐 크레이트의 산출물만 덮어쓴다. 고정 문턱이면 `test unit` 한 번 뒤(캐시 10 GB 남짓 · 여유
     2 GB 남짓, 실측 2026-11-02) 늘 지우게 된다.
  2. `target` 이 `TASTY_VM_CLEAN_MAX_TARGET_GB` 보다 크다 — cargo 는 옛 산출물을 안 거둔다.
  3. 지난 빌드의 커밋 표지(`target/.tasty-vm-built-rev`)가 없거나 이 저장소가 모르는 커밋이다. 표지는
     `target` 안에 있어 누가 `cargo clean` 을 하든 같이 사라진다 — 다른 lane 이 같은 게스트 트리를 쓴 뒤의
     출처 모르는 캐시를 같은 트리로 읽지 않는다.
  4. 그 커밋 뒤 작업 공간 설정(루트 `Cargo.toml` · `Cargo.lock` · 툴체인)이 바뀌었다.
  5. 그 커밋 뒤 바뀐 크레이트(커밋 안 한 변경 포함 · 루트 패키지는 하나로 센다 · 문서는 안 센다)가
     전체의 `TASTY_VM_CLEAN_DIRTY_PCT` 를 넘는다.

  남기면 옮긴 뒤 이번 커밋을 표지로 쓴다. 판정은 다시 짓는 양의 근사다 — 바뀐 크레이트에 기대는 쪽까지는
  세지 않고, 지난 실행에서 커밋 안 하고 옮긴 변경은 표지에 안 남는다(그 몫은 cargo 가 다시 짓는다).
  지운(또는 남긴) 뒤 여유를 재고 필요한 여유(지웠으면 `TASTY_VM_MIN_FREE_GB`, 기본 12) 아래면 rc 2 로 멈춘다 — 그때는 `shell` 로
  들어가 `du -sh ~/*` 로 큰 자리를 찾는다. 이 스크립트는 자기가 만든 자리만 지운다.

- 커밋 안 한 변경까지 옮긴다 — 커밋 전 확인용이다.
- git 저장소를 옮긴다. 셸 게이트가 `git ls-files` 로 좌변을 정하므로, **아직 `git add` 안 한 새 파일은
  게이트의 좌변 밖**이다(호스트와 같다).
- **worktree 도 원본으로 받는다.** worktree 의 `.git` 은 호스트 경로를 가리키는 파일이라 그대로 옮기면
  가상머신에서 git 이 깨진다. 그래서 공용 저장소(objects · refs)를 `~/tasty/.git` 으로 옮기고 그 worktree 몫인
  `HEAD` 와 `index` 를 그 위에 덮는다 — 가상머신에서는 그 브랜치를 체크아웃한 보통 저장소가 된다.
  원본이 레포 꼭대기인지는 `git rev-parse --show-toplevel` 로 가린다.
- git 이 무시하는 것(`.gitignore` · `.git/info/exclude` · 전역 설정)과 `target/` 은 안 옮긴다.

## 4. GUI 시험의 전제

- **입력 권한**: 시험은 `enigo` 로 실제 키·마우스 이벤트를 보낸다. ssh 로 띄운 프로세스가 손쉬운 사용과
  이벤트 보내기 권한을 가져야 하고, 이미지가 그 권한을 미리 넣어 둔다. `test gui` 는 돌리기 전에
  실제 프로세스에 묻고(`AXIsProcessTrusted` · `CGPreflightPostEventAccess`), 없으면 rc 2 로 멈춘다.
- **화면 기록**: macOS 26 의 ssh 세션 프로세스(`/usr/libexec/sshd-session`)에는 이미지가 그 권한을
  안 넣어 둔다. 없으면 화면을 찍는 순간 권한 요청 창이 맨 앞에 떠서 시험 창을 가린다. `prepare` 가
  TCC 에 넣고 가상머신을 다시 띄운다.
- **`exec` 길(§2.1)**: 프로세스를 게스트 에이전트(`tart-guest-agent`)가 띄운다. 이미지가 그것에 세 권한
  (손쉬운 사용 · 이벤트 보내기 · 화면 기록)을 미리 넣어 두었으므로 TCC 에 넣지 않고 있는지만 묻는다 —
  에이전트 경로에 판 번호가 들어가 있어 넣을 값을 정할 수 없다. 없으면 rc 2 로 멈춘다.
- **`exec` 길의 Apple 이벤트**: 클릭 시험(`test_ime_preedit_cleared_on_popup_focus_shortcut`)은 `osascript` 로
  System Events 에 창 위치를 묻는다. 그 자동화 권한은 **사용자** TCC 가 게스트 에이전트를 주체로 판정하는데,
  이미지는 그 권한을 `osascript` 와 ssh 쪽에만 넣어 두었다. 처음 물을 때 뜬 권한 창은 아무도 못 눌러
  거부(`auth_value` 0)로 남고, 그 뒤로 `-1743` 으로 죽는다. 그래서 `exec` 길의 `test gui` 는 시작 전에 사용자
  TCC 에 그 행을 허용으로 쓰고 tccd 를 다시 띄운 뒤 `osascript` 로 다시 물어 확인한다 — 못 보내면 rc 2 로
  멈춘다. 실측 2026-10-27: 쓰자마자 듣고, 다시 읽어도 허용으로 남았다(2026-10-26 에는 tccd 를 다시 안 띄워
  거부로 되돌아갔다).
- **입력 소스**: 이미지의 입력 소스는 U.S. 다. 한글 IME 가 선택돼 있으면 키 시험이 조용히 죽는다
  (`tests/gui_tests.rs` 머리말).
- 가상머신은 창 없이(`--no-graphics`) 뜨지만 가상 디스플레이와 로그인 세션은 그대로 있어 GUI 앱이
  뜬다. 화면을 보려면 `shell` 로 들어가 `screencapture -x <파일>` 을 쓴다.

### 4.1 GPU — 가상머신에서도 실제 GPU 로 그린다 (실측 2026-10-15)

- 가상머신의 Metal 장치는 하나이고 **`Apple Paravirtual device`** 다 — Apple 가상화 기능이 호스트 GPU 를
  빌려 주는 가상 GPU 다(Metal `mac2` 기능군 지원, `apple7` 미지원 · 통합 메모리). 확인한 법: 가상머신에서
  `MTLCopyAllDevices()` 의 이름과 `supportsFamily` 를 찍었다.
- tasty 는 macOS 에서 wgpu 의 Metal 백엔드로 어댑터를 고르고, 하드웨어 어댑터가 없을 때만
  `force_fallback_adapter` 로 한 번 더 묻는다(`src/app.rs`). Metal 에는 소프트웨어 대체가 없으므로, 창이
  떴다는 것은 **이 가상 GPU 로 장치를 만들었다**는 뜻이다. 고른 어댑터 이름은 로그에 안 남는다.
- **GPU 를 쓰는 시험은 GUI 바이너리를 띄우는 것들이다.** GPU 장치를 직접 만드는 단위 시험은 없다
  (`src/gfx/gpu.rs` 의 시험은 크기 계산 같은 순수 함수다). 기본 조합의 e2e 는 창과 wgpu 장치를 만든 뒤에야
  IPC 를 연다([e2e-tests](e2e-tests.md) §0-1) — 그날 `e2e_tests` 64 · `e2e_single_instance_guard` 19 ·
  attach·webhook·hooks 계열과 GUI 시험이 모두 이 가상 GPU 위에서 돌았다.
- **느리다.** 직접 띄운 인스턴스에서 프레임의 화면 지우기(`clear`)가 170~370 ms 였다(`slow gpu render`
  경고). 그래서 이 가상머신의 지연 수치는 실기기의 렌더 성능을 대변하지 않는다. 속도 시험의 상한(1 s)에
  닿기도 한다 — 실측 2026-10-27(`f1a5b01dd`): `test_tab_switch_speed` 가 평균 802 ms · 최대 1013 ms 로 실패했다.
  환경을 바꿔 흉내 낼 외부 의존이 아니라 이 GPU 의 속도라, 그 실패는 실기기에서 다시 잰다. 입력에서 상태 변화까지는
  짧았다 — osascript 로 ⌘N 을 넣고 workspace 수가 바뀔 때까지 80~114 ms(3 회).
- 무시된 채 안 도는 GPU 관련 시험은 없다. `tests/gui_common/stderr_tests.rs` 의 둘은 Linux 격리 X
  디스플레이가 필요해 `#[ignore]` 인데, GPU 가 아니라 enigo 의 디스플레이 조건 때문이다.

## 5. 결과를 읽는 법 (실측 2026-10-15, `84d8891f2`)

이 절의 수는 그날 그 커밋의 값이다 — 지금 값은 `test` 를 다시 돌려 낸다. 가상머신의 실패가
macOS 고유인지는 **같은 시험을 호스트에서 돌려** 가른다(호스트도 실패하면 `main` 에 쌓인 실패다).

### 5.1 `test unit` — 7897 통과 · 0 실패 (`367d28bcf` 에서 잼 — GUI 는 `84d8891f2`)

첫 실행(`205bb993e`)에서는 13 이 실패했고, 그중 12 가 호스트에서도 같은 실패였다 — CI 가 전체
스위트를 자동으로 안 돌려 `main` 에 쌓인 것이다. 지금은 전부 고쳤다. 그때 갈래:

- 문서·명부가 코드를 안 따라옴: `webview.eval_js` 의 CLI 사유 · 메서드 수 스냅샷 · macOS Edit 메뉴 키
  목록 · supervisor `agent` 번역.
- **실제 결함이 시험에 키 이름으로만 드러남**: surface 우클릭 메뉴에 닫기 항목이 없었고, 폴더 선택 창이
  파일 제목을 달았고, supervisor 의 유령 정리 단추가 미리보기만 하고 아무것도 안 지웠다.
- poison 을 조용히 넘기는 lock 자리(plugin-sdk · supervisor), plugin 문구의 한국어 고정.
- plugin surface 를 띄우는 e2e 는 빌드 순서 문제였다 — `cargo test --workspace` 는 plugin 실행 파일을
  안 지어, 새 트리에서는 그 surface 종류가 없다. `test unit` 이 이제 `cargo build --workspace` 를 먼저 한다.

### 5.2 가상머신이 처음 잡은 것

- event bus 세대 표지 겹침 — 벽시계가 1 µs 단위라 연달아 만든 두 버스가 같은 값을 받았다. 호스트에서는 갈렸다.
- IPC 의 상한 초과 거절이 RST 로 사라지는 간헐 실패 — 부하 아래 200 회 중 2 회. 서버가 쓰기 쪽을 먼저
  닫고 남은 입력을 비운 뒤 닫게 고친 다음 600 회 중 0 회다.

### 5.3 `test gui` — 33 중 33 통과 (두 번 연속)

이 수에는 **단일 값이 없다** — 계기는 한 프로세스(`--test-threads=1`, 공유 인스턴스 하나)였고, 같은 시험을
하나씩 갈라 돌리면 다른 수가 나온다(실제로 그날 스위트 안에서만 실패하던 시험이 있었다).

처음에는 4~8 이었다. 가상머신이 아니라 시험·하네스·debug 주입 쪽 원인이었고, 갈래는 이렇다:

- **키가 빗나감**: 하네스가 macOS 에서 `alt` 를 Option(⌥)으로 눌렀다(tasty 는 Command(⌘)로 읽는다).
  시험이 옛 단축키(`Ctrl+Shift+T` 등)를 눌렀다 — 지금 기본값(`alt+t` 등)으로 옮겼고, 기본값이 빈 탭
  전환은 하네스가 격리 홈 설정으로 붙인다.
- **클릭이 빗나감**: macOS 의 `client_to_screen` 이 창 위치를 안 더하고 물리 픽셀을 그대로 넘겼다 —
  enigo 는 화면 포인트를 쓴다. 이제 System Events 로 창 위치를 묻고 배율로 나눈다.
- **debug 주입이 메뉴를 지움**: 주입 함수가 버튼 이벤트마다 포획한 메뉴를 비워, press 에서 메뉴를 세우는
  macOS 에서는 뒤따르는 release 주입이 그것을 지웠다(Linux 는 release 에서 세워 안 드러났다).
- **시험 사이 오염**: `shared()` 가 시험마다 바닥 상태로 되돌린다 — 모달·host popup·알림 패널·여분
  workspace·남은 선택을 걷고, workspace 를 **새 터미널 하나**로 간다. 앞 시험이 남긴 explorer 탭과
  마우스 추적 모드(`?1003h`)가 우클릭을 앱으로 보내 마우스 시험 넷이 스위트 안에서만 실패했다.
- **느린 활성화**: 하네스의 창 활성화가 키마다 1 초를 먹어 속도 시험 일곱이 상한(1 s)을 넘겼다 — 이미
  맨 앞이면 건너뛴다.

호스트 대조는 아직 없다 — 호스트에서 GUI 시험을 돌리면 사용자 데스크톱의 포커스를 가져간다. 위 고침은
macOS 하네스 경로(`client_to_screen` · `focus`)와 플랫폼 공통 경로(주입·바닥 상태)에 걸쳐 있다.

## 6. Linux 대조 — Lima 가상머신 (실측 2026-10-15, `46c6e5958`)

GUI 하네스의 고침 일부(debug 주입 · 바닥 상태)는 플랫폼 공통이라 Linux 에서도 잰다. 스크립트는 없고 아래
명령을 손으로 돌린다 — 호스트 홈은 Lima 가 읽기 전용으로 붙이므로 사본을 가상머신 로컬에서 빌드한다.

```bash
limactl start --name=tasty-linux --cpus=6 --memory=10 --disk=80 --vm-type=vz --tty=false template://ubuntu-24.04
limactl shell tasty-linux -- sudo apt-get install -y build-essential pkg-config cmake git rsync \
  libgtk-3-dev libwebkit2gtk-4.1-dev libxdo-dev libfontconfig1-dev libfreetype6-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libssl-dev libayatana-appindicator3-1 \
  xvfb xauth xdotool mesa-vulkan-drivers libvulkan1 libegl1 libgl1-mesa-dri
# rustup 설치 뒤, git 이 무시하는 경로를 빼고 레포를 ~/tasty 로 옮긴다(macOS 스크립트의 sync_tree 와 같은 목록)
cargo install tokei                            # 파일 SLOC 시험이 부른다
git add -A                                     # 옮긴 트리에 git 인덱스가 있어야 한다(아래)
cargo build --workspace --locked
xvfb-run -a -s "-screen 0 1920x1080x24" cargo test --locked --test gui_tests -- --ignored --test-threads=1
xvfb-run -a bash -c 'TASTY_E2E_DISPLAY=$DISPLAY cargo test --workspace --locked --no-fail-fast'
```

- **전체 스위트(`cargo test --workspace`)를 돌릴 때 준비가 셋 더 든다**(실측 2026-10-27, `658971d89`). 빠뜨리면
  77 건이 실패했다:
  - e2e 는 디스플레이를 `TASTY_E2E_DISPLAY` 로 받는다 — `xvfb-run` 이 정한 `$DISPLAY` 를 그 이름으로 넘긴다(75 건).
  - SLOC 시험이 `tokei` 를 부른다(1 건).
  - 레포 파일 목록을 `git ls-files` 로 구하는 시험이 있다 — `.git` 을 빼고 옮겼거나 예전 저장소가 남아 있으면
    목록이 비거나 어긋난다. 옮긴 자리에서 `git add -A` 로 인덱스를 맞춘다(1 건).

- **결과**: 35 중 35 통과, 두 번 연속(GUI 33 + Linux 격리 X 디스플레이에서만 도는 `stderr_tests` 2).
  이 수에는 **단일 값이 없다** — 계기가 한 프로세스(`--test-threads=1`, 공유 인스턴스 하나)였고, 시험을
  갈라 돌리면 다른 수가 나온다([ci-gates](ci-gates.md) 의 GUI 층 서술).
- **GPU 가 없다.** 가상머신에 `/dev/dri` 가 없어 wgpu 는 mesa 의 소프트웨어 렌더러로 그렸다(lavapipe 인지
  llvmpipe 인지는 안 쟀다). macOS 가상머신(§4.1)과 반대 조건이라, 두 결과를 합쳐야 "GPU 가 있든 없든" 이 된다.
- **처음에는 빌드부터 실패했다.** `src/host_api/webview.rs` 의 navigation 판정이 macOS 백엔드만 쓰는데
  모든 OS 에서 컴파일돼 `-D dead-code` 에 걸렸다 — Linux dev gui 컴파일은 자동 채널이 없는 자리다
  ([ci-gates](ci-gates.md)). `46c6e5958` 에서 고쳤다.
- **트레이 라이브러리가 없으면 부팅에서 패닉한다.** `libayatana-appindicator3` 가 없으면
  `libappindicator-sys` 가 `Failed to load ayatana-appindicator3 or appindicator3 dynamic library` 로 죽고,
  시험은 33 건 전부 "부팅 중 종료" 로 나왔다. 제품 쪽은 트레이를 만들기 전에 그 라이브러리를 먼저 열어
  보도록 고쳤다(`system_tray::appindicator_present`) — 지운 상태에서 패닉 없이 트레이 없이 뜨고 경고만
  남는 것을 같은 가상머신에서 쟀다. 위 패키지 목록에 그대로 둔 것은 트레이 경로까지 시험하려는 것이다.

## 7. 정리

```bash
bash scripts/macos-vm-test.sh stop        # CPU·메모리를 돌려준다
bash scripts/macos-vm-test.sh destroy     # 가상머신 디스크를 지운다
tart prune --entries=caches               # 받아 둔 이미지(31 GB)까지 지운다
```
