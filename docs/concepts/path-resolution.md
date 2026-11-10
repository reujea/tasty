# 경로 해석 (Path resolution)

한 Tasty 프로세스 안에서 사용자가 준 경로 문자열을 푸는 규칙이다. 판정하는 곳과 쓰는 곳이 서로 다른 기준으로 경로를 풀어 다른 파일을 가리키는 일을 막는다. 구현은 `tasty_utils::path_resolve`(`crates/tasty-utils/src/path_resolve.rs`) 하나다. 다른 문서는 이 규칙을 다시 적지 않고 이 문서를 링크한다.

## 계약

같은 Tasty 프로세스와 그 플러그인 프로세스 안에서, 같은 입력 문자열과 같은 해석 종류(`resolve_path`·`resolve_command`)는 언제 불러도 같은 결과(같은 경로 또는 같은 오류)를 낸다. 결과는 파일시스템을 보지 않은 어휘적 경로다. 존재·종류·실제 경로(`canonical`)와 PATH 탐색 결과(`locate_command_in`)는 호출 시점의 파일시스템에 따라 달라지며 이 계약에 들지 않는다.

## 실행 폴더

- 실행 폴더는 Tasty 프로세스가 시작될 때 OS가 준 현재 작업 폴더다. `boot::run`의 첫 문장(`init_from_process`)에서 한 번 읽어 고정하고, 프로세스가 끝날 때까지 바꾸지 않는다. 심볼릭 링크는 풀지 않는다.
- `~`가 가리키는 OS 홈도 같은 시점에 한 번 읽어 고정한다. 실행 중 `HOME`이 바뀌어도 결과는 같다.
- 시작 폴더를 읽지 못하면(지워진 폴더에서 실행 등) "없음"으로 고정하고 경고 로그를 한 줄 남긴다. 상대 입력은 매번 같은 오류(`LaunchDirUnavailable`)가 된다. 홈이나 `/` 같은 다른 폴더로 대신하지 않는다. 절대 입력과 `~`는 영향이 없다.
- 실행 방법에 따라 실행 폴더가 다르다. 터미널에서 실행하면 명령을 친 폴더, Linux GNOME 실행기에서는 실행기의 폴더(관측값 홈), Windows 시작 메뉴 바로 가기는 설치 폴더(`wix/main.wxs`의 `WorkingDirectory`)다.
- GUI 호스트는 로그 파일을 연 직후 고정한 값을 `launch dir: <경로>`(info) 한 줄로 남긴다. 읽지 못했으면 `launch dir unavailable: <원인>`(warn)이다. release 로그 파일은 warn 이상만 남기므로 info 줄은 debug 빌드 로그 파일과 `TASTY_LOG=info` stderr에서 볼 수 있다.
- CLI(`tasty <명령>`)도 같은 바이너리의 별도 프로세스라 자기 실행 폴더, 곧 명령을 친 셸의 폴더를 쓴다. CLI는 경로 인자를 자기 실행 폴더 기준으로 절대화해 보내고, 실행 중인 Tasty는 CLI가 보낸 상대 경로를 자기 실행 폴더로 다시 풀지 않는다. mirror 대상의 `--file`은 원격이 풀도록 `file_base`를 함께 보낸다. 지금 이 규칙을 따르지 않는 인자는 `--path`(explorer, 호스트가 상대 값을 버리고 홈을 연다)와 `--url`(html, 상대 값이 `file:///<값>`이 된다)이다.
- 플러그인 프로세스의 cwd는 설치 폴더다. 플러그인용 `init_from_env`는 `TASTY_LAUNCH_DIR` 환경변수로 실행 폴더를 받고, 변수가 없거나 비었거나 상대 경로면 "없음"으로 고정한다(설치 폴더로 대신하지 않는다). 지금 호스트는 이 변수를 넣지 않고 플러그인 SDK도 `init_from_env`를 부르지 않으므로, 플러그인이 직접 푸는 상대 경로는 아직 설치 폴더 기준이다.

## 파일·폴더 경로 (`resolve_path`)

| 입력 | 결과 |
|---|---|
| `""` | `Empty` 오류 |
| NUL 포함 | `Invalid` 오류 |
| `scheme://…`(scheme 두 글자 이상, `file://` 포함) | `NotLocalPath` 오류. 원격·URL은 호출자가 먼저 가른다 |
| `~`, `~/x`(Windows는 `~\x`도) | 고정한 홈 기준. 홈을 모르면 `HomeUnavailable` 오류 |
| `~user`, `~x` | 일반 상대 이름(실행 폴더 기준) |
| 절대 경로 | 그대로 |
| 상대 경로(`./x`, `../y/x`, `rel/x`, `a.md`, `.`, `..`) | 실행 폴더 기준 |

