use serde::{Deserialize, Serialize};

/// 휠 1노치가 스크롤하는 논리 포인트의 기본값 — [`GeneralSettings::wheel_line_scroll`]
/// 의 유일한 정의처다.
///
/// 50 인 이유는 egui 기본값(native 40 / web 8)과 견주어 고른 것이 아니라 이 코드베이스가
/// plugin 표면에 이미 쓰던 값을 보존하는 것이다. egui 는 그 두 값이 왜 달라야 하는지
/// 자기 소스에 미결 표시로 남겨 두었으므로 기준이 될 수 없다. 근거 전문은 ADR-0130.
pub const DEFAULT_WHEEL_LINE_SCROLL: f32 = 50.0;

/// 빌트인 bashrc 의 *전반부* — LANG/LC_ALL, MSYS PATH, `__tasty_osc7` 함수 정의.
/// 모드와 무관하게 합성 rc 의 *맨 앞* 에 prepend 된다. 사용자 rc 가 이후에 source 된다.
pub const BUILTIN_BASHRC_PRE: &str = r#"# === tasty built-in (auto-generated, do not edit) ===
# This section is regenerated every time settings are saved.
# Put your customizations in ~/.tasty/bashrc.user instead.

# UTF-8
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8

# Inherit Windows PATH
ORIGINAL_PATH="${ORIGINAL_PATH:-${PATH}}"
export PATH="/usr/local/bin:/usr/bin:/bin:${ORIGINAL_PATH}"

# Emit OSC 7 so Tasty inherits cwd when opening new tabs/splits.
# Only re-emits when PWD actually changes, and avoids a cygpath fork by
# converting MSYS drive paths (/c/...) to Windows form (/C:/...) in pure bash.
# Virtual MSYS mounts (/tmp, /usr, ...) fall back to cygpath, which is rare.
__tasty_osc7() {
    [[ "$PWD" == "$__TASTY_LAST_PWD" ]] && return
    __TASTY_LAST_PWD="$PWD"
    local pwd_emit="$PWD"
    if [[ "$pwd_emit" =~ ^/([a-zA-Z])/(.*)$ ]]; then
        pwd_emit="/${BASH_REMATCH[1]^^}:/${BASH_REMATCH[2]}"
    elif [[ "$pwd_emit" =~ ^/([a-zA-Z])$ ]]; then
        pwd_emit="/${BASH_REMATCH[1]^^}:"
    elif command -v cygpath >/dev/null 2>&1; then
        pwd_emit=$(cygpath -w "$PWD" 2>/dev/null || printf '%s' "$PWD")
        pwd_emit=${pwd_emit//\\//}
        [[ "$pwd_emit" == [A-Za-z]:* ]] && pwd_emit="/${pwd_emit}"
    fi
    printf '\033]7;file://%s%s\033\\' "${HOSTNAME:-localhost}" "$pwd_emit"
}

# Emit OSC 0 (window title) with a cwd-derived name so tab titles show
# something meaningful instead of the ConPTY default (the shell exe path).
# Naming mirrors Tab::refresh_display_name: ~ for $HOME, / for root,
# basename otherwise. Re-emits only when PWD actually changes.
__tasty_title() {
    [[ "$PWD" == "$__TASTY_TITLE_LAST_PWD" ]] && return
    __TASTY_TITLE_LAST_PWD="$PWD"
    local name
    if [[ "$PWD" == "$HOME" ]]; then
        name="~"
    elif [[ "$PWD" == "/" ]]; then
        name="/"
    else
        name="${PWD##*/}"
    fi
    printf '\033]0;%s\007' "$name"
}

# OSC 133 (docs/features/terminal-output/index.md#명령-인덱싱-osc-133) — command-boundary reporting so the host can index
# command/exit-code records (surface.commands IPC). D reports the command
# that just finished (using $?), A announces a fresh prompt; both fire from
# PROMPT_COMMAND, D first — $? MUST be captured as this function's very
# first statement, since it reflects the last foreground pipeline's status
# only until something else runs (see BUILTIN_BASHRC_PROMPT: this function
# is prepended first in the PROMPT_COMMAND chain for exactly this reason).
__tasty_osc133_precmd() {
    local ec=$?
    printf '\033]133;D;%s\033\\' "$ec"
    printf '\033]133;A\033\\'
}

# C: about to execute a command. bash has no `preexec` hook (that's a zsh
# feature); the closest equivalent is `PS0` (bash 4.4+), which is expanded
# and displayed right after a command line is read, before it executes.
# bash < 4.4 (e.g. macOS's system bash 3.2) has no PS0 and is unsupported —
# no DEBUG-trap fallback is added (PS0-only; see docs/features/terminal-output/index.md#명령-인덱싱-osc-133).
#
# NOTE: the command text (`cmd=` payload) is intentionally omitted here.
# The primary reason is independent of bash: tasty's current OSC133 C-phase
# parser (`FinalTermSemanticPrompt::MarkEndOfInputAndStartOfOutput` arm in
# crates/tasty-terminal/src/vte_handler/osc.rs; see docs/features/terminal-output/index.md#명령-인덱싱-osc-133) unconditionally
# discards C's payload for every shell, bash and zsh alike — so a `cmd=`
# emitted here would never reach `command_index.rs` regardless of how it's
# obtained. Secondarily, and only relevant if that parser is later extended
# to preserve C's payload: PS0-evaluation-time command-text lookup in bash is
# inconsistent across candidates — `history 1` was verified (manual PTY test,
# manual PTY test) to correctly reflect the about-to-run command, but `$BASH_COMMAND`
# and `fc -ln -1` were verified to lag one command behind at that point (bash
# hasn't finished recording the about-to-run command yet). Left omitted for
# now rather than wiring up `history 1` for a payload the parser would just
# discard (OSC133 payload is optional by spec; `command_index.rs` already
# tolerates a missing command string).
__tasty_osc133_preexec() {
    printf '\033]133;C\033\\'
}
"#;

/// 빌트인 bashrc 의 *후반부* — `PROMPT_COMMAND`/`PS0` 설정. 합성 rc 의 *맨 뒤* 에
/// append 되어 사용자 rc 가 PROMPT_COMMAND/PS0 를 덮어쓰더라도 빌트인 훅이 마지막에
/// 이긴다(PROMPT_COMMAND 는 prepend, PS0 는 통째로 덮어씀 — OSC133 C phase 는
/// 선택적 기능이 아니라 필수 배선이라 사용자 PS0 커스터마이즈보다 우선한다).
pub const BUILTIN_BASHRC_PROMPT: &str = r#"PROMPT_COMMAND="__tasty_osc133_precmd;__tasty_osc7;__tasty_title${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
PS0='$(__tasty_osc133_preexec)'

# === end tasty built-in ===
"#;

/// 합성 bashrc 버전 스탬프. `compose_*_bashrc` 가 출력 맨 앞에 심고,
/// `ensure_compiled_bashrc_in` 가 기존 파일에서 이 줄이 정확히 일치하지 않으면
/// (스탬프 없음 = 구버전 포함) 강제 재생성한다 — "빠진 파일만 채우는" 기존
/// 동작으로는 빌트인 블록 변경이 기존 설치본에 반영되지 않기 때문.
///
/// **빌트인 블록(`BUILTIN_BASHRC_PRE`/`BUILTIN_BASHRC_PROMPT`) 내용을 바꿀
/// 때마다 숫자를 +1 할 것.** (v1 = 스탬프 도입 전 무표기 세대, v3 = OSC133 훅
/// 추가.)
pub const BUILTIN_BASHRC_STAMP: &str = "# tasty-bashrc-v3";

/// `~/.tasty/bashrc.user`의 초기 시드. 사용자가 자유롭게 수정/리셋할 수 있는 기본값.
pub const INITIAL_USER_BASHRC: &str = r#"# Tasty user bashrc — edit freely.
# Tasty prepends a built-in block (OSC 7 emission etc.) automatically; no need
# to include those here.

# Prompt
PS1='\[\033[32m\]\u@\h\[\033[0m\] \[\033[33m\]\w\[\033[0m\]\n\$ '

# Common aliases
alias ls='ls --color=auto'
alias ll='ls -la'
alias grep='grep --color=auto'
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub shell: String,
    /// Shell startup mode: "default" or "tasty". Windows 전용 — OSC7/MSYS PATH
    /// 빌트인을 prepend 할지 여부를 결정한다. 비-Windows 에서는 셸 모드 자체가
    /// 의미가 없어 노출하지 않는다.
    #[cfg(windows)]
    pub shell_mode: String,
    pub startup_command: String,
    pub language: String,
    /// Number of scrollback lines to keep.
    pub scrollback_lines: usize,
    /// Show confirmation dialog when closing a surface with a running process.
    pub confirm_close_running: bool,
    /// Enable click-to-move-cursor: clicking on the editable region moves the
    /// shell cursor to that position.
    pub click_to_move_cursor: bool,
    /// When creating a new pane/surface/workspace, inherit the working directory
    /// from the source surface.
    pub inherit_cwd: bool,
    /// Behavior when closing the last window: "ask", "minimize", "quit".
    pub close_behavior: String,
    /// Save and restore layout (workspaces, panes, tabs) on restart.
    pub restore_layout: bool,
    /// Restore surface content on restart (requires `restore_layout`). 현재 보존되는
    /// "내용"은 터미널 scrollback 이며, 옵션명만 surface 일반으로 넓힌 것이다(메커니즘 동일).
    /// `alias`: 구버전 settings.toml 의 `restore_terminal_content` 키를 계속 읽는다.
    #[serde(alias = "restore_terminal_content")]
    pub restore_surface_content: bool,
    /// 휠 1노치(= 데스크톱 마우스 휠 한 칸)가 스크롤하는 논리 포인트. 이 값 하나가
    /// host UI 위젯과 plugin 표면 **양쪽**에 걸린다 — egui `Options::line_scroll_speed`
    /// 로 밀어 넣으면 host 의 `ScrollArea` 가 그것으로 스크롤하고, 휠을 포인트로 바꾸는
    /// plugin 경로도 같은 옵션을 읽는다(ADR-0130). 기본값
    /// [`DEFAULT_WHEEL_LINE_SCROLL`]. 스크롤 속도가 접근성 축의 조정 요구가 잦은 값이라
    /// 노출하지만, 접근성 전용이 아니라 마우스 동작 설정이므로 이 절에 둔다.
    pub wheel_line_scroll: f32,
    /// 터미널 내 링크 클릭 시 요구되는 수식키. "ctrl" | "alt" | "none".
    /// "none"이면 평범한 클릭으로 링크가 열리므로 텍스트 선택과 구분되지 않는 점에 유의.
    pub link_click_modifier: String,
    /// Allow programs in the terminal to read the system clipboard via OSC 52
    /// (`OSC 52 ; c ; ? ST`). Off by default: clipboard read lets any program —
    /// including untrusted remote/SSH processes — silently exfiltrate clipboard
    /// contents (passwords, tokens). When off, read queries get no reply.
    pub allow_clipboard_read: bool,
    /// Render DECSCNM (DEC private mode 5, "reverse screen"): when a program
    /// sends `\e[?5h`, the whole viewport's default fg/bg are swapped. Some
    /// setups abuse this as a visible bell (readline `bell-style visible` emits
    /// the terminfo `flash` = a DECSCNM toggle), producing a jarring full-screen
    /// flash. When this is off, the mode flag is still tracked (queries answer
    /// correctly) but the renderer does NOT apply the swap, so the flash is
    /// suppressed. On by default (spec-compliant). See `screen_reverse()` on the
    /// terminal and the render gate in `render_terminals`.
    pub reverse_screen_enabled: bool,
    /// Show a notification (and play the notification sound, subject to the
    /// global notification gates) when the terminal receives a BEL (`\a`). When
    /// off, BEL no longer raises the bell toast (title `t("notification.bell_title")`)
    /// — but user-registered `bell`
    /// hooks STILL fire, since a hook is explicit automation the user opted into,
    /// not a passive reaction. On by default. Gated in `cascade_terminal_bell_ring`
    /// on top of the global `notification.enabled`.
    pub bell_notification: bool,
    /// 마우스 트래킹 앱(vim/htop 등) 위에서 처음 좌/우 클릭할 때, 마우스가 앱에 캡처
    /// 중이며 텍스트 선택은 Shift+드래그, tasty 메뉴는 Shift+우클릭으로 띄울 수 있음을
    /// 안내하는 toast 를 트래킹 세션당 1회 표시한다(발견성, ADR-0022 ②). off 면 안내하지
    /// 않는다. `alias`: 구버전 settings.toml 의 `right_click_capture_hint` 키를 계속 읽는다.
    #[serde(alias = "right_click_capture_hint")]
    pub mouse_capture_hint: bool,
    /// 마우스 캡처 비활성화 블랙리스트 — foreground 프로세스 이름 패턴 목록.
    /// 여기에 매칭되는 TUI 가 surface 의 foreground 일 때, 그 surface 에서는 클릭/
    /// 드래그/버튼 캡처를 끄고(좌클릭=선택, 우클릭=tasty 메뉴 등 로컬 처리) 앱에
    /// 버튼 보고를 보내지 않는다. **휠은 예외로 계속 앱에 보고된다.** 패턴은 `.exe`
    /// 제거·소문자화 후 substring 매칭(또는 `*` 와일드카드)이며, 빈 목록(기본값)이면
    /// 어떤 surface 도 비활성화하지 않는다.
    pub mouse_capture_blacklist: Vec<String>,
    /// 마우스 캡처 **안내 배너만** 억제하는 블랙리스트 — foreground 프로세스 이름 패턴
    /// 목록. `mouse_capture_blacklist`(캡처 자체를 끔)와 독립적인 별개 축이다: 여기
    /// 매칭되는 TUI 가 surface 의 foreground 일 때는 캡처(클릭/드래그 앱 위임)는 평소대로
    /// 유지하되 "마우스 캡처 중 — Shift 로 우회 가능" 안내 배너만 표시하지 않는다.
    /// 매칭 규칙은 `mouse_capture_blacklist` 와 동일(`.exe` 제거·소문자화 후 substring
    /// 또는 `*` 와일드카드). 빈 목록(기본값)이면 어떤 surface 도 억제하지 않는다.
    pub mouse_capture_banner_blacklist: Vec<String>,
    /// 워크스페이스 카테고리(사이드바 폴더) 계층을 활성화한다. off(기본)면 사이드바·
    /// 단축키·영속이 현행 평면 동작과 동일하다. on 으로 켜면 normal 외 사용자
    /// 카테고리를 만들 수 있고, off 로 끄면 모든 워크스페이스를 normal 로 귀속한다.
    pub workspace_categories_enabled: bool,
    /// "다음/이전 워크스페이스" 전환(quick-switch)이 활성 카테고리의 경계에서 같은
    /// 카테고리 안으로 wrap-around 하는 대신 인접 카테고리로 넘어간다. off(기본)면
    /// 카테고리 로컬 wrap 을 유지한다(현행 동작). on 이면 카테고리 마지막 워크스페이스에서
    /// "다음" → 다음 카테고리의 첫 워크스페이스로, 카테고리 첫 워크스페이스에서 "이전" →
    /// 이전 카테고리의 마지막 워크스페이스로 이동하며, 카테고리 목록 자체도 wrap 한다.
    /// 카테고리가 1개뿐이면(`workspace_categories_enabled` off 포함) on 이어도 기존
    /// 로컬 wrap 과 동일하게 동작한다.
    pub workspace_switch_crosses_category: bool,
    /// Explorer 의 마지막으로 선택한 view mode ("grid" | "list" | "detail"). 사용자가
    /// 툴바 segmented 로 형태를 바꾸면 여기에 기록되고, 새로 생성되는 explorer
    /// surface·내부 탭의 기본 표시 형태로 쓰인다. 알 수 없는 값은
    /// `ExplorerViewMode::from_str` 이 detail 로 fallback (normalize 도 detail 로 교정).
    pub explorer_view_mode: String,
    /// macOS 전용 — Option 키를 Meta(Alt-prefix) 키로 해석한다. on 이면 Option+문자가
    /// 특수문자(compose) 대신 `ESC` + base 문자 시퀀스로 PTY 에 전달돼 readline/Emacs/
    /// vim 의 Meta 바인딩(`Alt+f`/`Alt+b` 등)이 동작한다. 기본 off — 기존 Option=특수문자
    /// 동작을 보존한다(iTerm2 도 기본 off). 다른 OS 에는 Option 키가 없어 노출하지 않는다.
    #[cfg(target_os = "macos")]
    pub option_as_meta: bool,
    /// macOS 전용 — Full Disk Access 안내를 부팅 시 이미 한 번 띄웠는가. 안내는
    /// 평생 1 회만 뜨고, 이 값을 다시 false 로 되돌리는 것은 설정 화면의 토글이다.
    /// 다른 OS 에는 Full Disk Access 개념이 없어 노출하지 않는다.
    #[cfg(target_os = "macos")]
    pub macos_fda_notice_shown: bool,
    /// 단축키 표시 시 `alt` 토큰의 텍스트: "alt" | "cmd" | "symbol"(⌘). 저장 포맷은
    /// OS 독립 추상 토큰(`"alt+n"`)을 유지하고(`docs/design/policies/key-mapping.md`
    /// 저장↔표시 분리 원칙), 이 필드는 화면 표시 레이어만 바꾼다
    /// (`KeybindingSettings::format_display_parts` 가 소비). macOS 에서 물리적으로는
    /// Cmd 키에 매핑되지만, 크로스플랫폼 필드로 두고 UI(설정 > 일반 > 표시)는 macOS
    /// 에서만 노출한다 — 다른 OS 는 Alt 키/글리프 개념이 없어 값이 있어도 의미가 없고,
    /// 항상 fallback(alt)로 조회된다. 기본값 "alt" — 기존 사용자에게 표시 변화 없음.
    pub alt_display_style: String,
    /// 단축키 표시 시 `option` 토큰의 텍스트: "option" | "symbol"(⌥). `alt_display_style`
    /// 참고 — 저장↔표시 분리, macOS 전용 UI, 크로스플랫폼 필드. 기본값 "option".
    pub option_display_style: String,
    /// 단축키 표시 시 `shift` 토큰의 텍스트: "shift" | "symbol"(⇧). `alt_display_style`
    /// 참고 — 저장↔표시 분리, macOS 전용 UI, 크로스플랫폼 필드. 기본값 "shift".
    pub shift_display_style: String,
}

