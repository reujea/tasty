# epaint 0.31.1 — tasty 사본

crates.io의 `epaint` 0.31.1(상류 커밋 `1669e52a7ccfc3489c1b0999b9ed48894a0b3887`, `crates/epaint`)에 아래 패치를 적용한 사본이다. 루트 `Cargo.toml`의 `[patch.crates-io]`가 이 디렉토리를 가리킨다. 이 항목은 이 workspace의 빌드에만 적용되므로 본체, 갤러리, 이 workspace의 플러그인이 이 사본으로 빌드된다. `tasty-plugin-sdk-wasm`은 workspace 밖이지만 egui를 쓰지 않는다. `tasty-official-plugins` 레포는 자기 workspace로 빌드하지만, 그 레포의 `[patch.crates-io]`가 epaint를 이 저장소의 고정 커밋(git `rev`)에서 받아 같은 사본으로 빌드한다.

## 옮긴 것과 뺀 것

- 포함: `src/` 전체, crates.io가 정규화한 `Cargo.toml`. 라이선스는 상류의 `MIT OR Apache-2.0`을 유지한다. crates.io 묶음에 라이선스 파일이 없어 `LICENSE-MIT`, `LICENSE-APACHE`는 같은 상류 커밋의 레포 루트에서 가져왔다.
- 제외: `benches/`, `README.md`, `Cargo.toml.orig`.
- `Cargo.toml`에서 뺀 것: `[[bench]]`, `[dev-dependencies]`, `[lints]` 전체, `readme`, `include`. lints를 뺀 이유는 사본이 workspace 밖이라 상류의 lint 목록이 이 저장소의 도구 체인에서 경고가 될 수 있기 때문이다.

## 패치 — atlas가 담을 수 없는 글리프를 비운다

상류는 글리프가 atlas보다 크면 프로세스를 끝낸다. 글꼴 하나, 글자 크기 하나로 앱이 내려간다.

- 폭이 atlas보다 크면 `TextureAtlas::allocate`의 assert에서 패닉한다.
- 높이가 넘치면 `allocate`가 이미지를 늘리지 않고 커서를 이미지 높이의 3분의 1로 되돌린다. 그 아래 남은 높이보다 글리프가 크면 `text/font.rs`의 쓰기가 범위를 넘어 패닉하고, 쓰기를 건너뛰어도 `take_delta`의 region에서 패닉한다.

바꾼 곳은 두 파일이다. 패치에는 `tasty patch` 주석이 있고, crates.io의 0.31.1과 `diff -r <원본>/src src`로 비교할 수 있다.

- `src/texture_atlas.rs`: `TextureAtlas::can_always_allocate`를 더한다. 폭은 atlas 폭까지, 높이는 atlas 폭 이하의 가장 큰 2의 거듭제곱의 절반까지 허용한다. atlas 폭 2048이면 2048×1024다. 이유는 함수 주석에 있다.
- `src/text/font.rs`: `FontImpl::allocate_glyph`가 `allocate` 전에 이 판정을 부른다. 통과하지 못한 글리프는 `UvRect::default()`로 비우고 advance를 1em 이하로 줄인다. epaint의 `log` feature가 켜진 빌드는 경고 로그도 남긴다. 번들 플러그인을 `-p`로 빌드하면 이 feature가 꺼져 경고가 남지 않는다. 원래 advance를 두면 수만 pt가 되어 같은 줄의 나머지가 창 밖으로 밀린다. 측정에서 언어팩 글꼴의 큰 글리프가 설정 창 탭 줄을 밀어냈다.

판정을 상태에 맞추지 않고 크기만 보는 이유는 같은 글자가 앞서 그린 글자에 따라 비거나 그려지지 않게 하기 위해서다. 그 대가로 지금까지 앞선 줄이 적을 때만 그려지던 높이 1025px 이상의 글리프가 항상 빈다.

패치가 바꾸지 않는 것:

- 허용 크기 안의 글리프가 atlas를 넘치게 하면 상류와 같이 앞선 글리프 자리를 덮어쓰고 다음 프레임에 atlas를 다시 만든다. 화면의 다른 글자 일부가 깨져 보일 수 있다. 패치 전 바이너리에서 정상 글꼴 250pt, 배율 2로 같은 화면을 확인했다.
- `epaint`의 공개 타입은 그대로이고 `TextureAtlas`에 메서드 하나만 늘었다. egui-mesh 채널의 와이어 형식에 닿지 않는다.

## 패치 — atlas 이미지 높이를 폭에서 멈춘다

