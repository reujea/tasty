# epaint 0.31.1 — tasty 사본

crates.io의 `epaint` 0.31.1(상류 커밋 `1669e52a7ccfc3489c1b0999b9ed48894a0b3887`, `crates/epaint`)을 옮긴 사본이다. 루트 `Cargo.toml`의 `[patch.crates-io]`가 이 디렉토리를 가리키므로 본체, 갤러리, egui를 쓰는 모든 플러그인이 이 사본으로 빌드된다. `tasty-plugin-sdk-wasm`은 workspace 밖이지만 egui를 쓰지 않는다.

## 옮긴 것과 뺀 것

- 포함: `src/` 전체, crates.io가 정규화한 `Cargo.toml`. 라이선스는 상류의 `MIT OR Apache-2.0`을 유지한다. crates.io 묶음에 라이선스 파일이 없어 `LICENSE-MIT`, `LICENSE-APACHE`는 같은 상류 커밋의 레포 루트에서 가져왔다.
- 제외: `benches/`, `README.md`, `Cargo.toml.orig`.
- `Cargo.toml`에서 뺀 것: `[[bench]]`, `[dev-dependencies]`, `[lints]` 전체, `readme`, `include`. lints를 뺀 이유는 사본이 workspace 밖이라 상류의 lint 목록이 이 저장소의 도구 체인에서 경고가 될 수 있기 때문이다.