/// 파싱된 링크 클릭 수식키.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkModifier {
    Ctrl,
    Alt,
    None,
}

impl LinkModifier {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Self::Ctrl,
            "alt" | "option" => Self::Alt,
            "none" | "off" | "" => Self::None,
            _ => Self::Ctrl,
        }
    }

    /// 현재 modifier 상태가 링크 클릭 트리거 조건을 만족하는지.
    /// 호출자가 winit/macos 등 GUI 레이어에서 추출한 boolean을 그대로 넘긴다.
    pub fn matches(&self, ctrl: bool, alt: bool, super_key: bool) -> bool {
        match self {
            Self::Ctrl => ctrl && !alt && !super_key,
            Self::Alt => alt && !ctrl && !super_key,
            Self::None => true,
        }
    }
}

impl Default for GeneralSettings {
    fn default() -> Self {
        let shell = Self::detect_shell();
        Self {
            shell,
            #[cfg(windows)]
            shell_mode: "default".to_string(),
            startup_command: String::new(),
            language: "en".to_string(),
            scrollback_lines: 10000,
            confirm_close_running: true,
            click_to_move_cursor: true,
            inherit_cwd: true,
            close_behavior: "ask".to_string(),
            restore_layout: true,
            restore_surface_content: true,
            wheel_line_scroll: DEFAULT_WHEEL_LINE_SCROLL,
            link_click_modifier: "ctrl".to_string(),
            allow_clipboard_read: false,
            reverse_screen_enabled: true,
            bell_notification: true,
            mouse_capture_hint: true,
            mouse_capture_blacklist: Vec::new(),
            mouse_capture_banner_blacklist: Vec::new(),
            workspace_categories_enabled: false,
            workspace_switch_crosses_category: false,
            explorer_view_mode: "detail".to_string(),
            #[cfg(target_os = "macos")]
            option_as_meta: false,
            #[cfg(target_os = "macos")]
            macos_fda_notice_shown: false,
            alt_display_style: "alt".to_string(),
            option_display_style: "option".to_string(),
            shift_display_style: "shift".to_string(),
        }
    }
}

/// `*` 와일드카드만 지원하는 소형 glob 매칭 (정규식/`?` 미지원). 패턴·대상 모두
/// 호출 전 소문자/`.exe` 정규화돼 있다고 가정한다. `*` 는 0개 이상의 임의 문자에
/// 대응하며, 앞/뒤/중간 어디에 와도 된다.
fn glob_match(pattern: &str, text: &str) -> bool {
    // 패턴을 `*` 로 분할한 리터럴 조각들을 순서대로 text 에서 소비한다.
    let parts: Vec<&str> = pattern.split('*').collect();
    let starts_wild = pattern.starts_with('*');
    let ends_wild = pattern.ends_with('*');

    let mut cursor = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let is_first = i == 0;
        let is_last = i == parts.len() - 1;
        if is_first && !starts_wild {
            // 첫 리터럴은 text 시작에 고정.
            if !text[cursor..].starts_with(part) {
                return false;
            }
            cursor += part.len();
        } else if is_last && !ends_wild {
            // 마지막 리터럴은 text 끝에 고정.
            if !text[cursor..].ends_with(part) {
                return false;
            }
            cursor = text.len();
        } else {
            // 중간 리터럴: 다음 출현 위치를 찾아 소비.
            match text[cursor..].find(part) {
                Some(off) => cursor += off + part.len(),
                None => return false,
            }
        }
    }
    true
}

/// `list` 에 `fg_name` 이 매칭되는지 판정하는 공용 헬퍼 — 마우스 캡처 블랙리스트와
/// 배너 억제 블랙리스트가 동일한 매칭 규칙(소문자화·`.exe` 제거 후 substring 또는
/// `*` glob)을 공유하므로 여기서 한 번만 구현한다. 빈 리스트면 항상 false.
fn matches_blacklist(list: &[String], fg_name: &str) -> bool {
    if list.is_empty() {
        return false;
    }
    let lower = fg_name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    list.iter().any(|pat| {
        let pat = pat.trim().to_ascii_lowercase();
        if pat.is_empty() {
            return false;
        }
        if pat.contains('*') {
            glob_match(&pat, stem)
        } else {
            stem.contains(&pat)
        }
    })
}

impl GeneralSettings {
    /// foreground 프로세스 이름이 마우스 캡처 블랙리스트에 걸리면 true → 그 surface
    /// 의 클릭/드래그 캡처를 끈다(휠은 별도로 계속 보고됨, 결정 ②).
    ///
    /// 매칭: 인자 이름을 소문자화하고 `.exe` 접미사를 제거한 stem 을, 각 블랙리스트
    /// 패턴(소문자·trim, 빈 줄은 무시)에 대해 — 패턴에 `*` 가 있으면 glob, 없으면
    /// substring — 으로 비교한다(결정 ①). 빈 블랙리스트면 항상 false.
    pub fn mouse_capture_disabled_for(&self, fg_name: &str) -> bool {
        matches_blacklist(&self.mouse_capture_blacklist, fg_name)
    }

    /// foreground 프로세스 이름이 마우스 캡처 **배너 억제** 블랙리스트에 걸리면 true →
    /// 캡처 자체는 유지하되 "마우스 캡처 중..." 안내 배너만 표시하지 않는다.
    /// [`mouse_capture_disabled_for`](Self::mouse_capture_disabled_for) 와 완전히
    /// 독립적인 별도 필드(`mouse_capture_banner_blacklist`)를 참조하며 매칭 로직만
    /// 공유한다.
    pub fn mouse_capture_banner_disabled_for(&self, fg_name: &str) -> bool {
        matches_blacklist(&self.mouse_capture_banner_blacklist, fg_name)
    }

    /// Detect bash (Git Bash on Windows, system bash on Unix).
    /// Returns the path if found, or an empty string if not.
    pub fn detect_shell() -> String {
        Self::detect_bash().unwrap_or_default()
    }

