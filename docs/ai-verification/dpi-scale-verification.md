# DPI 배율(scale factor) 검증

DPI≠1 환경을 Xvfb 에서 재현해, 논리↔물리 변환이 실제로 맞는지 확인하는 절차.

DPI scale factor는 논리 픽셀을 물리 픽셀로 바꾸는 배율이고, `AppearanceSettings.ui_scale`은 UI 토큰 크기에 적용하는 배율이다. 두 값이 동시에 영향을 주므로 원인을 구분할 수 있게 한 번에 하나만 바꾼다.

## 배율을 거는 법

winit 이 X11 환경변수를 직접 읽는다. 코드 변경도 설정도 필요 없다.

```bash
WINIT_X11_SCALE_FACTOR=2 <바이너리>
```

## 걸렸는지 무엇으로 아는가 — 신호를 먼저 정한다

설정값이 적용됐는지와 창 크기가 실제로 바뀌었는지를 함께 확인한다. debug 전용 `debug.fullscreen.state`가 두 값을 제공한다.

| 신호 | 답하는 질문 | 배율 1 | 배율 2 |
|---|---|---|---|
| `monitor.scale_factor` | winit 이 배율을 **받았는가** | `1.0` | `2.0` |
| `inner_size`(물리 px) | 그것이 **효과를 냈는가** | `1280×720` | `2560×1440` |
| `monitor.size` | (대조) X 화면 자체는 안 변해야 한다 | 화면 그대로 | 화면 그대로 |

```bash
tasty debug fullscreen state --window-id <ID>
```

`inner_size`는 winit `Window::inner_size()`가 반환하는 물리 픽셀 크기다. 같은 논리 크기의 창을 비교하면 배율에 비례해 커져야 한다. `monitor.size`까지 바뀌었다면 화면 설정도 달라진 것이므로 같은 조건의 배율 비교가 아니다.

## 실행 절차

1. **gui 조합으로 다시 빌드한다.** `cargo test --no-default-features` 를 돌린 적이
   있으면 `target/debug/tasty` 가 헤드리스본으로 덮여 있다. `cargo build --bin tasty`.
2. **바이너리가 맞는지 관측 가능한 값으로 확인한다.** `--version` 으로는 두 조합이
   안 갈린다. `tasty list windows` 의 **창 수가 1 이상**인 것으로 확인한다 — 헤드리스본은
   창을 만들지 않는다.
3. **격리 `TASTY_HOME` 으로 띄운다.** 사용자 인스턴스를 건드리지 않기 위해서다.
   화면 크기는 아래 "화면이 창보다 커야 한다" 를 따른다.
4. **CLI 환경에서 부모의 `TASTY_*` 를 전부 뗀다** — 특히 `TASTY_SESSION_TOKEN`.
   그것은 사용자 인스턴스의 토큰이라 격리본이 거부하고, 증상은
   `permission_denied: session_token unknown/expired/revoked` 다. 배율과 무관한 실패라
   여기서 막히면 원인을 엉뚱한 데서 찾게 된다.
5. **배율 없이 한 번, 배율을 걸고 한 번** 띄워 위 표의 신호를 각각 읽는다.
6. **좌표를 IPC로 읽을 수 있으면 먼저 확인한다.** 픽셀은
   "몇 px 인가" 만 답하고 "논리 몇인가" 는 안 답해서, 배율이 걸린 값에서 두 가설
   (토큰이 논리라 정확히 배수인가 / 콘텐츠가 정하는 치수라 스냅됐는가)을 못 가른다.

   | 표면 | 명령 | 무엇이 나오는가 |
   |------|------|-----------------|
   | popup | `tasty debug host-popup list` | `rect`(논리) · `z_seq` |
   | banner | `tasty debug banner list` | `shown[].rect`(논리) · `shown[].content_rect`(물리, plugin mesh 만) · `coords` |

   두 응답의 `rect`는 같은 논리 좌표계다. plugin mesh 배너의 `content_rect`는 물리 좌표이며 응답의 `coords`에서 확인한다. host egui가 그리는 바깥 영역과 plugin이 ppp를 적용해 그린 콘텐츠를 구분하기 위한 값이다.

   **배너의 `rect` 는 한 프레임 늦다** — 배너는 popup 과 달리 좌표를 모델에 들고 있지
   않고 컨테이너가 매 프레임 배치하므로, 뜬 직후 첫 프레임에는 `null` 이다. `show`
   직후가 아니라 한 프레임 뒤에 읽는다.

   2026-09-05 Linux의 mouse-capture/view 배너에서는 논리 높이가 배율 1일 때 50.0, 배율 2일 때 49.5였다(물리 50·99px). 콘텐츠가 정한 높이를 물리 픽셀에 맞춘 뒤 논리값으로 되돌린 결과다. 스크린샷만 보면 토큰 배율 오류와 구분하기 어려우므로 논리 좌표도 함께 확인한다.