상류는 행이 이미지 높이에 정확히 맞으면 이미지를 두 배로 늘린다. `allocate`의 넘침 판정은 `max(이미지 높이, 폭)`을 쓰므로 한도도 함께 올라가고, 다음 행이 다시 정확히 맞으면 또 늘어난다. 그래서 판정을 통과한 크기만으로 이미지 높이가 GPU 텍스처 한도를 넘을 수 있다. 앱의 wgpu 한도는 8192이고, 2048×1024 이하 글리프 9개로 된 글꼴에서 높이 16384를 만들면 업로드에서 패닉했다(`Dimension Y value 16384 exceeds the limit of 8192`).

`src/texture_atlas.rs`의 `resize_to_min_height`는 이미지 높이가 폭에 이르면 더 늘리지 않는다. 폭이 2의 거듭제곱이면 높이는 폭을 넘지 않고, 그렇지 않으면 폭 이상의 가장 작은 2의 거듭제곱에서 멈춘다. 앱의 atlas 폭은 2048이다. `allocate`는 필요 높이가 `max(이미지 높이, 폭)` 이하일 때만 이 함수를 부르므로 글리프는 계속 이미지 안에 들어간다. 넘침 때 이미지 높이가 폭 이하 최대 2의 거듭제곱 이상이라는 `can_always_allocate`의 근거도 그대로다.

대가로 행이 폭에 정확히 맞은 뒤에는 이미지가 늘지 않고 다음 행에서 넘친다. 측정값은 [의존성 이슈](../../docs/dev-guide/dep-issues.md)의 남는 한계에 있다.

## 시험

`crates/tasty-egui-theme/tests/oversized_glyphs.rs`가 이 패치를 검사한다. 사본 자체의 `cargo test`는 돌리지 않는다.

이 사본은 워크스페이스에서 제외되어 `cargo fmt`와 저장소 가드가 포맷을 검사하지 않는다. 패치를 수정한 뒤 다음 명령을 실행한다. 상류와 같은 포맷을 유지하면 이후 패치를 다시 적용할 때 불필요한 차이를 줄일 수 있다.

```sh
rustfmt --edition 2021 --check --config skip_children=true vendor/epaint/src/texture_atlas.rs vendor/epaint/src/text/font.rs vendor/epaint/src/text/fonts.rs
```

## 패치 — mono의 행 높이와 baseline을 쓰는 sans family

`tasty_ui_widgets::label_font`는 ja·zh-Hans에서 UI sans로 풀린다. RULES 8절은 이 라벨이 같은 크기 mono의 행 높이와 baseline을 갖도록 요구한다. 언어를 바꿔도 레이아웃이 움직이지 않게 하기 위해서다. 상류의 `FontTweak`은 baseline을 크기에 비례해 옮길 뿐 행 높이를 바꾸지 못한다. `Font`의 행 높이는 체인 첫 글꼴의 값이고, 대체 글꼴은 그 줄 상자의 가운데에 놓인다.

- `src/text/fonts.rs`: `pub const MONO_LINE_SANS_FAMILY`를 더한다. `FontFamily::Name`으로 이 이름을 받으면 등록 없이 Proportional 체인을 쓰고, 체인의 글꼴마다 같은 크기 Monospace 첫 글꼴의 행 높이와 ascent를 준다. 그 Monospace 첫 글꼴을 이 family의 체인 맨 끝에 더해, 체인 전체에 없는 글자만 그 글꼴로 그린다. 글꼴을 설치하는 모든 경로에서 쓸 수 있고, 값은 그 글꼴 구성과 배율에서 읽으므로 상수가 없다. 구성에 Monospace가 없으면 Proportional 그대로 그린다.
- `src/text/font.rs`: `FontImpl::with_line_box_of`를 더한다. 글꼴과 래스터는 그대로이고 글리프 캐시는 따로 가지므로 같은 글자가 atlas에 한 번 더 들어간다. 얇은 공백의 폭은 행 높이에서 나오므로 그 글자만 mono 행 높이를 따른다.
- 다른 family는 그대로다. Proportional 본문과 Monospace는 값과 체인이 바뀌지 않는다.

`crates/tasty-ui-widgets/src/label_font.rs`의 시험이 행 높이, baseline, 글자 폭을 잰다.

## 걷는 조건

조건과 상류 비교 명령, 남는 한계는 [의존성 이슈](../../docs/dev-guide/dep-issues.md)에 있다. egui를 올리는 것만으로는 해결되지 않으므로 이 패치를 새 판으로 옮기거나 위 시험으로 상류 동작을 다시 확인한다. 상류로 확인할 때는 `can_always_allocate`를 부르는 시험 2개(`the_limits_are_the_atlas_width_and_half_of_it`, `any_order_of_allowed_sizes_stays_in_bounds`)를 빼야 컴파일된다.