    /// Try to find bash. On Windows this means Git Bash.
    pub fn detect_bash() -> Option<String> {
        #[cfg(windows)]
        {
            let candidates = [
                std::env::var("ProgramFiles")
                    .map(|p| format!("{}/Git/bin/bash.exe", p))
                    .unwrap_or_default(),
                "C:/Program Files/Git/bin/bash.exe".to_string(),
                "C:/Program Files (x86)/Git/bin/bash.exe".to_string(),
            ];
            for path in &candidates {
                if !path.is_empty() && std::path::Path::new(path).exists() {
                    return Some(path.clone());
                }
            }
            None
        }
        #[cfg(not(windows))]
        {
            // 1. Check /etc/passwd for the user's login shell (most authoritative after chsh)
            if let Some(login_shell) = Self::login_shell_from_passwd()
                && std::path::Path::new(&login_shell).exists()
            {
                return Some(login_shell);
            }
            // 2. Fall back to $SHELL env var
            if let Ok(shell) = std::env::var("SHELL")
                && std::path::Path::new(&shell).exists()
            {
                return Some(shell);
            }
            // 3. Common paths
            for path in &["/bin/zsh", "/bin/bash", "/bin/sh"] {
                if std::path::Path::new(path).exists() {
                    return Some(path.to_string());
                }
            }
            None
        }
    }

    /// Read the user's login shell from /etc/passwd.
    #[cfg(not(windows))]
    fn login_shell_from_passwd() -> Option<String> {
        use std::io::BufRead;
        // SAFETY: getuid는 POSIX thread-safe 시스템콜, errno도 안 set한다.
        let uid = unsafe { libc::getuid() };
        let file = std::fs::File::open("/etc/passwd").ok()?;
        for line in std::io::BufReader::new(file).lines() {
            let line = line.ok()?;
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() >= 7
                && let Ok(entry_uid) = fields[2].parse::<u32>()
                && entry_uid == uid
            {
                return Some(fields[6].to_string());
            }
        }
        None
    }

    /// Returns true if the configured shell path points to an existing bash-compatible shell.
    /// On Windows, the filename must contain "bash" (e.g. bash.exe).
    /// On Unix, any existing shell is accepted (zsh, bash, fish, sh).
    pub fn is_shell_valid(&self) -> bool {
        if self.shell.is_empty() {
            return false;
        }
        let path = std::path::Path::new(&self.shell);
        if !path.exists() {
            return false;
        }
        #[cfg(windows)]
        {
            // On Windows, only accept bash-compatible shells
            let filename = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            filename.contains("bash") || filename.contains("zsh")
        }
        #[cfg(not(windows))]
        {
            true
        }
    }

    /// Resolve effective shell **arguments** for auto-injected shell integration
    /// (OSC7/OSC133/title hooks). bash 는 `--rcfile` 기반 인자로 주입한다(아래
    /// [`bash_rcfile_args_in`]). zsh 는 인자가 아니라 `ZDOTDIR` 환경변수로 주입하므로
    /// ([`effective_shell_envs`]) 여기서는 항상 빈 벡터 — fish/nu/pwsh 등 기타
    /// 셸도 이번 범위 밖이라 마찬가지로 빈 벡터(docs/features/terminal-output/index.md#명령-인덱싱-osc-133 참고).
    pub fn effective_shell_args(&self) -> Vec<String> {
        self.effective_shell_args_in(tasty_dir().as_deref())
    }

    /// [`Self::effective_shell_args`] 의 순수 본문 — 홈 루트를 인자로 받아 프로세스
    /// 전역 env 접촉 없이 단정하게 한다(테스트가 이 경로를 직접 쓴다). 셸 종류
    /// 판정은 여기 남는다. private 이므로 crate 공개 표면은 늘지 않는다.
    fn effective_shell_args_in(&self, dir: Option<&std::path::Path>) -> Vec<String> {
        match tasty_utils::shell_family::ShellFamily::detect(&self.shell) {
            tasty_utils::shell_family::ShellFamily::Bash => {
                ensure_compiled_bashrc_in(dir);
                bash_rcfile_args_in(self, dir)
            }
            _ => Vec::new(),
        }
    }

    /// Resolve effective shell **environment variables** for auto-injected shell
    /// integration. 현재는 zsh 의 `ZDOTDIR` 스왑(docs/features/terminal-output/index.md#명령-인덱싱-osc-133 참고)만 여기 산다 — bash 는
    /// 인자([`effective_shell_args`])로 주입하므로 관여하지 않는다.
    pub fn effective_shell_envs(&self) -> Vec<(String, String)> {
        self.effective_shell_envs_in(tasty_dir().as_deref())
    }

    /// [`Self::effective_shell_envs`] 의 순수 본문 — 홈 루트를 인자로 받아 env 접촉
    /// 없이 단정하게 한다(테스트가 이 경로를 직접 쓴다). private 이라 공개 표면 불변.
    fn effective_shell_envs_in(&self, dir: Option<&std::path::Path>) -> Vec<(String, String)> {
        if tasty_utils::shell_family::ShellFamily::detect(&self.shell)
            != tasty_utils::shell_family::ShellFamily::Zsh
        {
            return Vec::new();
        }
        zsh_shell_envs_in(dir)
    }
}

/// [`GeneralSettings::effective_shell_envs`] 의 zsh 본문 — 루트를 인자로 받아
/// "홈 미해석(`None`)이면 `ZDOTDIR` 을 주입하지 않는다" 를 환경 없이 단정할 수 있게
/// 한다. 셸 종류 판정은 호출부에 남는다.
fn zsh_shell_envs_in(dir: Option<&std::path::Path>) -> Vec<(String, String)> {
    // 홈 미해석이면 wrapper 디렉토리를 만들 수도 가리킬 수도 없다 — 상대
    // `ZDOTDIR` 을 넘기면 zsh 가 자기 CWD 기준으로 읽어 통합이 무음으로 죽는다.
    // 통합 없이 평소 zsh 로 뜨는 편이 옳다(`tasty_dir` 참고).
    let Some(zdotdir) = tasty_zsh_integration_dir_in(dir) else {
        return Vec::new();
    };
    ensure_compiled_zshenv_in(dir);
    let mut envs = vec![("ZDOTDIR".to_string(), zdotdir.to_string_lossy().to_string())];
    // wrapper `.zshenv` 가 원래 ZDOTDIR 로 정확히 복원할 수 있도록 원래 값을
    // 함께 넘긴다(설계결정 3). "미설정"과 "빈 문자열"을 구분해야 하므로(Codex
    // 지적) 마커 env 로 분리한다 — 마커가 없으면 원래 미설정이었다는 뜻이라
    // wrapper 가 unset 으로 복원한다.
    if let Ok(orig) = std::env::var("ZDOTDIR") {
        envs.push(("__TASTY_ORIG_ZDOTDIR_SET".to_string(), "1".to_string()));
        envs.push(("__TASTY_ORIG_ZDOTDIR".to_string(), orig));
    }
    envs
}

// bash 의 `--rcfile` 주입 인자 조립 — 기존
// #[cfg(windows)] 로직과 통합 가능한지 조사한 결과:
//
// - **소싱 인프라(BUILTIN_BASHRC_PRE/PROMPT 상수, compose_*_bashrc, 버전 스탬프,
//   ensure_compiled_bashrc, 경로 헬퍼)는 전부 공유한다** — OSC133 훅 스크립트
//   내용은 OS 와 무관하게 동일해야 하므로(같은 bash, 같은 셸 통합) 이 부분을
//   플랫폼별로 중복시키는 건 정당화가 안 된다. **그 "동일해야" 는 갚을 빚이 아니다** —
//   그 인프라는 이미 한 벌이고, 이 주석의 본론은 아래 "그래서 무엇은 갈라야 하는가" 다.
// - **다만 최종 CLI 인자 모양은 플랫폼마다 다르게 나와야 한다**(진짜 제약,
//   Windows 고유 사정): Windows(Git Bash) 는 `build_shell_command` 가 `-li`
//   (로그인 셸)를 애초에 non-Windows 에만 추가해왔다 — 즉 Windows bash 는
//   이미 non-login 상태로 떠서 `--rcfile` 이 그대로 먹힌다(기존 테스트
//   `tasty_mode_uses_tasty_rc_file` 이 인자 2개 `["--rcfile", path]` 를 정확히
//   검증). 반대로 비-Windows 는 지금까지 무조건 `-li` 를 추가해왔는데, bash 의
//   로그인 셸 판정은 `--rcfile` 유무와 무관하게 우선한다(bash(1): `--rcfile`
//   은 "interactive **non-login**" 셸에만 적용) — `-li` 를 유지한 채 `--rcfile`
//   만 얹으면 조용히 무시된다. 그래서 비-Windows 경로는 명시적 `-i` 를 추가로
//   반환해야 하고(순서: `--rcfile <path> -i`), `build_shell_command` 쪽에서
//   `--rcfile` 이 인자에 있으면 `-li` 추가를 건너뛰도록 맞춰야 한다(tasty-terminal
//   담당, 별도 커밋 아님— 같은 배선).
// - 결론: 목적(로그인 셸처럼 사용자 rc 를 소싱하며 훅 주입)은 두 플랫폼 다
//   달성 가능하므로 **인프라는 통합**하되, "그 결과로 나오는 CLI 인자 모양"은
//   플랫폼별 사실을 그대로 반영해 함수 자체를 분리한다(억지로 한 반환값에
//   합치면 Windows 기존 테스트가 검증하는 정확한 인자 모양이 깨진다) — 이
//   함수(`bash_rcfile_args_in`)가 그 분리 지점이다.
/// bash `--rcfile` 인자를 조립한다 — 루트를 인자로 받아 "홈 미해석(`None`)이면
/// 무엇을 내는가" 를 환경 없이 단정할 수 있게 한다. 프로덕션은 env 를 읽는
/// 진입점(`effective_shell_args`)에서 해석한 루트를 넘긴다.
#[cfg(windows)]
fn bash_rcfile_args_in(settings: &GeneralSettings, dir: Option<&std::path::Path>) -> Vec<String> {
    // OSC7/OSC133 빌트인은 셸 모드와 무관하게 강제 주입한다 — Windows 는 다른
    // 프로세스 cwd 조회 API 가 없어 새 탭 cwd 상속이 OSC7 emit 에만 의존하기
    // 때문. 모드는 "어떤 사용자 rc 를 source 하느냐" 만 결정한다:
    //   - tasty: ~/.tasty/bashrc          (BUILTIN_PRE + ~/.tasty/bashrc.user + BUILTIN_PROMPT)
    //   - default (or unknown): ~/.tasty/bashrc.default (BUILTIN_PRE + source ~/.bashrc + BUILTIN_PROMPT)
    // 홈 미해석이면 인자를 아예 내지 않는다 — 상대 `--rcfile bashrc.default` 는
    // 셸이 자기 CWD 기준으로 해석해 조용히 실패한다(`tasty_dir` 참고). 빈 벡터를
    // 돌려주면 `build_shell_command` 가 평소대로 로그인 셸(`-li`)로 띄운다.
    let rcfile = match settings.shell_mode.as_str() {
        "tasty" => tasty_bashrc_path_in(dir),
        _ => tasty_bashrc_default_path_in(dir),
    };
    let Some(rcfile) = rcfile else {
        return Vec::new();
    };
    vec!["--rcfile".to_string(), rcfile.replace('\\', "/")]
}

/// 비-Windows bash 전용 경로(위 rationale 참고). `shell_mode` UI
/// 개념이 Windows 전용이라 비-Windows 는 "default 모드"(사용자 로그인 프로필
/// 소싱, `default_mode_user_source` 참고) 하나만 쓴다. 루트를 인자로 받아
/// 환경 없이 단정 가능하게 한다(위 Windows 변형과 같은 이유). 프로덕션은 env 를
/// 읽는 진입점(`effective_shell_args`)에서 해석한 루트를 넘긴다.
#[cfg(not(windows))]
fn bash_rcfile_args_in(_settings: &GeneralSettings, dir: Option<&std::path::Path>) -> Vec<String> {
    // 위 Windows 분기와 같은 이유로 홈 미해석이면 빈 벡터 — `-i` 도 함께 뺀다.
    // `-i` 는 `--rcfile` 이 로그인 셸에서 무시되기 때문에 로그인 모드를 포기하며
    // 붙이는 짝이라, `--rcfile` 이 없으면 붙일 이유가 없다. 빈 벡터면
    // `build_shell_command` 가 종전대로 `-li`(대화형 로그인 셸)로 띄운다.
    let Some(rcfile) = tasty_bashrc_default_path_in(dir) else {
        return Vec::new();
    };
    vec!["--rcfile".to_string(), rcfile, "-i".to_string()]
}

