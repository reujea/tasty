# vtparse 0.6.2 — tasty 사본

crates.io의 `vtparse` 0.6.2(상류 커밋 `edeae72b5fc55c7fa4aa1d08bbefd08c5493f757`, `vtparse`)에 아래 패치를 적용한 사본이다. 루트 `Cargo.toml`의 `[patch.crates-io]`가 이 디렉토리를 가리키므로 이 workspace에서 vtparse를 직접 의존하는 세 크레이트가 이 사본으로 빌드된다. 루트 `tasty`(gui 기능, `src/remote/copy_progress.rs`), `tasty-terminal`, termwiz다. `tasty-official-plugins` 레포는 vtparse를 쓰지 않는다.

## 옮긴 것과 뺀 것

- 포함: crates.io 묶음 전체(`src/`, crates.io가 정규화한 `Cargo.toml`, `Cargo.toml.orig`, `README.md`, `.cargo_vcs_info.json`)를 바꾸지 않고 옮겼다. 라이선스는 상류의 `MIT`를 유지한다. crates.io 묶음에 라이선스 파일이 없어 `LICENSE.md`는 같은 상류 커밋의 레포 루트에서 가져왔다.
- 제외: 없다. `[dev-dependencies]`(k9)도 남긴다. 사본이 workspace 밖이라 루트 `Cargo.lock`에는 들어가지 않는다.

## 패치 — 파서 상태 조회

상류는 파서 상태를 `is_ground()`로만 알린다. 열린 시퀀스를 final 바이트 없이 떠날 때(ESC, C1 도입자, CAN·SUB) 콜백이 오는지는 떠나는 상태의 exit 동작이 정한다(`src/transitions.rs`의 `EXIT`). OSC 문자열(`OscEnd` → `osc_dispatch`), APC 문자열(`ApcEnd` → `apc_dispatch`), DCS 본문(`Unhook` → `dcs_unhook`)은 무엇으로 떠나든 콜백이 온다. Escape·Escape 중간 바이트, CSI 네 상태, DCS 머리 네 상태(entry·param·intermediate·ignore), SOS/PM 문자열은 exit 동작이 없어 ESC나 C1 도입자로 떠나면 콜백이 없다. CAN·SUB는 `execute_c0_or_c1`만 부르고 ground로 간다. attach 효과 소유자가 떠난 뒤 그가 시작한 시퀀스가 어느 바이트에서 끝나는지 판정하려면 바이트마다 상태를 읽어야 한다(`effects_held_prefix`, `crates/tasty-terminal/src/attach_stream.rs`).

- `src/lib.rs`: `State`를 `pub use`로 공개하고 `VTParser::state()`를 더한다. 현재 상태를 돌려주기만 한다.
- `src/enums.rs`: `State::from_u16`을 `pub(crate)`로 좁힌다. 이 함수는 검사 없는 transmute라 `State`가 공개되면 안전한 코드에서 잘못된 값을 만들 수 있다.

전이표와 파싱 동작은 바꾸지 않는다. 패치에는 `tasty patch` 주석이 있다. crates.io의 0.6.2와 `diff -r <원본>/src src`로 비교할 수 있다.

## 시험

attach 효과 소유자 시험(`crates/tasty-terminal/src/attach_stream_tests.rs`)이 이 조회를 쓰는 판정을 검사한다. 사본 자체의 시험은 루트에서 `cargo test -p vtparse`로 돌릴 수 없다(workspace 밖 패키지의 dev-dependencies). 이 디렉토리를 저장소 밖으로 복사해 그 안에서 `cargo test`를 돌린다. 저장소 안에서 돌리면 이 디렉토리에 `Cargo.lock`과 `target/`이 생긴다.

이 사본은 워크스페이스에서 제외되어 `cargo fmt`와 저장소 가드가 포맷을 검사하지 않는다. 패치를 수정한 뒤 다음 명령을 실행한다.

```sh
rustfmt --edition 2018 --check vendor/vtparse/src/lib.rs
```

## 걷는 조건

상류 vtparse가 파서 상태를 읽는 공개 API를 제공하면 이 사본과 `[patch.crates-io]` 항목을 제거하고 그 API로 옮긴다. 검증은 위 시험을 따른다.