7. 대상 화면(webview·banner·popup mesh·탭바 높이·네이티브 메뉴 좌표)의 좌표·크기를
   두 배율에서 비교한다. 캡처 방법과 Xvfb 함정은
   [screenshot-methods](screenshot-methods.md) 를 따르고, 캡처 전에 포인터를 한 번
   움직여 재렌더를 유발한다. **대상마다 캡처가 갈린다** — banner·popup mesh·탭바·메뉴는
   `screenshot --window` 로 나오지만 **webview 는 그 캡처에 안 담긴다**(swapchain 밖의
   OS 자식 창이라 host chrome 만 찍힌다). webview 좌표를 재려면 OS 화면 캡처를 쓴다.
8. 정리는 **저장한 PID** 로 한다. `xvfb-run` 을 쓰면 `$!` 는 래퍼이므로, 안의 프로세스는
   `/proc/<pid>/environ` 의 격리 `TASTY_HOME` 으로 찾는다. 패턴 매칭으로 죽이지 않는다.

## 화면이 창보다 커야 한다

배율 2 에서 창의 물리 크기는 `2560×1440` 이다. 화면이 `1600x1200` 이면 창이 화면을
넘어 잘리고, 그 잘림이 배율 결함처럼 보인다. **화면을 창의 물리 크기보다 크게 잡는다**
(예 `3200x2400`). 화면을 키워도 `inner_size` 는 `2560×1440` 로 같다 — 창 크기는 화면이
아니라 논리 크기와 배율이 정한다.

## 알려진 실측값

기준 화면 `1600x1200`(6번 항목의 비교에는 `3200x2400`), 기본 창.

| 조건 | `scale_factor` | `inner_size` | 창 수 |
|---|---|---|---|
| 지정 없음 | `1.0` | `1280×720` | 1 |
| `WINIT_X11_SCALE_FACTOR=2` | `2.0` | `2560×1440` | 1 |

두 신호가 **함께** 움직이므로 배율이 적용됐고 효과도 났다. 하나만 움직였다면 그
자체가 결함 신호다.

## 네이티브 메뉴 앵커는 winit 배율 == GDK 배율을 전제한다

`WINIT_X11_SCALE_FACTOR` 는 **winit 만** 움직인다. Linux 네이티브 컨텍스트 메뉴는
GTK3 가 띄우고(`crates/tasty-platform/src/native_menu/linux.rs`), GDK 는 배율을 `GDK_SCALE` 에서
따로 읽는다. 앵커 좌표는 winit(=egui) 논리 좌표로 넘어가는데 GTK 는 같은 수를
GDK 논리 좌표로 읽으므로, **두 배율이 같을 때만** 메뉴가 클릭 지점에 뜬다.

실측(Xvfb `3200x2400`, 창 기본 크기, 실제 `xdotool` 우클릭):

| winit 배율 | `GDK_SCALE` | 클릭(물리) | 메뉴 창 기하 | 판정 |
|---|---|---|---|---|
| 1 | 미지정(=1) | `(250,48)` | `184x172+250+48` | 앵커 일치 |
| 2 | 미지정(=1) | `(500,96)` | `184x172+250+48` | 위치·크기 모두 1 배 — 어긋남 |
| 2 | `2` | `(500,96)` | `368x344+500+96` | 앵커 일치 |

이 환경의 메뉴 위치 차이는 winit과 GDK의 배율을 서로 다르게 설정해서 생겼다. 배율 2로 메뉴를 검사할 때는 `GDK_SCALE=2`도 지정한다. 두 배율이 다른 결과를 실제 기기의 HiDPI 동작 근거로 사용하지 않는다.

전제가 깨진 순간은 `show_context_menu` 를 부르기 직전에
`warn_if_menu_anchor_scale_premise_broken` 이 `tracing::warn!` 한 줄로 남긴다.
같은 배율 조합에 대해 한 번만 남으므로 우클릭 횟수에 비례하지 않는다.

`WINIT_X11_SCALE_FACTOR`와 `GDK_SCALE`은 Xvfb에서 배율을 만드는 수단이다. GNOME 세션에서 컴포지터가 주는 값은 [Linux 디스플레이 백엔드](../design/systems/linux-display-backend.md#xwayland-경로의-배율)에 있다. 그 환경에서는 winit과 GDK가 컴포지터 설정에서 같은 배율을 받고, X 입력(`xdotool`, XTEST)으로 연 메뉴가 클릭 지점에 뜬다. 배율 2와 논리 레이아웃의 실제 포인터 경로는 미측정이다.

## 관련

- [screenshot-methods](screenshot-methods.md) — 캡처 수단과 Xvfb 함정
- [screenshot-methods › 시각 판정 체크리스트](screenshot-methods.md#시각-판정-체크리스트) — 시각 판정 휴리스틱
- [`docs/concepts/typed-length.md`](../concepts/typed-length.md) — 이 검증이 지키려는 정책
- [docs/concepts/typed-length.md](../concepts/typed-length.md) — 정적 축의 현재 결정(생성자를 봉인하지 않는다)