/// Path to Tasty's compiled bashrc for **tasty 모드** (BUILTIN_PRE + ~/.tasty/bashrc.user + BUILTIN_PROMPT).
/// Windows 전용 개념(`shell_mode` UI 토글) — 비-Windows 는 이 파일을 만들지 않는다.
pub fn tasty_bashrc_path() -> Option<String> {
    tasty_bashrc_path_in(tasty_dir().as_deref())
}

fn tasty_bashrc_path_in(dir: Option<&std::path::Path>) -> Option<String> {
    Some(dir?.join("bashrc").to_string_lossy().to_string())
}

/// Path to Tasty's compiled bashrc for **default 모드**
/// (BUILTIN_PRE + [`default_mode_user_source`] + BUILTIN_PROMPT). 비-Windows 는
/// 유일하게 쓰는 합성 rc 파일이다(셸 모드 토글이 없어 이거 하나뿐).
pub fn tasty_bashrc_default_path() -> Option<String> {
    tasty_bashrc_default_path_in(tasty_dir().as_deref())
}

fn tasty_bashrc_default_path_in(dir: Option<&std::path::Path>) -> Option<String> {
    Some(dir?.join("bashrc.default").to_string_lossy().to_string())
}

/// Path to the user-editable bashrc fragment. Windows 전용(tasty 모드에서만 쓰임).
pub fn tasty_bashrc_user_path() -> Option<String> {
    tasty_bashrc_user_path_in(tasty_dir().as_deref())
}

fn tasty_bashrc_user_path_in(dir: Option<&std::path::Path>) -> Option<String> {
    Some(dir?.join("bashrc.user").to_string_lossy().to_string())
}

/// `~/.tasty/zsh-integration/` — zsh `ZDOTDIR` 스왑 대상 디렉토리(docs/features/terminal-output/index.md#명령-인덱싱-osc-133 참고).
/// 이 안의 `.zshenv` 가 zsh 가 셸 인스턴스당 정확히 한 번, 가장 먼저 읽는 파일이라
/// 셸 통합 진입점이 된다.
pub fn tasty_zsh_integration_dir() -> Option<std::path::PathBuf> {
    tasty_zsh_integration_dir_in(tasty_dir().as_deref())
}

fn tasty_zsh_integration_dir_in(dir: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    Some(dir?.join("zsh-integration"))
}

/// default 모드 합성 rc 에서 사용자 커스터마이즈를 로드하는 스니펫. Windows(Git
/// Bash) 는 `~/.bashrc` 를 직접 source 하는 게 기존 관례(그 파일이 사실상 유일한
/// 커스터마이즈 지점) — 그대로 보존한다. 비-Windows 는 진짜 login 셸의 파일 탐색
/// 순서(`~/.bash_profile` → `~/.bash_login` → `~/.profile`, 최초 존재하는 파일
/// 하나만)를 그대로 재현한다 — `--rcfile` 로 로그인 모드 자체를 포기하는 대신
/// wrapper 가 login 셸의 소싱 규칙을 흉내내는 것이다. 이 규칙을
/// 따르는 사용자 profile 은 관례상 스스로 `~/.bashrc` 를 source 하므로(예:
/// `[ -f ~/.bashrc ] && . ~/.bashrc`) 이중 소싱 없이 자연히 이어진다 — profile 이
/// 그렇게 안 되어 있으면 `~/.bashrc` 는 로드되지 않는데, 이는 실제 login 셸의
/// 동작과 동일하다(새 버그가 아니라 login 셸 표준 동작의 정확한 재현).
#[cfg(windows)]
fn default_mode_user_source() -> &'static str {
    "[ -f ~/.bashrc ] && source ~/.bashrc\n"
}
#[cfg(not(windows))]
fn default_mode_user_source() -> &'static str {
    "if [ -f ~/.bash_profile ]; then\n    source ~/.bash_profile\nelif [ -f ~/.bash_login ]; then\n    source ~/.bash_login\nelif [ -f ~/.profile ]; then\n    source ~/.profile\nfi\n"
}

/// default 모드용 합성 rc 본문. BUILTIN PRE → [`default_mode_user_source`] → BUILTIN
/// PROMPT. 사용자 rc 가 PROMPT_COMMAND/PS0 를 덮어써도 마지막에 빌트인 훅이 이긴다.
pub fn compose_default_mode_bashrc() -> String {
    format!(
        "{}\n{}\n{}{}",
        BUILTIN_BASHRC_STAMP,
        BUILTIN_BASHRC_PRE,
        default_mode_user_source(),
        BUILTIN_BASHRC_PROMPT,
    )
}

/// tasty 모드용 합성 rc 본문. BUILTIN PRE → tasty user rc → BUILTIN PROMPT.
/// 사용자 영역 (`~/.tasty/bashrc.user`) 이 PROMPT_COMMAND 를 덮어써도 마지막에 prepend.
/// Windows 전용(`shell_mode` UI 토글이 있어야 의미가 있다).
pub fn compose_tasty_mode_bashrc(user_content: &str) -> String {
    format!(
        "{}\n{}\n{}\n{}",
        BUILTIN_BASHRC_STAMP, BUILTIN_BASHRC_PRE, user_content, BUILTIN_BASHRC_PROMPT,
    )
}

/// 셸 통합 파일들이 사는 루트. SoT 는 `tasty_home()` (debug/release 격리 +
/// `TASTY_HOME` override). 판단은 순수부 [`resolve_tasty_dir`] 가 하고 여기서는
/// 환경 조회와 경고만 담당한다 — 경고는 프로세스당 한 번씩만 낸다(경로 헬퍼가
/// 셸을 띄울 때마다 불리므로 매번 찍으면 로그가 쓸모없어진다).
fn tasty_dir() -> Option<std::path::PathBuf> {
    let home = tasty_utils::path::tasty_home();
    if home.is_none() {
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| {
            tracing::warn!(
                "tasty home directory unresolved (TASTY_HOME empty and base dirs unavailable) \
                 — shell integration is disabled: bashrc / zsh-integration paths cannot be derived"
            );
        });
        return None;
    }
    let cwd = std::env::current_dir();
    if let Err(e) = &cwd
        && home.as_ref().is_some_and(|h| !h.is_absolute())
    {
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| {
            tracing::warn!(
                "tasty home is a relative path but the current directory is unreadable ({e}) \
                 — shell integration is disabled: relative bashrc / ZDOTDIR paths would be \
                 re-resolved against the child shell's own directory"
            );
        });
    }
    resolve_tasty_dir(home, cwd.ok())
}

/// [`tasty_dir`] 의 **순수부** — 환경을 읽지 않고 후보만으로 최종 루트를 정한다.
/// 로그는 래퍼가 소유한다(경로 헬퍼가 셸을 띄울 때마다 불리므로 프로세스당 1회).
///
/// `home` 이 `None`(= `TASTY_HOME` 이 비었고 `BaseDirs::new()` 도 실패, HOME 없는
/// 데몬/컨테이너)이면 **빈 경로로 폴백하지 않고** `None` 을 그대로 돌린다.
/// `PathBuf::default()` 는 `""` 라 파생 경로가 전부 상대가 되고, 그러면 (a) 설정
/// 화면의 저장이 프로세스 CWD 에 파일을 만들면서 성공한 것처럼 끝나고 (b)
/// `--rcfile bashrc.default` 가 그대로 셸에 넘어가 셸이 자기 CWD 기준으로 해석한다
/// — OSC7/OSC133 이 무음으로 죽는다.
///
/// 상대 루트(`TASTY_HOME=tmp` 같은 격리 실행)는 거부하지 않고 **`cwd` 기준으로
/// 절대화**한다 — 우리가 파일을 쓰는 위치는 그대로 두면서, 자식 셸에 넘어간 뒤
/// 셸의 CWD 로 다시 해석되는 (b) 를 막는다. `cwd` 마저 없으면 절대화할 수 없으므로
/// `None`.
fn resolve_tasty_dir(
    home: Option<std::path::PathBuf>,
    cwd: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    let dir = home?;
    if dir.is_absolute() {
        return Some(dir);
    }
    Some(cwd?.join(dir))
}

/// 사용자 편집 파일을 로드. 파일이 없으면 `INITIAL_USER_BASHRC`를 반환(파일은 생성하지 않음).
pub fn load_user_bashrc() -> String {
    load_user_bashrc_in(tasty_dir().as_deref())
}

/// [`load_user_bashrc`] 의 본문 — 루트를 인자로 받는다(위 `_in` 들과 같은 이유).
fn load_user_bashrc_in(dir: Option<&std::path::Path>) -> String {
    // 홈 미해석이면 상대 `bashrc.user` 를 CWD 에서 읽지 않는다 — 남의 파일을 사용자
    // 편집 내용으로 오인할 수 있다(`tasty_dir` 참고).
    let Some(path) = tasty_bashrc_user_path_in(dir) else {
        return INITIAL_USER_BASHRC.to_string();
    };
    match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => INITIAL_USER_BASHRC.to_string(),
    }
}

/// 사용자 편집 내용을 디스크에 쓰고, 파생 bashrc(builtin + user)를 재생성한다.
///
/// **실패 사유를 호출자에게 올린다** — 설정 화면의 저장이라 실패는 사용자 작업이
/// 의미를 잃는 사건이고(`docs/dev-guide/error-handling.md` 의 레벨 표가 "설정 저장
/// 실패" 를 `error` 의 대표 사례로 든다), 이 크레이트 안에서 로그로 삼켜버리면
/// 호출자가 사용자에게 알릴 방법 자체가 없어진다. 합성 rc 쓰기 실패도 같은 등급이다:
/// 사용자 본문은 남았는데 셸이 읽는 파일이 옛 내용이면 편집이 반영되지 않은 것과 같다.
pub fn save_user_bashrc(user_content: &str) -> Result<(), String> {
    save_user_bashrc_in(tasty_dir().as_deref(), user_content)
}

/// [`save_user_bashrc`] 의 본문 — 루트를 인자로 받는다(위 `_in` 들과 같은 이유).
fn save_user_bashrc_in(dir: Option<&std::path::Path>, user_content: &str) -> Result<(), String> {
    // 홈 미해석이면 **쓰지 않는다.** 빈 경로로 폴백하면 `create_dir_all("")` 이
    // `Ok` 라 경고 하나 없이 프로세스 CWD 에 `bashrc.user` 를 만들고 성공한 것처럼
    // 끝난다(`tasty_dir` 참고).
    let (Some(user_path), Some(compiled_path)) =
        (tasty_bashrc_user_path_in(dir), tasty_bashrc_path_in(dir))
    else {
        return Err("tasty home directory unresolved — the edit was not persisted".to_string());
    };
    if !ensure_bashrc_parent_dir(&user_path) {
        return Err(format!("parent directory unavailable ({user_path})"));
    }
    std::fs::write(&user_path, user_content).map_err(|e| format!("write {user_path}: {e}"))?;
    let compiled = compose_tasty_mode_bashrc(user_content);
    if !write_generated_file(std::path::Path::new(&compiled_path), &compiled) {
        return Err(format!(
            "compiled rc {compiled_path} not written — the saved edit is not in effect"
        ));
    }
    Ok(())
}

/// `user_path` 의 부모 디렉토리를 보장한다. 생성 실패 시 `false`(호출자는 이후 쓰기
/// 단계를 건너뛴다). 부모가 없는 경로(루트 등)는 이미 존재하는 것으로 간주.
fn ensure_bashrc_parent_dir(user_path: &str) -> bool {
    let Some(parent) = std::path::Path::new(user_path).parent() else {
        return true;
    };
    if let Err(e) = std::fs::create_dir_all(parent) {
        tracing::warn!("create_dir_all for bashrc failed: {e}");
        return false;
    }
    true
}