- 구분자가 없는 이름(`a.md`)도 실행 폴더 기준이다. PATH에서 찾지 않는다.
- 결과에서 `.`만 지우고 `..`는 남긴다. `link/../x`를 어휘적으로 접으면 OS가 심볼릭 링크를 따라 푼 결과와 달라지기 때문이다. 판정과 사용이 같은 문자열을 OS에 넘기면 OS가 `..`와 링크를 같은 방식으로 푼다.
- 심볼릭 링크를 푼 실제 경로가 필요한 곳(보안 경계 검사, 같은 파일 판정)은 결과를 `canonical`에 넘긴다. 없는 경로는 오류다.
- 표시·중복 제거용으로 `..`까지 접을 때는 `tasty_utils::path::lexically_normalize`를 쓴다.
- 기준을 명시하는 판(`resolve_path_against`)은 플러그인 설치 폴더 기준 entry, 문서 폴더 기준 링크처럼 일부러 다른 기준을 쓰는 곳을 위한 것이다. 규칙은 같고 기준만 다르다. 기준이 상대 경로면 먼저 실행 폴더 기준으로 푼다.

## 실행 명령 (`resolve_command`, `locate_command_in`)

셸처럼 프로그램을 띄우려고 찾는 값에만 적용한다.

- 구분자가 없으면 이름으로 보고 PATH에서 찾는다(`Search`). 구분자가 있으면 `resolve_path`와 같은 규칙으로 실행 폴더 기준 절대 경로다(`Path`). `/`는 모든 OS의 구분자다. Windows는 `\`와 드라이브 접두(`C:`)도 구분자로 본다. macOS·Linux의 `\`는 파일 이름 문자라 `a\b`는 이름이다.
- `~`, `~/…`, `.`, `..`는 구분자가 없어도 폴더를 뜻하므로 경로다. `~name`은 이름이다.
- `locate_command_in`은 `Path`면 그대로, `Search`면 넘겨받은 PATH를 앞에서부터 보고 첫 실행 파일의 절대 경로를 돌려준다. 디렉터리와 실행 권한 없는 파일(macOS·Linux는 `access(X_OK)`)은 건너뛴다. 없으면 `NotFoundInPath`이며 실행 폴더의 같은 이름 파일로 대신하지 않는다.
- PATH의 상대 항목과 빈 항목은 실행 폴더 기준이다. POSIX `execvp`는 자식의 작업 폴더 기준으로 보지만, 그러면 같은 설정이 터미널마다 다른 파일을 가리키고 터미널이 없는 판정(설정 창, 부팅)과 기준이 갈린다.
- Windows는 이름에 확장자가 없으면 `.exe`·`.cmd`·`.bat`·`.com`을 차례로 붙여 찾는다.
- 판정하는 곳과 실행하는 곳은 PATH 탐색 결과 하나를 함께 쓴다. 찾은 절대 경로를 실행에 넘기면 portable-pty가 PATH를 다시 찾지 않는다.

## Windows

규칙은 `crates/tasty-utils/src/path_resolve/windows.rs`에 있다.

| 입력 | 결과 |
|---|---|
| `C:\a`, `C:/a\b` | 절대 경로. 구분자는 `\`로 통일한다(`C:\a\b`) |
| `\\srv\share\a` | UNC 절대 경로 |
| `\\?\C:\a`, `\\?\UNC\srv\share\a` | verbatim 접두를 벗긴 `C:\a`, `\\srv\share\a` |
| `/c`, `/c/a`(한 글자 드라이브) | MSYS 형식을 `C:\`, `C:\a`로 바꾼다 |
| `\a`, `/a`, `/usr/bin/bash` | `RootRelative` 오류. 현재 드라이브에 붙이면 MSYS 마운트를 다른 파일로 잘못 푼다 |
| `C:a`, `C:` | `DriveRelative` 오류. 드라이브별 현재 폴더는 고정할 수 없는 프로세스 상태다 |
| 그 밖의 상대 경로 | 실행 폴더 기준 |

- 대소문자는 바꾸지 않는다. verbatim 접두를 벗기므로 260자를 넘는 경로는 일부 API에서 실패할 수 있다.

## 완료 조건

- Given 실행 폴더 `D`, When `resolve_path`에 `./x`, `../y/x`, `rel/x`, `a.md`를 주면, Then `D/x`, `D/../y/x`, `D/rel/x`, `D/a.md`를 돌려준다.
- Given PATH `/opt/x:/usr/bin`에 실행 가능한 `/opt/x/zsh`가 있고 실행 폴더에도 `zsh`가 있다, When `zsh`를 실행 명령으로 해석해 찾으면, Then `/opt/x/zsh`다.
- Given PATH 어디에도 `zsh`가 없다, When 찾으면, Then `NotFoundInPath`이고 실행 폴더의 같은 이름 파일을 돌려주지 않는다.
- Given PATH에 상대 항목 `rel`, When `sh`를 찾으면, Then `D/rel/sh`를 본다.
- Given 같은 입력, When 다른 스레드에서 부르거나 그 사이 대상 파일을 만들고 지우면, Then 절대화 결과는 같다.
- Given 지워진 폴더에서 시작한 Tasty, When 상대 입력을 풀면, Then 패닉 없이 `LaunchDirUnavailable` 오류이고 절대 입력은 그대로 풀린다.
