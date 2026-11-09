# 의존성 이슈 모니터링

외부 의존성에서 비롯돼 우리 코드로 즉시 해결할 수 없는 빌드 이슈 추적.

## `block v0.1.6` — future-incompatibility (모니터링 중)

`cargo build`/`clippy` 끝의 경고:

```
warning: the following packages contain code that will be rejected by a future version of Rust: block v0.1.6
```

- 원인: transitive 의존 `block v0.1.6`(기존 버전)의 uninhabited static — Rust [#74840](https://github.com/rust-lang/rust/issues/74840) 에 따라 향후 hard error 예정(hard-error toolchain 버전 미정).
- 의존 경로:
  1. `cocoa` ← `drag` — `drag` 신버전이 cocoa 의존을 제거 → `cargo update` 로 해소 예정.
  2. `metal` ← `wgpu-hal` ← `wgpu` — wgpu 가 `block2`(objc2 생태계) 기반 metal 로 올라간 버전 채택 시 해소. wgpu major upgrade 시 점검.
- 대응: **upstream upgrade 대기**(`[patch.crates-io]` 우회·의존 제거는 hard-error 임박 전엔 안 함).

### 점검 / 전환 트리거

```bash
cargo report future-incompatibilities
cargo tree -i block@0.1.6     # 빈 출력이면 본 항목 삭제
```

다음 중 하나면 대기 종료 후 우회 적용: Rust release note 에 hard-error 화 버전 공지 / 경고가 deny 승격으로 빌드 실패. 전환 시 ① `[patch.crates-io]` 로 fixed fork 대체(ABI 호환 필수) ② wgpu 를 block2 기반으로 선제 업그레이드.

## `winit` — 개인 포크 핀 (IME · X11 서버 재연결 · 외부 토큰 활성화)

`Cargo.toml` 의 `[patch.crates-io]` 가 winit 을 개인 포크 `zilhak/winit-ime-fix` 의 `tasty/xim-bounded-20261024` 브랜치에 공개된 커밋에 핀:

```toml
winit = { git = "https://github.com/zilhak/winit-ime-fix.git", rev = "83d620da3b15518fd9b6371138957a6da85964ab" }
```

- **포크가 담은 패치**: 업스트림 winit 0.30.13 위에 아래 변경을 얹었다.
  - 한글·CJK IME 입력 수정(macOS 한글 IME 트리거 키, 입력 소스 전환 뒤 첫 글자, Cmd 키를 누른 동안 IME 처리 건너뛰기). 업스트림 PR [#4478](https://github.com/rust-windowing/winit/pull/4478) 로 제출돼 있다.
  - macOS `NSApplicationDelegate` 를 외부에서 바꿔도 panic 하지 않게 하는 수정.
  - Linux X11 원격 XIM을 이벤트 기반 프로토콜로 처리하고 응답 기한이 지나면 연결 세대를 폐기한다. 서버 광고 또는 재시도 기한 뒤 다시 연결한다. local Compose와 키의 불확실한 결과, 자원 수명 및 검증 한계는 [X11 IME 서버 복구](x11-ime-recovery.md)에 있다.
  - Wayland 외부 xdg-activation 토큰으로 이미 있는 창을 활성화하는 `WindowExtWayland::activate_with_token`. 업스트림에는 대응 API 가 없다. Tasty 는 이 API 를 직접 호출한다(`crates/tasty-platform/src/window_activation.rs`). 그래서 `[patch.crates-io]` 를 걷으면 컴파일이 실패한다.
- **빌드 결합**: X11 프로토콜 처리는 선택 의존성 `xim-parser = 0.2.2`, `xim-ctext = 0.4.1`을 사용한다. 포크의 최소 Rust 버전은 1.82이며 본체의 고정 도구 체인 1.96.0이 이를 충족한다. Wayland·macOS·Windows 입력 구현은 이 프로토콜을 사용하지 않는다.
- **왜 `rev` 핀인가**: `branch =` 핀은 브랜치가 움직이면(force-push 포함) 빌드 입력이 조용히 바뀌어 재현성이 깨진다. 고정 commit(`rev`)에 핀해 빌드 입력을 동결한다. 포크 갱신이 필요하면 `rev` 를 의도적으로 갱신한다.
- **리스크**: 포크 레포가 사라지거나 commit 이 GC 되면 빌드 재현 불가. 핀한 커밋을 가리키는 `tasty/xim-bounded-20261024` 브랜치를 보존한다. 이 커밋에 도달하는 참조를 모두 지우거나 force-push 하면 GC 대상이 된다. 포크 rev 를 바꿀 때는 새 커밋이 원격 브랜치에 push 된 뒤 바꾼다(그 전에는 새 clone 의 `--locked` 빌드가 fetch 에 실패한다). winit 0.30 업스트림 업그레이드는 포크가 따라가야 가능(wgpu/egui-winit 업글과 충돌 여지).

### 점검 / 전환 트리거

- 전환 조건: 아래가 모두 공식 crates.io 버전에 들어오면 `[patch.crates-io]` 항목을 제거한다.
  - PR #4478(한글 IME 수정)의 머지·릴리스.
  - X11 XIM 서버 종료·교체 시 동기 요청 보호와 fallback 이후 재연결을 제공하는 상류 수정. 전환할 때 [복구 계약](x11-ime-recovery.md)의 생존 확인·콜백 경계를 대조한다.
  - 이미 있는 창을 외부 xdg-activation 토큰으로 활성화하는 상류 API. 들어오면 `window_activation.rs` 의 Wayland 호출을 그 API 로 바꾼다.
  - `NSApplicationDelegate` 교체 수정의 상류 반영. 또는 Tasty 가 이 수정에 의존하지 않는지 확인한다(Tasty 는 위임자를 바꾸지 않고 winit 위임자 클래스에 메서드를 주입한다, `crates/tasty-platform/src/macos_delegate.rs`).
- PR #4478 상태와 외부 토큰 활성화 API 의 상류 진행을 주기적으로 확인한다(머지/클로즈/대체 PR 여부).
- **대비책**(포크 레포 소실 시): 해당 commit 을 조직 레포에 미러링하거나 `vendor/` 로 캐싱. 핀 고정으로 충분할 수 있으므로 레포 소실 징후가 보일 때만 착수(과투자 주의).

## `tiny_http` — 레포 사본 + 최소 패치 (공식 버전 전환 대기)

`Cargo.toml` 의 `[patch.crates-io]` 가 tiny_http 를 레포 안의 사본에 핀:

```toml
tiny_http = { path = "vendor/tiny_http" }
```

- **왜 사본인가**: 상류 0.12.0 의 `Content-Length` 리더는 요청을 파괴할 때 읽지 않은 body 를 끝까지 읽는다(drain). 웹훅 413 이 그 drain 없이 연결을 닫게 하는 공개 API(`Request::respond_and_close`)를 더한 최소 패치다. 결정·대안·재검토 조건은 [웹훅 (Inbound webhook listener)](../features/webhook/index.md), 사본 범위·패치 목록은 [`vendor/tiny_http/PATCHES.md`](../../vendor/tiny_http/PATCHES.md).
- **워크스페이스 멤버가 아니다**: 루트 `Cargo.toml` 의 `exclude` 에 있다 — 워크스페이스 lint·clippy·fmt·파일 SLOC 게이트가 상류 코드를 판정하지 않고, `crates/` 크레이트 수에도 안 들어간다.
- **리스크**: `cargo update` 가 이 의존을 올리지 않고 상류 수정도 자동으로 포함되지 않는다.
- **사본을 고치면 plugin 버전도 오른다**: 번들 plugin 중 `tasty-plugin-agent-stream` 이 이 사본을 링크한다. 사본에서 테스트 전용이 아닌 코드를 고친 커밋은 plugin 버전 게이트가 그 plugin 의 patch +1 을 요구한다 — 검사 대상이 워크스페이스 밖 path 의존까지 닿는다([플러그인 버전 비교](release.md#플러그인-버전-비교)). 테스트 전용 변경과 `PATCHES.md` 는 요구하지 않는다.

### 점검 / 전환 트리거

- 전환 조건: 상류가 요청 단위로 잔여 body 를 읽지 않고 연결을 닫는 공개 API 를 릴리스 → 사본과 `[patch.crates-io]` 줄을 지우고 그 API 로 옮긴다. 옮긴 뒤 `src/webhook/listener_body_tests.rs` 의 `raw_oversize_*` 시험 넷(413)과 `raw_blocked_source_is_closed_without_draining`(429)이 통과해야 한다 — 뒤의 것은 이름 패턴이 달라 `raw_oversize_*` 로 걸리지 않는다.
- RustSec 에 `tiny_http` 권고가 붙으면 사본에 수정을 옮기거나 상류 새 버전 위에 패치를 다시 얹는다.

## `epaint` — 레포 사본 + 최소 패치 (글리프가 atlas보다 클 때 패닉)

`Cargo.toml` 의 `[patch.crates-io]` 가 epaint 를 레포 안의 사본에 핀:

```toml
epaint = { path = "vendor/epaint" }
```

- **왜 사본인가**: 상류 0.31.1 은 글리프가 글꼴 atlas 보다 크면 패닉한다. 파일을 미리 판정해서는 막을 수 없다. 패닉 여부가 글자 크기·배율·앞서 그린 글자에 달려 있고, 정상 글꼴도 해당된다(`NotoSansArabic-Bold`의 U+FDFD, 72pt, 배율 4). 글리프를 만드는 지점에서 그 글자만 비우는 패치다. 사본 범위와 패치 내용은 [`vendor/epaint/PATCHES.md`](../../vendor/epaint/PATCHES.md).
- **적용 범위**: `[patch.crates-io]` 는 이 workspace 의 빌드에만 적용된다. `tasty-official-plugins` 레포는 자기 workspace 로 빌드해 상류 epaint 를 쓰므로 같은 패닉이 남아 있다.
- **egui 를 올릴 때**: 사본도 함께 옮긴다. `Cargo.lock` 의 epaint 판이 사본과 어긋나면 `[patch]` 가 쓰이지 않으므로 `crates/tasty-egui-theme/tests/oversized_glyphs.rs` 를 같이 돌린다.
- **라벨 글꼴 family**: 사본은 `MONO_LINE_SANS_FAMILY`도 제공한다. `tasty_ui_widgets::label_font`가 ja·zh-Hans에서 쓴다. 사본을 지우려면 이 기능을 옮길 곳도 있어야 한다. 상류 epaint로는 이 이름이 등록되지 않은 family라 패닉한다.
- **사본을 지울 수 있는 조건**: 상류 epaint 가 폭·높이가 큰 글리프와 atlas 넘침 뒤 되돌림에서 패닉하지 않고, 이미지 높이가 GPU 텍스처 한도를 넘지 않으며, 위 시험의 입력을 사본 없이 통과할 때. 상류로 확인할 때는 사본에만 있는 `can_always_allocate` 를 부르는 시험 2개(`the_limits_are_the_atlas_width_and_half_of_it`, `any_order_of_allowed_sizes_stays_in_bounds`)를 먼저 뺀다. 조건을 만족하면 `vendor/epaint`, `[patch.crates-io]` 항목, 저장소 `exclude` 항목, `no_todo_file_citation` 의 예외 줄, 이 두 시험을 함께 지운다. epaint 0.36.1 에는 같은 assert, 커서 되돌림, 높이 증가 조건이 있다(코드 읽기).
- **상류와의 차이를 재는 방법**: crates.io 묶음과 비교한다. `vendor/epaint/src/text/font.rs`, `vendor/epaint/src/text/fonts.rs`, `vendor/epaint/src/text/mod.rs`, `vendor/epaint/src/texture_atlas.rs` 만 달라야 한다.

  ```bash
  diff -r "$(ls -d ~/.cargo/registry/src/*/epaint-0.31.1)/src" vendor/epaint/src
  ```

### 남는 한계

- **글자 깨짐**: 허용 크기 안의 큰 글리프가 atlas 를 넘치게 하면 상류대로 앞선 글리프 자리를 덮어쓰고, 다음 프레임에 atlas 를 다시 만든다. 그동안 화면의 다른 글자 일부가 깨져 보인다. 패치 전 바이너리에서도 정상 글꼴 250pt, 배율 2 로 같은 화면을 확인했다. 입력란 상한 72pt 에서 정상 글꼴(D2Coding, 배율 4)은 넘침 경고가 없었다.
- **1024px 초과 글리프**: atlas 폭 2048 에서 높이 1024px 를 넘는 글리프는 앞선 줄이 적어 그릴 수 있는 상태에서도 항상 빈다. 같은 글자가 앞서 그린 글자에 따라 달라지지 않게 하기 위한 판정이다.
- **이미지 높이 상한**: atlas 이미지 높이는 폭(앱은 2048)에서 멈춘다. 상류는 행이 이미지 높이에 정확히 맞으면 이미지를 늘리므로, 판정을 통과한 크기만으로 8192 를 넘겨 wgpu 업로드에서 패닉한다(`Dimension Y value 16384 exceeds the limit of 8192`). 앱의 wgpu 한도는 8192 다. 상한의 대가로 행이 2048 에 정확히 맞으면 다음 행에서 넘친다.
  - 글리프 단계 비교: `D2Coding`, `NotoSansCJK-Regular`, `NotoSerifCJK-Regular`, `NotoNastaliqUrdu-Bold`, `NotoSansArabic-Bold` 에 ASCII 와 한글 11172자, 한자 20902자, 아랍 문자를 한 atlas 에 차례로 넣었다(10~72pt 2pt 간격, 배율 1·1.25·1.5·2·3·4, 2880칸). 상한을 뺀 사본과 2840칸은 atlas 픽셀까지 같다. 40칸이 다르고, 모두 상한 없는 쪽 이미지가 2048 을 넘어 커진 칸이다. 그 칸에서는 처음 넘치는 시점이 앞당겨진다. 예: `NotoSansCJK-Regular` 한글 14pt 배율 2 는 6144자 근처, 72pt 배율 2 는 320자 근처(64자 단위 측정). 상한 없는 쪽이 얼마나 더 버티는지는 칸마다 재지 않았다. 40칸 중 3칸(`NotoSerifCJK-Regular` 한자, 20pt 배율 4, 40pt 배율 2, 64pt 배율 1.25)은 상한 없는 쪽 높이가 16384 까지 커진다. 즉 상류에서는 수열에 맞춰 만든 글꼴이 아니어도 서로 다른 글자가 많으면 한도를 넘는다. 이 입력의 앱 실행은 재지 않았다.
  - 넘침 시점: 이 2880칸 중 1249칸이 넘치고, 처음 넘치는 시점은 128자(64pt 배율 4)에서 20032자(16pt 배율 2) 사이다.
  - 앱 비교: 설정 미리보기와 일반 화면을 상한을 뺀 빌드와 픽셀로 비교했다. D2Coding 14pt 배율 1·2, NotoSansCJK 14pt 배율 1·2, NotoSansCJK 72pt 배율 2, 언어팩 글꼴 NotoSansCJK 배율 1, 한국어·일본어 UI 배율 1·2 의 10칸, 창 2개씩 20쌍이 모두 차이 0 이고 넘침 경고도 0 이다. 같은 빌드를 두 번 찍은 노이즈 바닥도 0 이다. 서로 다른 글리프를 수천 개 그리는 egui 화면은 앱에서 재지 않았다.
  - 패닉 입력(폭 2048, 높이 341~1024 글리프 9개): 적용, 설정 열기, 다시 그림, 재시작, 재시작 후 설정 열기, 다시 그림의 6칸 모두 패닉과 크래시 보고가 없다. 넘침이 계속되므로 큰 글리프가 화면을 덮고 다른 글자 일부가 깨진다.
- **경고 로그**: 본체는 빈 글리프마다 경고 한 줄을 남긴다. atlas 를 다시 만들면 글리프 캐시가 함께 버려지므로 넘침이 계속되는 입력에서는 다시 그릴 때마다 반복된다. 경고는 epaint 의 `log` feature 뒤에 있어, `Justfile` 이 `-p` 로 빌드하는 번들 플러그인에서는 남지 않는다(`cargo tree -e features -i epaint` 로 확인). `--workspace` 빌드에서는 feature 가 켜진다.
- **확인 범위**: Linux 에서 설정 창과 언어팩 경로를 실제 실행으로 확인했다. Windows·macOS 실행과 플러그인 프로세스의 실제 실행, atlas 폭 8192·16384 의 임의 순서 시험, 넘침이 계속될 때의 재업로드 비용은 재지 않았다.

<a id="은퇴-egui_commonmark--egui-버전-lockstep"></a>

## egui 버전 호환

markdown 본문은 WebView로 그리지만 대용량 파일·파일 열기 확인 팝업은 egui-mesh를 쓴다.
따라서 다른 mesh plugin과 같이 host·plugin의 epaint wire 호환을 확인해야 한다.
기준은 [egui-mesh 채널](egui-mesh-channel.md)을 따른다.