/// 셸 통합 스크립트를 원자적으로 쓴다 — 같은 디렉토리의 임시 파일에 먼저 쓰고
/// rename 으로 교체해, 쓰는 도중 크래시하거나 다른 프로세스가 half-write 상태를
/// 읽는 상황을 막는다(Codex 지적). 부모 디렉토리는 호출자가 이미 보장한
/// 상태여야 한다. Unix 는 권한을 0o644 로 명시 — 비밀은 없는 평범한 셸 스크립트라
/// 다른 사용자도 읽을 수 있으면 충분하고, 소유자만 쓰기 가능하면 된다.
/// 성공 여부를 돌려준다 — 호출자가 부팅 시 재생성(실패해도 다음 부팅에 다시
/// 시도하므로 `warn`)과 사용자 저장(편집이 반영되지 않으므로 `error`)을 구분해
/// 레벨을 고를 수 있어야 한다.
fn write_generated_file(path: &std::path::Path, content: &str) -> bool {
    let Some(file_name) = path.file_name().and_then(|f| f.to_str()) else {
        tracing::warn!("write_generated_file: path has no file name: {path:?}");
        return false;
    };
    let tmp_path = path.with_file_name(format!("{file_name}.tmp"));
    if !write_tmp_file(&tmp_path, content) {
        return false;
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        tracing::warn!("rename {tmp_path:?} -> {path:?} failed: {e}");
        return false;
    }
    true
}

/// [`write_generated_file`]의 tmp-write + 권한설정 단계만 분리 — 인지 복잡도
/// 상한 때문에 뺐다(동작은 인라인이었을 때와 동일).
fn write_tmp_file(tmp_path: &std::path::Path, content: &str) -> bool {
    if let Err(e) = std::fs::write(tmp_path, content) {
        tracing::warn!("write {tmp_path:?} failed: {e}");
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(tmp_path, std::fs::Permissions::from_mode(0o644)) {
            tracing::warn!("set_permissions for {tmp_path:?} failed: {e}");
        }
    }
    true
}

/// 합성 rc/wrapper 파일이 주어진 버전 스탬프를 담고 있는지. 파일이 없거나 스탬프가
/// 다르면 (구버전/무표기) false — 재생성 대상. bash/zsh 양쪽이 공유하는 스탬프
/// 판정 로직(스탬프 문자열만 다르다).
fn generated_file_stamp_current(path: &str, stamp: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|s| s.lines().any(|l| l.trim() == stamp))
}

/// 두 합성 bash rc (`~/.tasty/bashrc`(Windows 전용) / `~/.tasty/bashrc.default`) 가
/// 존재하고 최신 빌트인 버전인지 보장한다. 파일이 없거나 버전 스탬프가 현재와
/// 다르면 재생성한다 (스탬프 없이 "빠진 파일만 채우면" 빌트인 블록 변경이 기존
/// 설치본에 영원히 반영되지 않는다). 사용자 편집 영역(`bashrc.user`)은 건드리지
/// 않는다 — tasty 모드 재생성은 `save_user_bashrc` 경유라 사용자 본문이 보존된다.
///
/// 비-Windows bash 지원 이전엔 `#[cfg(windows)]` 전용이었다 — bash 셸 통합 자체가 Windows
/// 전용이었기 때문. 비-Windows bash 지원 신설로 게이트를 없애 항상 호출되게
/// 하고, "tasty 모드"(shell_mode UI 토글) 재생성만 Windows 전용으로 남긴다(그
/// 개념 자체가 Windows 전용이므로).
///
/// 프로덕션은 env 를 읽는 진입점(`effective_shell_args`)에서 해석한 루트를
/// 넘긴다 — 이 함수는 env 를 만지지 않는다.
fn ensure_compiled_bashrc_in(dir: Option<&std::path::Path>) {
    // tasty 모드 합성 rc — Windows 전용(shell_mode UI 토글이 있어야 의미 있음).
    #[cfg(windows)]
    if let Some(tasty_path) = tasty_bashrc_path_in(dir)
        && !generated_file_stamp_current(&tasty_path, BUILTIN_BASHRC_STAMP)
    {
        let user = load_user_bashrc_in(dir);
        // 부팅 시 재생성이라 실패는 `warn` — 다음 부팅에 다시 시도한다(사용자
        // 저장 경로의 `error` 와 구분).
        if let Err(reason) = save_user_bashrc_in(dir, &user) {
            tracing::warn!("regenerate tasty-mode bashrc failed: {reason}");
        }
    }
    // default 모드 합성 rc — 양쪽 플랫폼 공통(비-Windows 는 이거 하나만 쓴다).
    // 홈 미해석이면 CWD 에 만들지 않고 그냥 만들지 않는다 — 호출자
    // (`bash_rcfile_args_in`)도 같은 조건에서 `--rcfile` 을 내지 않으므로 셸은 통합
    // 없이 평소대로 뜬다(`tasty_dir` 참고).
    let Some(default_path) = tasty_bashrc_default_path_in(dir) else {
        return;
    };
    if !generated_file_stamp_current(&default_path, BUILTIN_BASHRC_STAMP) {
        let compiled = compose_default_mode_bashrc();
        if !ensure_bashrc_parent_dir(&default_path) {
            return;
        }
        // 부팅 시 재생성이라 실패해도 다음 부팅에 다시 시도한다 — 레벨은 writer
        // 내부의 `warn` 그대로 두고 여기서 승격하지 않는다.
        write_generated_file(std::path::Path::new(&default_path), &compiled);
    }
}

/// zsh `ZDOTDIR` wrapper 의 `.zshenv` 본문(docs/features/terminal-output/index.md#명령-인덱싱-osc-133 참고). OSC133 훅 정의(zsh 네이티브
/// `precmd`/`preexec` 훅 배열 사용 — bash 의 PS0 우회가 필요 없다), `ZDOTDIR` 원복,
/// 원본 `.zshenv` source 를 이 순서로 담는다. `ZDOTDIR` 는 zsh 가 셸 인스턴스당
/// 정확히 한 번, 가장 먼저 읽는 파일이라(설계결정 3) 이 파일이 곧 셸 통합 진입점이다.
pub const BUILTIN_ZSHENV_BODY: &str = r#"# This section is regenerated every time settings are saved. Do not edit.

# OSC 133 (docs/features/terminal-output/index.md#명령-인덱싱-osc-133) — see tasty bashrc for phase semantics. zsh has native
# precmd/preexec hook arrays via `add-zsh-hook`, so no PS0-style workaround
# (needed for bash) is required here.
autoload -Uz add-zsh-hook

__tasty_osc133_precmd() {
    local ec=$?
    printf '\033]133;D;%s\033\\' "$ec"
    printf '\033]133;A\033\\'
}
__tasty_osc133_preexec() {
    printf '\033]133;C;cmd=%s\033\\' "$1"
}
add-zsh-hook precmd __tasty_osc133_precmd
add-zsh-hook preexec __tasty_osc133_preexec

# NOTE(known limitation; see docs/features/terminal-output/index.md#명령-인덱싱-osc-133): if the user's own .zshrc later reassigns
# precmd_functions=(...)/preexec_functions=(...) wholesale (instead of +=),
# or redefines a same-named function, these hooks can be silently dropped.
# zsh's hook-array model makes this far less likely than bash's single
# PROMPT_COMMAND slot, but it is not impossible — documented, not mitigated.

# Restore ZDOTDIR to the user's original value (unset if it wasn't set), then
# source the user's real .zshenv — that file is normally read exactly once per
# shell instance, and this wrapper already consumed that one read. Restore
# *immediately*, then source right after, in that order (design decision 3).
if [ -n "${__TASTY_ORIG_ZDOTDIR_SET:-}" ]; then
    export ZDOTDIR="$__TASTY_ORIG_ZDOTDIR"
else
    unset ZDOTDIR
fi
unset __TASTY_ORIG_ZDOTDIR_SET __TASTY_ORIG_ZDOTDIR

__tasty_real_zdotdir="${ZDOTDIR:-$HOME}"
[ -f "$__tasty_real_zdotdir/.zshenv" ] && source "$__tasty_real_zdotdir/.zshenv"
unset __tasty_real_zdotdir
"#;

/// 합성 zshenv 버전 스탬프. [`compose_zsh_zshenv`] 가 출력 맨 앞에 심고,
/// [`ensure_compiled_zshenv_in`] 이 기존 파일에서 이 줄이 일치하지 않으면 강제
/// 재생성한다(bash 스탬프와 동일한 이유 — `BUILTIN_BASHRC_STAMP` 참고).
/// **`BUILTIN_ZSHENV_BODY` 내용을 바꿀 때마다 숫자를 +1 할 것.**
pub const BUILTIN_ZSHENV_STAMP: &str = "# tasty-zshenv-v1";

/// Path to Tasty's compiled zsh wrapper `.zshenv` (`~/.tasty/zsh-integration/.zshenv`).
pub fn tasty_zshenv_path() -> Option<std::path::PathBuf> {
    tasty_zshenv_path_in(tasty_dir().as_deref())
}

fn tasty_zshenv_path_in(dir: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    Some(tasty_zsh_integration_dir_in(dir)?.join(".zshenv"))
}

/// 합성 `.zshenv` 본문. 스탬프 + [`BUILTIN_ZSHENV_BODY`].
pub fn compose_zsh_zshenv() -> String {
    format!("{}\n{}", BUILTIN_ZSHENV_STAMP, BUILTIN_ZSHENV_BODY)
}

/// wrapper `.zshenv` 가 존재하고 최신 빌트인 버전인지 보장한다. 파일이 없거나
/// 버전 스탬프가 현재와 다르면 재생성한다(bash 의 `ensure_compiled_bashrc_in` 와
/// 동형 — 사용자 편집 영역이 없어 훨씬 단순하다: zsh 는 wrapper 가 사용자 콘텐츠를
/// 감싸지 않고 그대로 원본 `.zshenv` 로 넘기므로 재생성이 사용자 데이터를 건드릴
/// 위험 자체가 없다).
/// 루트를 인자로 받는다 — 유일한 프로덕션 호출부([`zsh_shell_envs_in`])가 이미
/// 한 번 해석한 값을 그대로 넘기므로 얇은 래퍼를 따로 두지 않는다.
fn ensure_compiled_zshenv_in(dir: Option<&std::path::Path>) {
    // 홈 미해석이면 CWD 에 `zsh-integration/` 을 만들지 않는다(`tasty_dir` 참고).
    let (Some(zdir), Some(path)) = (tasty_zsh_integration_dir_in(dir), tasty_zshenv_path_in(dir))
    else {
        return;
    };
    if let Err(e) = std::fs::create_dir_all(&zdir) {
        tracing::warn!("create_dir_all for zsh-integration failed: {e}");
        return;
    }
    let path_str = path.to_string_lossy().to_string();
    if generated_file_stamp_current(&path_str, BUILTIN_ZSHENV_STAMP) {
        return;
    }
    let compiled = compose_zsh_zshenv();
    write_generated_file(&path, &compiled);
}

