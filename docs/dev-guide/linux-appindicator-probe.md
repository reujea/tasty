# Linux AppIndicator 사전 검사의 unsafe 불변조건

Linux 트레이는 `tray-icon`이 `libappindicator-sys`로 AppIndicator 라이브러리를 실행 중에 연다. 그 크레이트는 라이브러리를 열지 못하거나 함수를 찾지 못하면 패닉하므로, Tasty는 트레이를 만들기 전에 같은 라이브러리를 `dlopen`·`dlsym`으로 확인한다(`crates/tasty-platform/src/system_tray.rs`의 `first_loadable`·`lookup_symbol`). 동작과 실패 처리는 [시스템 트레이 정책](../design/policies/system-tray.md#linux-특이사항-gtk)에 있다. 이 문서는 [clippy 정책](clippy-policy.md)의 새 unsafe 절차 4번에 따라 이 OS 의존 unsafe가 기대는 조건을 적는다.

## 핸들 수명

- `first_loadable`은 `libloading`과 같은 플래그(`RTLD_LAZY | RTLD_LOCAL`)로 연 핸들을 돌려주고, 그 핸들을 `dlclose`에 넘기지 않는다. 그래서 라이브러리 종료 루틴이 이 코드로 실행되지 않는다.
- `lookup_symbol`은 `unsafe fn`이다. 호출자는 `dlopen`이 돌려준 null 아닌 핸들, 즉 `dlclose`에 넘기지 않은 핸들만 넘긴다. 유일한 호출자 `probe`는 바로 앞에서 `first_loadable`이 연 핸들을 넘긴다.
- 성공과 심볼 부족은 프로세스 수명 동안 보관하므로 같은 라이브러리를 시도마다 다시 열어 참조를 쌓지 않는다. 열기 실패는 핸들이 없어 보관하지 않는다.

## 실행 맥락과 조회 시점

- 제품 호출은 `create_tray_icon_inner` 안에서 `gtk_init::ensure()`가 성공한 뒤, GTK를 소유한 winit 메인 스레드에서 부팅을 시작할 때(`begin_boot`) 실행된다. `dlopen`이 실행하는 라이브러리 초기화 루틴은 `TrayIconBuilder::build`가 같은 라이브러리를 열 때와 같은 스레드 맥락에서 돈다.
- 조회 시점은 상류와 다르다. 사전 검사는 아이콘 디코딩과 빌더 호출보다 먼저, `app_indicator_new`로 객체를 만들기 전에 필수 함수 여섯 개를 모두 조회한다. 상류는 `app_indicator_new`를 부른 뒤 다음 함수를 차례로 조회하고, Tasty가 title을 주지 않아 지금은 `app_indicator_set_label`을 조회하지 않는다.
- GNU IFUNC 심볼이면 `dlsym`이 라이브러리의 resolver를 실행하고, resolver가 null을 돌려줄 수 있다(libloading-0.7.4/src/os/unix/mod.rs). 그래서 resolver가 있다면 상류보다 일찍, 또는 사전 검사에서만 실행될 수 있다. 이 검사는 AppIndicator가 객체를 만들기 전에도 함수를 조회할 수 있는 일반 공유 라이브러리라는 전제에 기댄다. aarch64 Ubuntu 24.04의 Ayatana 라이브러리는 여섯 함수를 모두 일반 FUNC 심볼로 내보낸다.
- 조회한 주소는 null과 비교만 하고 호출하거나 역참조하지 않는다. null이면 오류 여부와 관계없이 없는 함수로 센다. 상류는 받은 주소를 바로 호출하기 때문이다.
- 단위 시험은 `libc.so.6`·`libm.so.6`(이미 적재돼 새 초기화 루틴이 돌지 않는다)과 없는 이름만 열고, `cos`와 필수 함수 이름만 조회한다. GTK 조건은 시험에 적용되지 않는다.

## 상류가 바뀔 때

이 조건은 tray-icon 0.22.0, libappindicator 0.9.0, libappindicator-sys 0.9.0, libloading 0.7.4를 대조해 정했다. `system_tray.rs` 시험이 Cargo.lock의 이 판들을 고정하며, 하나라도 바뀌면 실패해 이름 순서·필수 함수·로드 플래그·null 처리를 다시 대조하게 한다.