/// # 이 모듈이 아직 덮지 못하는 것
///
/// 홈 **미해석**(`tasty_home() == None`) 축은 **판단과 다섯 호출부의 반응을 전부**
/// 덮는다 — [`resolve_tasty_dir`] 와 각 `_in` 함수에 `None` 을 그대로 넣어 단정한다
/// (`unresolved_home_*` 테스트들).
///
/// 덮지 못하는 것은 **`None` 을 만드는 축 하나**다: `tasty_dir()` 이
/// `tasty_utils::path::tasty_home()` 에서 실제로 `None` 을 받는 경우. `directories` 가
/// `HOME` 없이도 passwd 엔트리로 홈을 찾아내므로 `env -u HOME` 만으로는 `None` 을 만들
/// 수 없고, 재현하려면 passwd 엔트리가 없는 환경(컨테이너/`unshare`)이 필요하다. 그
/// 한 줄은 리뷰로만 보장된다 — 프로덕션에 테스트 전용 주입 지점을 만드는 비용보다
/// 낫다고 판단했다.
///
/// **각 `pub fn` 의 `tasty_dir().as_deref()` 위임은 덮는다** — `None` 이 필요 없고
/// 상대 `TASTY_HOME` 이면 갈리기 때문이다(절대 경로는 해석 전후가 같아 구분되지
/// 않는다). 출력 경로 둘(`bash_rcfile_args_in`·`effective_shell_envs`)을
/// `*_uses_the_resolved_root` 가 각각 고정한다.
///
/// 상대 경로 축(`TASTY_HOME` 이 상대일 때 파생 경로가 자식 셸의 CWD 로 재해석되는 것)
/// 은 환경만으로 재현 가능해 `relative_tasty_home_is_absolutized_for_child_processes`
/// 가 실제 env 로 끝까지 고정한다.
#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with_blacklist(patterns: &[&str]) -> GeneralSettings {
        GeneralSettings {
            mouse_capture_blacklist: patterns.iter().map(|s| s.to_string()).collect(),
            ..GeneralSettings::default()
        }
    }

    #[test]
    fn mouse_capture_substring_hit_and_miss() {
        let g = settings_with_blacklist(&["htop"]);
        assert!(g.mouse_capture_disabled_for("htop"));
        assert!(g.mouse_capture_disabled_for("htop-vim")); // substring
        assert!(!g.mouse_capture_disabled_for("vim"));
    }

    #[test]
    fn mouse_capture_case_insensitive() {
        let g = settings_with_blacklist(&["HTOP"]);
        assert!(g.mouse_capture_disabled_for("htop"));
        assert!(g.mouse_capture_disabled_for("HTOP"));
    }

    #[test]
    fn mouse_capture_strips_exe_suffix() {
        let g = settings_with_blacklist(&["htop"]);
        assert!(g.mouse_capture_disabled_for("htop.exe"));
        assert!(g.mouse_capture_disabled_for("HTOP.EXE"));
    }

    #[test]
    fn mouse_capture_empty_blacklist_never_matches() {
        let g = settings_with_blacklist(&[]);
        assert!(!g.mouse_capture_disabled_for("htop"));
    }

    #[test]
    fn mouse_capture_blank_pattern_ignored() {
        // 공백/빈 줄 패턴이 전체 매칭으로 새지 않아야 한다.
        let g = settings_with_blacklist(&["   ", ""]);
        assert!(!g.mouse_capture_disabled_for("htop"));
        assert!(!g.mouse_capture_disabled_for("anything"));
    }

    #[test]
    fn mouse_capture_glob_wildcard() {
        let g = settings_with_blacklist(&["ht*"]);
        assert!(g.mouse_capture_disabled_for("htop"));
        assert!(!g.mouse_capture_disabled_for("vim"));

        let g = settings_with_blacklist(&["*top"]);
        assert!(g.mouse_capture_disabled_for("htop"));
        assert!(!g.mouse_capture_disabled_for("htopx"));

        let g = settings_with_blacklist(&["h*p"]);
        assert!(g.mouse_capture_disabled_for("htop"));
        assert!(!g.mouse_capture_disabled_for("hat"));
    }

    #[test]
    fn mouse_capture_blacklist_defaults_empty() {
        let g = GeneralSettings::default();
        assert!(g.mouse_capture_blacklist.is_empty());
    }

    #[test]
    fn mouse_capture_banner_disabled_for_matches_pattern() {
        let mut s = GeneralSettings::default();
        s.mouse_capture_banner_blacklist = vec!["vim".to_string()];
        assert!(s.mouse_capture_banner_disabled_for("vim"));
        assert!(!s.mouse_capture_banner_disabled_for("htop"));
    }

    #[test]
    fn mouse_capture_banner_blacklist_independent_of_capture_blacklist() {
        let mut s = GeneralSettings::default();
        s.mouse_capture_blacklist = vec!["htop".to_string()];
        s.mouse_capture_banner_blacklist = vec!["vim".to_string()];
        assert!(s.mouse_capture_disabled_for("htop"));
        assert!(!s.mouse_capture_disabled_for("vim"));
        assert!(s.mouse_capture_banner_disabled_for("vim"));
        assert!(!s.mouse_capture_banner_disabled_for("htop"));
    }

    #[test]
    fn mouse_capture_banner_blacklist_defaults_empty() {
        let g = GeneralSettings::default();
        assert!(g.mouse_capture_banner_blacklist.is_empty());
    }

    #[test]
    fn legacy_restore_terminal_content_key_still_loads() {
        // 구버전 키(`restore_terminal_content`)로 저장된 값이 새 필드로 매핑되어야 한다
        // (serde alias). 구버전 settings 호환. (settings 는 toml 직렬화.)
        let g: GeneralSettings = toml::from_str("restore_terminal_content = false").unwrap();
        assert!(!g.restore_surface_content);
    }

    #[test]
    fn new_restore_surface_content_key_loads() {
        let g: GeneralSettings = toml::from_str("restore_surface_content = false").unwrap();
        assert!(!g.restore_surface_content);
    }

    #[test]
    fn reverse_screen_enabled_defaults_true() {
        // 기본값은 현행 스펙 유지(DECSCNM 정상 렌더).
        let g = GeneralSettings::default();
        assert!(g.reverse_screen_enabled);
    }

    #[test]
    fn reverse_screen_enabled_missing_key_uses_default() {
        // 구 config.toml 마이그레이션 안전: 키가 없으면 기본 true.
        let g: GeneralSettings = toml::from_str("scrollback_lines = 5000").unwrap();
        assert!(g.reverse_screen_enabled);
    }

    #[test]
    fn reverse_screen_enabled_round_trips() {
        let g: GeneralSettings = toml::from_str("reverse_screen_enabled = false").unwrap();
        assert!(!g.reverse_screen_enabled);
        let out = toml::to_string(&g).unwrap();
        let g2: GeneralSettings = toml::from_str(&out).unwrap();
        assert!(!g2.reverse_screen_enabled);
    }

    #[test]
    fn bell_notification_defaults_true() {
        let g = GeneralSettings::default();
        assert!(g.bell_notification);
    }

    #[test]
    fn bell_notification_missing_key_uses_default() {
        // 구 config.toml 마이그레이션 안전: 키가 없으면 기본 true.
        let g: GeneralSettings = toml::from_str("scrollback_lines = 5000").unwrap();
        assert!(g.bell_notification);
    }

    #[test]
    fn bell_notification_round_trips() {
        let g: GeneralSettings = toml::from_str("bell_notification = false").unwrap();
        assert!(!g.bell_notification);
        let out = toml::to_string(&g).unwrap();
        let g2: GeneralSettings = toml::from_str(&out).unwrap();
        assert!(!g2.bell_notification);
    }

    #[test]
    fn explorer_view_mode_defaults_to_detail() {
        let g = GeneralSettings::default();
        assert_eq!(g.explorer_view_mode, "detail");
    }

    #[test]
    fn explorer_view_mode_round_trips() {
        let g: GeneralSettings = toml::from_str("explorer_view_mode = \"list\"").unwrap();
        assert_eq!(g.explorer_view_mode, "list");
        let out = toml::to_string(&g).unwrap();
        let g2: GeneralSettings = toml::from_str(&out).unwrap();
        assert_eq!(g2.explorer_view_mode, "list");
    }

    #[test]
    fn explorer_view_mode_missing_key_uses_default() {
        // 구 config.toml 마이그레이션 안전: 키가 없으면 기본값 detail.
        let g: GeneralSettings = toml::from_str("scrollback_lines = 5000").unwrap();
        assert_eq!(g.explorer_view_mode, "detail");
    }

    #[cfg(windows)]
    fn settings_with_mode(mode: &str) -> GeneralSettings {
        GeneralSettings {
            // 명시적 bash 경로 — CI 러너에 실제 Git Bash 가 설치돼 있는지와
            // 무관하게 `ShellFamily::detect` 가 항상 Bash 로 판정하도록 한다
            // (effective_shell_args 가 이제 shell 필드도 본다).
            shell: "bash.exe".to_string(),
            shell_mode: mode.to_string(),
            ..GeneralSettings::default()
        }
    }

    /// 플랫폼 무관 — Windows 에서도 `unresolved_home_emits_no_bash_rcfile_args` 와
    /// `bash_rcfile_args_uses_the_resolved_root` 가 쓴다(그 둘은 홈 해석 축이라
    /// 플랫폼별로 갈리지 않는다).
    fn settings_with_shell(shell: &str) -> GeneralSettings {
        GeneralSettings {
            shell: shell.to_string(),
            ..GeneralSettings::default()
        }
    }

    // TASTY_HOME env 는 프로세스 전역이라 `set_var`/`remove_var` 가 다른 스레드의
    // `var_os` 와 겹치면 UB(edition 2024 가 unsafe 로 표시)다. 그래서 대부분의
    // 테스트는 홈 경로를 `_in(Some(home.path()))` 로 **주입**해 env 를 아예 안
    // 만진다(아래 `TmpHome`). 남은 SERIAL 은 **상대 TASTY_HOME 해석 자체가 검증
    // 대상**이라 env 를 만질 수밖에 없는 소수(`relative_tasty_home_is_absolutized_for_child_processes`,
    // `RelativeHomeGuard` 계열)와 CWD 오염 canary 를 직렬화한다.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 테스트마다 격리된 임시 홈 디렉토리. **env 를 만지지 않는다** — 홈 경로는
    /// `_in(Some(home.path()))` 로 명시 주입한다(프로세스 전역 env 경합 회피).
    struct TmpHome {
        dir: tempfile::TempDir,
    }
    impl TmpHome {
        fn new() -> Self {
            Self {
                dir: tempfile::tempdir().expect("tempdir"),
            }
        }
        fn path(&self) -> &std::path::Path {
            self.dir.path()
        }
    }

    // default 모드: Windows 는 `--rcfile ~/.tasty/bashrc.default` 로 OSC7 강제 주입.
    #[cfg(windows)]
    #[test]
    fn default_mode_uses_default_rc_file() {
        let home = TmpHome::new();
        let args = settings_with_mode("default").effective_shell_args_in(Some(home.path()));
        assert!(args.iter().any(|a| a == "--rcfile"));
        assert!(args.iter().any(|a| {
            a.ends_with("bashrc.default") && std::path::Path::new(a).starts_with(home.path())
        }));
    }

    // unknown 모드도 default 와 동일하게 fallback.
    #[cfg(windows)]
    #[test]
    fn unknown_mode_falls_back_to_default_rc_file() {
        let home = TmpHome::new();
        let args = settings_with_mode("fast").effective_shell_args_in(Some(home.path()));
        assert!(args.iter().any(|a| a == "--rcfile"));
        assert!(args.iter().any(|a| {
            a.ends_with("bashrc.default") && std::path::Path::new(a).starts_with(home.path())
        }));
    }

    // tasty 모드: Windows 는 `--rcfile ~/.tasty/bashrc` 로 빌트인을 source 한다(S-2 픽스 보존).
    #[cfg(windows)]
    #[test]
    fn tasty_mode_uses_tasty_rc_file() {
        let home = TmpHome::new();
        let args = settings_with_mode("tasty").effective_shell_args_in(Some(home.path()));
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "--rcfile");
        assert!(args.iter().any(|a| {
            a.ends_with("bashrc") && std::path::Path::new(a).starts_with(home.path())
        }));
        assert!(args.iter().all(|a| !a.ends_with("bashrc.default")));
    }

    // default 모드 합성 rc(Windows) 는 BUILTIN PRE / 사용자 ~/.bashrc source /
    // BUILTIN PROMPT 셋 다 포함.
    #[cfg(windows)]
    #[test]
    fn default_mode_compiled_rc_sources_user_bashrc() {
        let compiled = compose_default_mode_bashrc();
        assert!(compiled.contains("__tasty_osc7")); // BUILTIN PRE 적용
        assert!(compiled.contains("source ~/.bashrc")); // 사용자 rc 호출
        assert!(compiled.contains("PROMPT_COMMAND=")); // BUILTIN PROMPT 적용
    }

    // default 모드 합성 rc(비-Windows) 는 진짜 login 셸의 프로필 탐색 순서를
    // 재현한다(docs/features/terminal-output/index.md#명령-인덱싱-osc-133 참고) — bash_profile/bash_login/profile 셋 다 언급.
    #[cfg(not(windows))]
    #[test]
    fn default_mode_compiled_rc_searches_login_profiles_on_unix() {
        let compiled = compose_default_mode_bashrc();
        assert!(compiled.contains("__tasty_osc7")); // BUILTIN PRE 적용
        assert!(compiled.contains(".bash_profile"));
        assert!(compiled.contains(".bash_login"));
        assert!(compiled.contains(".profile"));
        assert!(compiled.contains("PROMPT_COMMAND=")); // BUILTIN PROMPT 적용
    }

    // tasty 모드 합성 rc 는 BUILTIN PRE / 사용자 본문 / BUILTIN PROMPT 셋 다 포함.
    #[test]
    fn tasty_mode_compiled_rc_wraps_user_content() {
        let compiled = compose_tasty_mode_bashrc("alias hi='echo hi'\n");
        assert!(compiled.contains("__tasty_osc7"));
        assert!(compiled.contains("alias hi='echo hi'"));
        assert!(compiled.contains("PROMPT_COMMAND="));
        // PROMPT_COMMAND 는 사용자 본문 *뒤* 에 와야 한다.
        let user_pos = compiled.find("alias hi").unwrap();
        let prompt_pos = compiled.find("PROMPT_COMMAND=").unwrap();
        assert!(prompt_pos > user_pos);
    }

    // 두 모드 합성 rc 모두 __tasty_title 정의 + PROMPT_COMMAND 합류를 포함한다.
    #[test]
    fn compiled_rcs_define_and_join_tasty_title() {
        for compiled in [
            compose_default_mode_bashrc(),
            compose_tasty_mode_bashrc("alias hi='echo hi'\n"),
        ] {
            assert!(compiled.contains("__tasty_title()"), "definition missing");
            assert!(
                compiled.contains(
                    r#"PROMPT_COMMAND="__tasty_osc133_precmd;__tasty_osc7;__tasty_title"#
                ),
                "PROMPT_COMMAND joint missing"
            );
        }
    }

    // OSC133 훅 — 정의 + PROMPT_COMMAND(A/D) + PS0(C) 배선 모두 포함.
    #[test]
    fn compiled_rcs_wire_osc133_hooks() {
        for compiled in [compose_default_mode_bashrc(), compose_tasty_mode_bashrc("")] {
            assert!(
                compiled.contains("__tasty_osc133_precmd()"),
                "precmd definition missing"
            );
            assert!(
                compiled.contains("__tasty_osc133_preexec()"),
                "preexec definition missing"
            );
            assert!(compiled.contains(r#"PS0='$(__tasty_osc133_preexec)'"#));
            assert!(compiled.contains(r#"\033]133;A\033\\"#));
            // bash C phase 는 cmd= 를 싣지 않는다 — osc.rs 의 C phase 파서가
            // 셸 무관하게 payload 를 버리는 게 주 이유고, bash PS0 시점
            // command-text 조회 신뢰성 문제는 부차적(위 __tasty_osc133_preexec
            // 주석 참고).
            assert!(compiled.contains(r#"\033]133;C\033\\"#));
            assert!(compiled.contains(r#"\033]133;D;%s\033\\"#));
        }
    }

    // 두 모드 합성 rc 모두 버전 스탬프를 담는다 (재생성 감지의 전제).
    #[test]
    fn compiled_rcs_carry_version_stamp() {
        for compiled in [compose_default_mode_bashrc(), compose_tasty_mode_bashrc("")] {
            assert!(
                compiled.lines().any(|l| l.trim() == BUILTIN_BASHRC_STAMP),
                "version stamp missing"
            );
        }
    }

    // 스탬프 없는 (구버전) 기존 합성 rc 는 ensure_compiled_bashrc 가 강제 재생성한다.
    // default_path 는 양쪽 플랫폼 공통 경로, tasty_path 는 Windows 전용(shell_mode
    // UI 토글이 있어야 의미 있음)이라 그 부분만 추가로 windows 게이트한다.
    #[test]
    fn ensure_compiled_bashrc_regenerates_stale_files() {
        let home = TmpHome::new();
        let default_path =
            tasty_bashrc_default_path_in(Some(home.path())).expect("injected home always resolves");
        std::fs::create_dir_all(std::path::Path::new(&default_path).parent().unwrap()).unwrap();
        std::fs::write(&default_path, "# old builtin without stamp\n").unwrap();
        #[cfg(windows)]
        let tasty_path = {
            let p = tasty_bashrc_path_in(Some(home.path())).expect("injected home always resolves");
            std::fs::write(&p, "# old builtin without stamp\n").unwrap();
            p
        };

        ensure_compiled_bashrc_in(Some(home.path()));

        #[cfg(windows)]
        let paths = [default_path.as_str(), tasty_path.as_str()];
        #[cfg(not(windows))]
        let paths = [default_path.as_str()];
        for path in paths {
            let content = std::fs::read_to_string(path).unwrap();
            assert!(
                content.lines().any(|l| l.trim() == BUILTIN_BASHRC_STAMP),
                "{path} not regenerated with current stamp"
            );
            assert!(
                content.contains("__tasty_title()"),
                "{path} missing __tasty_title"
            );
        }
    }

    // 현재 스탬프를 담은 파일은 재생성하지 않는다 (사용자 저장 결과 보존). Windows
    // 전용(shell_mode UI 로 저장되는 tasty 모드 rc 를 검증) — 비-Windows 동형 검증은
    // `ensure_compiled_zshenv_keeps_current_file`(zsh)로 커버한다.
    #[cfg(windows)]
    #[test]
    fn ensure_compiled_bashrc_keeps_current_files() {
        let home = TmpHome::new();
        let tasty_path =
            tasty_bashrc_path_in(Some(home.path())).expect("injected home always resolves");
        std::fs::create_dir_all(std::path::Path::new(&tasty_path).parent().unwrap()).unwrap();
        let current = compose_tasty_mode_bashrc("# SENTINEL-user-content\n");
        std::fs::write(&tasty_path, &current).unwrap();

        ensure_compiled_bashrc_in(Some(home.path()));

        let content = std::fs::read_to_string(&tasty_path).unwrap();
        assert!(
            content.contains("# SENTINEL-user-content"),
            "current-stamped file must not be overwritten"
        );
    }

    // 비-Windows bash: `--rcfile <default rc> -i` — `-i` 는 `-li`(로그인 셸)를
    // 유지한 채로는 `--rcfile` 이 무시되기 때문에 필요하다(docs/features/terminal-output/index.md#명령-인덱싱-osc-133 참고).
    #[cfg(not(windows))]
    #[test]
    fn unix_bash_effective_args_use_rcfile_and_interactive() {
        let home = TmpHome::new();
        let args = settings_with_shell("/bin/bash").effective_shell_args_in(Some(home.path()));
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], "--rcfile");
        assert!(args[1].ends_with("bashrc.default"));
        assert_eq!(args[2], "-i");
    }

    // 비-Windows bash 는 env 로 아무것도 주입하지 않는다(전부 args 경로).
    #[cfg(not(windows))]
    #[test]
    fn unix_bash_effective_envs_are_empty() {
        // 결과는 홈과 무관하지만 wrapper `effective_shell_envs()` 는 인자로
        // `tasty_dir()`(env 읽음)를 먼저 평가한다 — `_in(None)` 으로 그 env 읽기를
        // 피한다(아래 두 "빈 결과" 테스트도 같은 이유).
        assert!(
            settings_with_shell("/bin/bash")
                .effective_shell_envs_in(None)
                .is_empty()
        );
    }

    // 비-Windows zsh 는 args 가 아니라 env(ZDOTDIR)로만 주입한다.
    #[cfg(not(windows))]
    #[test]
    fn unix_zsh_effective_args_are_empty() {
        assert!(
            settings_with_shell("/bin/zsh")
                .effective_shell_args_in(None)
                .is_empty()
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_zsh_effective_envs_set_zdotdir() {
        let home = TmpHome::new();
        let envs = settings_with_shell("/bin/zsh").effective_shell_envs_in(Some(home.path()));
        let zdotdir = envs
            .iter()
            .find(|(k, _)| k == "ZDOTDIR")
            .map(|(_, v)| v.as_str())
            .expect("ZDOTDIR present");
        assert!(zdotdir.ends_with("zsh-integration"));
    }

    // 기타 셸(fish 등)은 이번 범위 밖 — args/env 둘 다 빈 채로 조용히 넘어간다.
    #[cfg(not(windows))]
    #[test]
    fn unix_other_shell_args_and_envs_are_empty() {
        let s = settings_with_shell("/usr/bin/fish");
        assert!(s.effective_shell_args_in(None).is_empty());
        assert!(s.effective_shell_envs_in(None).is_empty());
    }

    // zsh wrapper .zshenv 는 OSC133 훅 정의 + ZDOTDIR 복원/원본 소싱을 모두 포함.
    #[test]
    fn zsh_zshenv_contains_osc133_hooks_and_zdotdir_restore() {
        let compiled = compose_zsh_zshenv();
        assert!(compiled.contains("__tasty_osc133_precmd()"));
        assert!(compiled.contains("__tasty_osc133_preexec()"));
        assert!(compiled.contains("add-zsh-hook precmd __tasty_osc133_precmd"));
        assert!(compiled.contains("add-zsh-hook preexec __tasty_osc133_preexec"));
        assert!(compiled.contains("__TASTY_ORIG_ZDOTDIR_SET"));
        assert!(compiled.contains("source \"$__tasty_real_zdotdir/.zshenv\""));
    }

    #[test]
    fn zsh_zshenv_carries_version_stamp() {
        assert!(
            compose_zsh_zshenv()
                .lines()
                .any(|l| l.trim() == BUILTIN_ZSHENV_STAMP)
        );
    }

    // 스탬프 없는 기존 wrapper .zshenv 는 강제 재생성된다.
    #[test]
    fn ensure_compiled_zshenv_regenerates_stale_file() {
        let home = TmpHome::new();
        let path = tasty_zshenv_path_in(Some(home.path())).expect("injected home always resolves");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# old wrapper without stamp\n").unwrap();

        ensure_compiled_zshenv_in(Some(home.path()));

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.lines().any(|l| l.trim() == BUILTIN_ZSHENV_STAMP));
        assert!(content.contains("__tasty_osc133_precmd()"));
    }

    /// 홈 미해석은 **빈 경로로 폴백하지 않는다.** `PathBuf::default()` 는 `""` 라
    /// 파생 경로가 전부 상대가 되고, 그게 CHANGELOG 가 헤드라인으로 내건 결함
    /// (셸 통합이 상대 경로로 새어 나감)의 정확한 원인이다. `home?` 를
    /// `home.unwrap_or_default()` 로 되살리면 이 테스트가 실패한다.
    #[test]
    fn unresolved_home_resolves_to_none_not_an_empty_path() {
        let cwd = std::path::PathBuf::from("/some/cwd");
        assert_eq!(resolve_tasty_dir(None, Some(cwd.clone())), None);
        // 대조군 — 절대 루트는 그대로, 상대 루트는 cwd 기준 절대화.
        assert_eq!(
            resolve_tasty_dir(Some("/abs/root".into()), Some(cwd.clone())),
            Some(std::path::PathBuf::from("/abs/root"))
        );
        assert_eq!(
            resolve_tasty_dir(Some("rel".into()), Some(cwd.clone())),
            Some(cwd.join("rel"))
        );
        // cwd 마저 없으면 상대 루트를 절대화할 수 없다.
        assert_eq!(resolve_tasty_dir(Some("rel".into()), None), None);
    }

    /// 홈 미해석이면 **셸에 `--rcfile` 을 넘기지 않는다.** 넘기면 상대 경로가 되어
    /// 셸이 자기 CWD 기준으로 해석하고 OSC7/OSC133 이 무음으로 죽는다. 빈 벡터면
    /// `build_shell_command` 가 평소대로 로그인 셸(`-li`)로 띄운다.
    #[test]
    fn unresolved_home_emits_no_bash_rcfile_args() {
        let settings = settings_with_shell("/bin/bash");
        assert!(
            bash_rcfile_args_in(&settings, None).is_empty(),
            "홈 미해석이면 --rcfile 인자를 내지 않는다"
        );
        #[cfg(windows)]
        assert!(
            bash_rcfile_args_in(&settings_with_mode("tasty"), None).is_empty(),
            "tasty 모드도 마찬가지"
        );
    }

    /// 홈 미해석이면 **`ZDOTDIR` 을 주입하지 않는다.** 상대 `ZDOTDIR` 을 넘기면 zsh 가
    /// 자기 CWD 기준으로 읽어 통합이 무음으로 죽는다.
    #[test]
    fn unresolved_home_injects_no_zdotdir() {
        assert!(zsh_shell_envs_in(None).is_empty());
    }

    /// 홈 미해석이면 **CWD 의 `bashrc.user` 를 읽지 않는다** — 남의 파일을 사용자
    /// 편집 내용으로 오인하면 그걸 그대로 합성 rc 에 실어 저장한다.
    #[test]
    fn unresolved_home_loads_the_initial_user_bashrc_not_a_cwd_file() {
        assert_eq!(load_user_bashrc_in(None), INITIAL_USER_BASHRC);
    }

    /// 홈 미해석이면 **저장을 거절한다.** 빈 경로로 폴백하면 `create_dir_all("")` 이
    /// `Ok` 라 경고 하나 없이 프로세스 CWD 에 파일을 만들고 성공한 것처럼 끝난다.
    #[test]
    fn unresolved_home_refuses_to_save_user_bashrc() {
        let err = save_user_bashrc_in(None, "# user\n").expect_err("저장을 거절해야 한다");
        assert!(
            err.contains("unresolved"),
            "실패 사유가 호출자에게 전달돼야 한다: {err}"
        );
    }

    /// 홈 미해석이면 **합성 rc 파일을 아무 데도 만들지 않는다.** 빈 경로로 폴백하면
    /// 파생 경로가 상대가 되어 프로세스 CWD(= 이 크레이트 디렉토리)에 생긴다.
    ///
    /// 사전에 **지우지 않고 단정하는** 이유: 앞선 실패/변이 실험이 그 파일을 남기면
    /// "이미 있었으니 변화 없음" 으로 **거짓 통과**한다(실제로 변이 실험 중
    /// `bashrc.default` 가 남아 그 위험을 확인했다). 그렇다고 조용히 지우면 그 오염
    /// 자체가 감춰진다 — 이 이름들은 크레이트 소스 디렉토리에 정당하게 존재할 수 없으므로
    /// 있으면 그 사실을 실패로 드러내고, 정리는 사람이 한다.
    #[test]
    fn unresolved_home_writes_no_generated_files() {
        let _s = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let cwd = std::env::current_dir().expect("cwd");
        let names = ["bashrc", "bashrc.default", "bashrc.user", "zsh-integration"];
        for n in names {
            assert!(
                !cwd.join(n).exists(),
                "{n} 이 이미 CWD 에 있다 — 앞선 실험이 남긴 오염이다. 이 테스트가 \
                 무엇을 단정하는지 무의미해지므로 지우고 다시 돌려라"
            );
        }

        ensure_compiled_bashrc_in(None);
        ensure_compiled_zshenv_in(None);

        for n in names {
            assert!(
                !cwd.join(n).exists(),
                "홈 미해석이면 프로세스 CWD 에 {n} 을 만들지 않는다"
            );
        }
    }

    // 상대 `TASTY_HOME`(격리 실행 관례)에서 파생 경로가 절대 경로로 나오는지.
    // 상대인 채로 자식 셸의 `--rcfile` / `ZDOTDIR` 로 나가면 셸이 자기 CWD 기준으로
    // 다시 해석해 통합이 무음으로 죽는다. 경로 헬퍼만 부른다 — `effective_shell_*`
    // 는 합성 rc 를 실제로 만들기 때문에 CWD 를 더럽힌다.
    #[test]
    fn relative_tasty_home_is_absolutized_for_child_processes() {
        // 손으로 set 하고 마지막에 되돌리면 **아래 두 `expect` 가 패닉할 때 복원에
        // 도달하지 못한다** — 그러면 이 바이너리의 뒤 테스트가 상대 `TASTY_HOME` 을
        // 물려받아 자기 물음 대신 남의 실패를 본다. [`RelativeHomeGuard`] 는 생성자가
        // `SERIAL` 을 직접 쥐고 Drop 이 되돌린다(파일은 안 만드는 경로라 `name` 을 쓴다).
        let _home = RelativeHomeGuard::name("relative-tasty-home");

        let rc = tasty_bashrc_default_path().expect("relative home still resolves");
        let zdotdir = tasty_zsh_integration_dir().expect("relative home still resolves");

        let cwd = std::env::current_dir().expect("cwd");
        assert!(
            std::path::Path::new(&rc).is_absolute(),
            "--rcfile path must be absolute, got {rc}"
        );
        assert_eq!(
            std::path::Path::new(&rc),
            cwd.join("relative-tasty-home").join("bashrc.default")
        );
        assert!(
            zdotdir.is_absolute(),
            "ZDOTDIR must be absolute, got {}",
            zdotdir.display()
        );
        assert_eq!(
            zdotdir,
            cwd.join("relative-tasty-home").join("zsh-integration")
        );
    }

    /// 상대 `TASTY_HOME` 을 세팅하고 복원하는 가드.
    ///
    /// [`TmpHome`] 는 임시 디렉토리의 **절대** 경로를 주입하므로 "래퍼가 해석된 루트를
    /// 넘기는가" 를 구분하지 못한다 — 절대 경로는 해석 전후가 같기 때문이다. 상대
    /// 경로여야 `tasty_dir()`(CWD 기준 절대화)과 미해석 `tasty_home()` 이 갈린다.
    /// **생성자가 [`SERIAL`] 을 직접 쥔다** — 호출부가 잊어도 직렬화가 깨지지 않는다.
    /// 이 락은 상대 `TASTY_HOME` 을 만지는 다른 테스트·CWD canary 와 **공유**하므로
    /// 반드시 그 모듈의 `SERIAL` 을 잡아야 그들과 직렬화된다(새 락이면 충돌한다). 락은
    /// `_lock` 필드로 가드 수명 동안 유지되고, Drop 이 env 를 되돌린 뒤 풀린다.
    struct RelativeHomeGuard {
        prev: Option<String>,
        /// 실제로 파일이 떨어질 임시 디렉토리(파일을 만드는 경로에서만 쓴다).
        _dir: Option<tempfile::TempDir>,
        /// 마지막 필드라 Drop::drop(env 복원) 뒤에 떨어져 복원이 락 안에서 난다.
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl RelativeHomeGuard {
        fn set(
            rel: &std::path::Path,
            dir: Option<tempfile::TempDir>,
            lock: std::sync::MutexGuard<'static, ()>,
        ) -> Self {
            let prev = std::env::var("TASTY_HOME").ok();
            // SAFETY: 테스트 프로세스 단독 — SERIAL 락으로 병렬 간섭 차단.
            unsafe { std::env::set_var("TASTY_HOME", rel) };
            Self {
                prev,
                _dir: dir,
                _lock: lock,
            }
        }

        /// CWD 아래의 상대 이름. **파일을 만들지 않는 경로에서만** 쓴다 — 만들면
        /// 레포 워킹트리가 더러워진다.
        fn name(rel: &str) -> Self {
            let lock = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
            Self::set(std::path::Path::new(rel), None, lock)
        }

        /// 임시 디렉토리를 가리키는 **상대** 경로(CWD 에서 `..` 로 거슬러 올라간다).
        /// 상대성은 유지하면서 생성 파일은 임시 디렉토리에 떨어뜨린다.
        fn temp() -> Self {
            // 락을 먼저 잡는다 — 아래 `current_dir()` 읽기까지 직렬화 범위에 넣는다.
            let lock = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
            let dir = tempfile::tempdir().expect("tempdir");
            let cwd = std::env::current_dir().expect("cwd");
            let mut rel = std::path::PathBuf::new();
            for c in cwd.components() {
                if matches!(c, std::path::Component::Normal(_)) {
                    rel.push("..");
                }
            }
            for c in dir.path().components() {
                if let std::path::Component::Normal(seg) = c {
                    rel.push(seg);
                }
            }
            assert!(rel.is_relative(), "가드가 상대 경로를 만들어야 의미가 있다");
            Self::set(&rel, Some(dir), lock)
        }
    }

    impl Drop for RelativeHomeGuard {
        fn drop(&mut self) {
            match &self.prev {
                // SAFETY: 테스트 프로세스 단독 — SERIAL 락으로 병렬 간섭 차단.
                Some(v) => unsafe { std::env::set_var("TASTY_HOME", v) },
                // SAFETY: 상동.
                None => unsafe { std::env::remove_var("TASTY_HOME") },
            }
        }
    }

    fn bash_shell_path() -> String {
        if cfg!(windows) {
            "bash.exe"
        } else {
            "/bin/bash"
        }
        .to_string()
    }

    /// bash `--rcfile` 인자가 **해석된** 루트를 쓰는지 — `tasty_dir()`(env→절대화)로
    /// 해석한 루트를 `bash_rcfile_args_in` 에 넘기는, 프로덕션 진입점
    /// (`effective_shell_args`)과 같은 조립을 파일 생성 없이 재현한다.
    ///
    /// `relative_tasty_home_is_absolutized_for_child_processes` 는 경로 헬퍼만 부르므로
    /// 해석 한 줄(`tasty_dir().as_deref()`)을 지나지 않는다 — 그 한 줄을 미해석 홈으로
    /// 되돌리는 변이가 잡히지 않는다. `--rcfile` 은 자식 셸이 **자기 CWD** 기준으로
    /// 다시 해석하므로 상대 경로가 나가면 통합이 무음으로 죽는다.
    #[test]
    fn bash_rcfile_args_uses_the_resolved_root() {
        // 가드 생성자가 SERIAL 을 직접 쥔다(문자열만 조립하고 파일은 안 만든다 — CWD
        // 아래 상대 이름을 써도 워킹트리가 더러워지지 않는다).
        let _home = RelativeHomeGuard::name("relative-tasty-home");

        let settings = GeneralSettings {
            shell: bash_shell_path(),
            ..GeneralSettings::default()
        };
        let args = bash_rcfile_args_in(&settings, tasty_dir().as_deref());

        let rc = args
            .iter()
            .position(|a| a == "--rcfile")
            .and_then(|i| args.get(i + 1))
            .expect("--rcfile 인자");
        assert!(
            std::path::Path::new(rc).is_absolute(),
            "래퍼가 해석된 루트를 넘겨야 한다 — 상대 --rcfile 은 자식 셸의 CWD 로 \
             재해석된다, got {rc}"
        );
        assert!(
            rc.contains("relative-tasty-home"),
            "설정한 TASTY_HOME 아래를 가리켜야 한다, got {rc}"
        );
    }

    /// `effective_shell_envs`(zsh `ZDOTDIR`)도 같은 래퍼 한 줄을 지난다 — 위 bash
    /// 테스트와 같은 이유로 별도 단정이 필요하다. 상대 `ZDOTDIR` 이 나가면 zsh 가
    /// 자기 CWD 기준으로 wrapper 를 찾아 통합이 무음으로 죽는다.
    ///
    /// 이 경로는 wrapper `.zshenv` 를 **실제로 만든다** — 그래서 상대성을 유지하되
    /// 임시 디렉토리를 가리키는 가드를 쓴다(CWD 를 더럽히지 않는다).
    #[test]
    fn effective_shell_envs_uses_the_resolved_root() {
        // 가드 생성자가 SERIAL 을 직접 쥔다.
        let _home = RelativeHomeGuard::temp();

        let settings = GeneralSettings {
            shell: "zsh".to_string(),
            ..GeneralSettings::default()
        };
        let envs = settings.effective_shell_envs();

        let zdotdir = envs
            .iter()
            .find(|(k, _)| k == "ZDOTDIR")
            .map(|(_, v)| v.as_str())
            .expect("ZDOTDIR");
        assert!(
            std::path::Path::new(zdotdir).is_absolute(),
            "래퍼가 해석된 루트를 넘겨야 한다 — 상대 ZDOTDIR 은 zsh 의 CWD 로 \
             재해석된다, got {zdotdir}"
        );
    }

    // 현재 스탬프를 담은 wrapper .zshenv 는 재생성하지 않는다.
    #[test]
    fn ensure_compiled_zshenv_keeps_current_file() {
        let home = TmpHome::new();
        let path = tasty_zshenv_path_in(Some(home.path())).expect("injected home always resolves");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // 스탬프는 있지만 임의로 조작해 재생성되면 바로 드러나는 sentinel 을 심는다.
        let current = format!("{}\n# SENTINEL\n", BUILTIN_ZSHENV_STAMP);
        std::fs::write(&path, &current).unwrap();

        ensure_compiled_zshenv_in(Some(home.path()));

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("# SENTINEL"),
            "current-stamped file must not be overwritten"
        );
    }
}
