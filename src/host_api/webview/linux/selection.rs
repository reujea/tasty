//! 편집 바인딩을 web process 확장과 주고받는다. 키 경로는 키를 무장한 뒤 입력 순서대로 확장의 keydown
//! 리스너가 답하고, host가 키 없이 시작한 Copy·Cut은 포커스 frame의 선택을 직접 묻는다.
//! 계약: `docs/design/systems/webview.md`의 편집 액션 절.
//!
//! 확장(`selection_extension.c`)은 build.rs가 빌드해 본체에 내장한다. 실행 시 Tasty 홈의 전용
//! 디렉터리에 꺼내고, WebContext가 첫 web process를 띄우기 전에 그 디렉터리를 지정한다.

use std::cell::Cell;
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Duration;

use gtk::prelude::*;
use sha2::Digest as _;
use webkit2gtk::gio::Cancellable;
use webkit2gtk::glib;
use webkit2gtk::{
    UserMessage, UserMessageError, UserMessageExt as _, WebContext, WebContextExt as _, WebView,
    WebViewExt as _,
};

const MODULE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/libtasty-webkit-selection.so"));
const MODULE_NAME: &str = "libtasty-webkit-selection.so";
const QUERY_NAME: &str = "tasty-selection-query";
const ARM_NAME: &str = "tasty-edit-arm";
const UNCLAIM_NAME: &str = "tasty-edit-unclaim";
const KEY_NAME: &str = "tasty-edit-key";
const READY_NAME: &str = "tasty-edit-ready";
/// 확장과 같은 수정자 비트. 확장은 네 비트를 모두 정확히 비교한다.
const MOD_CTRL: u32 = 1;
const MOD_SHIFT: u32 = 2;
const MOD_ALT: u32 = 4;
const MOD_META: u32 = 8;
/// keyCode가 없는 키를 keyIdentifier code point로 맞출 때 붙이는 표지. 확장의 `IDENTIFIER_KEY`와 같다.
const IDENTIFIER_KEY: u32 = 0x80_0000;
/// 측정한 응답은 2~34ms였다. 이보다 늦으면 명령을 실행하지 않는다.
const REPLY_TIMEOUT: Duration = Duration::from_secs(1);

/// 질의가 범위 선택을 확인하지 못한 이유. 어느 경우든 Copy·Cut을 실행하지 않는다.
#[derive(Debug)]
pub(super) enum QueryError {
    /// 이 WebView의 web process에 확장이 없다.
    Unavailable,
    /// 제한 시간 안에 응답이 없었다.
    TimedOut,
    /// 입력·로드·폐기로 질의가 취소됐다.
    Cancelled,
    /// 다른 질의나 다른 page의 응답이다.
    Mismatch,
    /// 기다리는 무장이 상한을 넘어 확장이 이 page의 순서 경로를 닫았다. 소유한 키는 명령 없이 막힌다.
    Overflow,
    Failed(glib::Error),
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => f.write_str("the selection extension is not loaded"),
            Self::TimedOut => write!(f, "no reply within {} ms", REPLY_TIMEOUT.as_millis()),
            Self::Cancelled => f.write_str("query cancelled"),
            Self::Mismatch => f.write_str("reply for another query or page"),
            Self::Overflow => f.write_str("more than 64 edit keys waited"),
            Self::Failed(e) => write!(f, "query failed: {e}"),
        }
    }
}

/// 이 WebContext의 web process가 선택 확장을 불러오게 한다. 첫 web process를 띄우기 전에 불러야 한다.
/// 확장을 준비하지 못했으면 지정하지 않으며, 그 경우 질의는 `Unavailable`로 끝난다.
pub(in crate::host_api::webview) fn install(context: &WebContext) {
    if let Some(dir) = extension_dir() {
        context.set_web_extensions_directory(&dir.to_string_lossy());
    }
}

/// 기본 WebContext에 확장을 지정한다. 같은 디렉터리를 다시 지정해도 동작은 같다.
pub(super) fn install_default() {
    if let Some(context) = WebContext::default() {
        install(&context);
    }
}

fn extension_dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let prepared = tasty_utils::path::tasty_home()
            .ok_or_else(|| std::io::Error::other("no Tasty home"))
            .and_then(|home| prepare(&home.join("webkit-extensions"), MODULE));
        match prepared {
            Ok(dir) => {
                tracing::debug!(?dir, "webview selection extension ready");
                Some(dir)
            }
            Err(e) => {
                tracing::warn!(
                    "webview selection extension unavailable ({e}); Copy and Cut stay inert"
                );
                None
            }
        }
    })
    .as_deref()
}

/// 내용 해시별 디렉터리에 모듈 하나만 둔다. WebKit은 디렉터리의 모든 `.so`를 불러오므로
/// 이 사용자 소유의 0700(그룹·기타 권한 없음)이 아니거나 다른 항목이 있으면 쓰지 않는다.
/// 실행 중인 다른 버전의 모듈을 덮어쓰지 않도록 내용이 다르면 다른 디렉터리를 쓴다.
fn prepare(root: &Path, module: &[u8]) -> std::io::Result<PathBuf> {
    let digest = sha2::Sha256::digest(module);
    let hash: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    let dir = root.join(hash);
    for path in [root, dir.as_path()] {
        match std::fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        let meta = std::fs::symlink_metadata(path)?;
        // SAFETY: geteuid는 인자 없이 현재 프로세스의 유효 uid만 읽는다.
        let uid = unsafe { libc::geteuid() };
        if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
            return Err(std::io::Error::other(format!(
                "{} is not a private directory of this user",
                path.display()
            )));
        }
    }
    let target = dir.join(MODULE_NAME);
    for entry in std::fs::read_dir(&dir)? {
        let name = entry?.file_name();
        if name != MODULE_NAME {
            return Err(std::io::Error::other(format!(
                "unexpected entry {name:?} in {}",
                dir.display()
            )));
        }
    }
    let current = std::fs::symlink_metadata(&target)
        .ok()
        .filter(|meta| meta.is_file())
        .and_then(|_| std::fs::read(&target).ok());
    if current.as_deref() != Some(module) {
        publish(root, &target, module)?;
    }
    Ok(dir)
}

/// 디렉터리 밖의 새 파일에 쓰고 이름만 바꿔, 쓰는 도중의 파일을 web process가 불러오지 않게 한다.
/// 스테이징 이름은 이 호출만의 것이고 이미 있으면(링크 포함) 열지 않는다. rename은 대상 이름을
/// 원자적으로 바꾸며 링크를 따라가지 않으므로, 같은 홈의 다른 프로세스와 겹쳐도 완성된 모듈 하나만 남는다.
fn publish(root: &Path, target: &Path, module: &[u8]) -> std::io::Result<()> {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let staging = root.join(format!(
        ".{MODULE_NAME}.{}.{sequence}.{nanos}.tmp",
        std::process::id()
    ));
    // 열지 못했으면 그 경로는 이 호출이 만든 것이 아니므로 지우지 않는다.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&staging)?;
    let written = file
        .write_all(module)
        .and_then(|()| file.sync_all())
        .and_then(|()| std::fs::rename(&staging, target));
    if written.is_err()
        && let Err(e) = std::fs::remove_file(&staging)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(
            ?staging,
            "webview selection extension: staging file left behind: {e}"
        );
    }
    written
}

/// 포커스 frame에 범위 선택이 있는지 묻는다. `nonce`와 page id가 질의와 같은 응답만 받아들인다.
/// `cancellable`을 취소하면 `Cancelled`, 제한 시간이 지나면 `TimedOut`으로 끝난다.
pub(super) fn query(
    webview: &WebView,
    nonce: u64,
    cancellable: &Cancellable,
    done: impl FnOnce(Result<bool, QueryError>) + 'static,
) {
    let page = webview.page_id();
    let message = UserMessage::new(QUERY_NAME, Some(&(nonce,).to_variant()));
    // 타이머가 실제로 실행됐는지 따로 기록한다. 다른 이유의 취소로 끝나면 남은 타이머를 지운다.
    let fired = Rc::new(Cell::new(false));
    let timer = {
        let fired = fired.clone();
        let cancellable = cancellable.clone();
        glib::timeout_add_local_once(REPLY_TIMEOUT, move || {
            fired.set(true);
            cancellable.cancel();
        })
    };
    let timer = Cell::new(Some(timer));
    let target = webview.clone();
    webview.send_message_to_page(&message, Some(cancellable), move |result| {
        if let Some(timer) = timer.take()
            && !fired.get()
        {
            timer.remove();
        }
        done(read_reply(
            result,
            fired.get(),
            nonce,
            page,
            target.page_id(),
        ));
    });
}

/// 키 누름을 확장에 무장한다. 그 키가 입력 순서대로 문서에 도착하면 확장이 [`ViewMessage::Key`]로
/// 답한다. 무장 메시지가 키보다 먼저 가도 무장만 하므로 순서와 무관하다. `done`은 무장 실패만 알린다.
/// `needs_range`(Copy·Cut)이면 확장은 범위 선택이 없을 때 그 키를 막지 않고 엔진·페이지로 보낸다.
pub(super) fn arm(
    webview: &WebView,
    nonce: u64,
    key: ArmKey,
    needs_range: bool,
    done: impl FnOnce(Result<(), QueryError>) + 'static,
) {
    let page = webview.page_id();
    let message = UserMessage::new(
        ARM_NAME,
        Some(&(nonce, key.code, key.mods, needs_range).to_variant()),
    );
    let target = webview.clone();
    webview.send_message_to_page(&message, None::<&Cancellable>, move |result| {
        let armed = match result {
            Ok(reply) => match reply.parameters().and_then(|p| p.get::<(u64, u64, bool)>()) {
                Some((n, p, false)) if n == nonce && p == page => Err(QueryError::Overflow),
                Some((n, p, true)) if n == nonce && p == page && target.page_id() == page => Ok(()),
                _ => Err(QueryError::Mismatch),
            },
            Err(e) if e.matches(UserMessageError::Message) => Err(QueryError::Unavailable),
            Err(e) => Err(QueryError::Failed(e)),
        };
        done(armed);
    });
}

/// 바인딩에서 빠진 조합을 확장의 소유 목록에서 지운다. 그 키보다 먼저 도착하므로 그 키는 엔진 기본
/// 동작으로 간다.
pub(super) fn unclaim(webview: &WebView, key: ArmKey) {
    let message = UserMessage::new(UNCLAIM_NAME, Some(&(key.code, key.mods).to_variant()));
    webview.send_message_to_page(&message, None::<&Cancellable>, |result| {
        if let Err(e) = result {
            tracing::warn!("webview: edit key unclaim failed: {e}");
        }
    });
}

/// Super·Hyper·MOD4가 든 키 이벤트에 GDK META_MASK를 더한다. WebKit의 기본 처리보다 먼저 불러야 한다.
/// WebKitGTK는 META_MASK만 DOM `metaKey`로 옮기므로, 더하지 않으면 Ctrl+Super+C가 DOM에서 Ctrl+C와
/// 같아진다. 원래 비트는 지우지 않는다: 지우면 엔진의 Ctrl+C 바인딩이 Ctrl+Super+C에 맞는다(측정).
/// UI 분류는 META_MASK를 쓰지 않으므로 WebKit이 다시 보낸 키도 같은 키로 분류된다.
pub(super) fn add_meta_for_webkit(event: &gtk::gdk::EventKey) {
    if !needs_meta(event.state()) {
        return;
    }
    let raw: *const gtk::gdk::ffi::GdkEventKey = glib::translate::ToGlibPtr::to_glib_none(event).0;
    // SAFETY: 신호가 끝날 때까지 GDK가 소유하는 유효한 키 이벤트다. GTK가 이 이벤트를 다음 처리기에
    // 그대로 넘기므로 제자리에서 수정자 비트만 더한다.
    unsafe {
        (*raw.cast_mut()).state |= gtk::gdk::ffi::GDK_META_MASK;
    }
}

fn needs_meta(state: gtk::gdk::ModifierType) -> bool {
    use gtk::gdk::ModifierType as M;
    state.intersects(M::SUPER_MASK | M::HYPER_MASK | M::MOD4_MASK) && !state.contains(M::META_MASK)
}

/// 확장이 확인할 키. 코드는 WebKitGTK가 keydown의 keyCode로 쓰는 Windows 가상 키 코드다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ArmKey {
    code: u32,
    mods: u32,
}

impl ArmKey {
    /// 코드로 옮길 수 없는 키는 None이다. 그 키의 Copy·Cut은 순서를 보장할 수 없어 실행하지 않는다.
    /// Super·Hyper·MOD4·Meta는 Meta 비트다. [`add_meta_for_webkit`]가 DOM `metaKey`를 그렇게 맞춘다.
    pub(super) fn from_gdk(
        keyval: gtk::gdk::keys::Key,
        state: gtk::gdk::ModifierType,
    ) -> Option<Self> {
        use gtk::gdk::ModifierType as M;
        let code = windows_key_code(*keyval).or_else(|| identifier_key(&keyval))?;
        let mut mods = 0;
        for (mask, bit) in [
            (gtk::gdk::ModifierType::CONTROL_MASK, MOD_CTRL),
            (gtk::gdk::ModifierType::SHIFT_MASK, MOD_SHIFT),
            (gtk::gdk::ModifierType::MOD1_MASK, MOD_ALT),
            (
                M::SUPER_MASK | M::HYPER_MASK | M::MOD4_MASK | M::META_MASK,
                MOD_META,
            ),
        ] {
            if state.intersects(mask) {
                mods |= bit;
            }
        }
        Some(Self { code, mods })
    }
}

/// WebKitGTK가 keyCode를 주지 않는 키(키릴 문자 등)의 키. keydown의 keyIdentifier는 keyval을 대문자로 바꾼
/// Unicode `U+XXXX`이며, 확장은 ASCII가 아닌 그 code point에 같은 표지를 붙여 맞춘다.
fn identifier_key(keyval: &gtk::gdk::keys::Key) -> Option<u32> {
    let cp = keyval.to_upper().to_unicode()? as u32;
    (cp > 0x7F).then_some(IDENTIFIER_KEY | cp)
}

/// WebKitGTK 2.50 `windowsKeyCodeForGdkKeyCode`에서 바인딩으로 받을 수 있는 keyval의 부분. 글자는 대소문자,
/// 숫자·기호 키는 Shift 전후 keyval이 같은 코드다(US 배치 기준이며 WebKit 표를 그대로 따른다).
/// 표에 없는 keyval(F25 이상, KP_Space·KP_Equal 등)은 WebKit도 keyCode 0을 주므로 None이다.
fn windows_key_code(keyval: u32) -> Option<u32> {
    use gtk::gdk::keys::constants as k;
    let ascii = char::from_u32(keyval).filter(char::is_ascii_alphanumeric);
    if let Some(c) = ascii {
        return Some(c.to_ascii_uppercase() as u32);
    }
    let key = gtk::gdk::keys::Key::from(keyval);
    if (*k::F1..=*k::F24).contains(&keyval) {
        return Some(0x70 + (keyval - *k::F1));
    }
    if (*k::KP_0..=*k::KP_9).contains(&keyval) {
        return Some(0x60 + (keyval - *k::KP_0));
    }
    let code = match key {
        k::BackSpace => 0x08,
        k::Tab | k::ISO_Left_Tab | k::_3270_BackTab => 0x09,
        k::Return | k::KP_Enter | k::ISO_Enter => 0x0D,
        k::Escape => 0x1B,
        k::space => 0x20,
        k::Page_Up | k::KP_Page_Up => 0x21,
        k::Page_Down | k::KP_Page_Down => 0x22,
        k::End | k::KP_End => 0x23,
        k::Home | k::KP_Home => 0x24,
        k::Left | k::KP_Left => 0x25,
        k::Up | k::KP_Up => 0x26,
        k::Right | k::KP_Right => 0x27,
        k::Down | k::KP_Down => 0x28,
        k::Insert | k::KP_Insert => 0x2D,
        k::Delete | k::KP_Delete => 0x2E,
        k::parenright => 0x30,
        k::exclam => 0x31,
        k::at => 0x32,
        k::numbersign => 0x33,
        k::dollar => 0x34,
        k::percent => 0x35,
        k::asciicircum => 0x36,
        k::ampersand => 0x37,
        k::asterisk => 0x38,
        k::parenleft => 0x39,
        k::KP_Multiply => 0x6A,
        k::KP_Add => 0x6B,
        k::KP_Subtract => 0x6D,
        k::KP_Decimal => 0x6E,
        k::KP_Divide => 0x6F,
        k::semicolon | k::colon => 0xBA,
        k::plus | k::equal => 0xBB,
        k::comma | k::less => 0xBC,
        k::minus | k::underscore => 0xBD,
        k::period | k::greater => 0xBE,
        k::slash | k::question => 0xBF,
        k::asciitilde | k::grave => 0xC0,
        k::bracketleft | k::braceleft => 0xDB,
        k::backslash | k::bar => 0xDC,
        k::bracketright | k::braceright => 0xDD,
        k::apostrophe | k::quotedbl => 0xDE,
        _ => return None,
    };
    Some(code)
}

/// 확장이 WebView로 보낸 메시지.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ViewMessage {
    /// 이 page의 새 문서에 리스너를 붙였다.
    Ready { page: u64 },
    /// 무장한 키가 입력 순서대로 문서에 도착했다. `range`는 그 시점 그 문서의 범위 선택 여부다.
    Key { nonce: u64, page: u64, range: bool },
}

impl ViewMessage {
    pub(super) fn parse(message: &UserMessage) -> Option<Self> {
        let params = message.parameters();
        match message.name()?.as_str() {
            READY_NAME => params?.get::<(u64,)>().map(|(page,)| Self::Ready { page }),
            KEY_NAME => params?
                .get::<(u64, u64, bool)>()
                .map(|(nonce, page, range)| Self::Key { nonce, page, range }),
            _ => None,
        }
    }
}

fn read_reply(
    result: Result<UserMessage, glib::Error>,
    timed_out: bool,
    nonce: u64,
    page: u64,
    page_now: u64,
) -> Result<bool, QueryError> {
    let reply = match result {
        Ok(reply) => reply,
        Err(e) if e.matches(webkit2gtk::gio::IOErrorEnum::Cancelled) => {
            return Err(if timed_out {
                QueryError::TimedOut
            } else {
                QueryError::Cancelled
            });
        }
        Err(e) if e.matches(UserMessageError::Message) => return Err(QueryError::Unavailable),
        Err(e) => return Err(QueryError::Failed(e)),
    };
    let Some((reply_nonce, reply_page, range)) = reply
        .parameters()
        .and_then(|params| params.get::<(u64, u64, bool)>())
    else {
        return Err(QueryError::Mismatch);
    };
    accept(reply_nonce, reply_page, range, nonce, page, page_now)
}

/// 질의를 보낸 page가 응답 시점에도 같은 page이고 응답이 이 질의의 것일 때만 받아들인다.
fn accept(
    reply_nonce: u64,
    reply_page: u64,
    range: bool,
    nonce: u64,
    page: u64,
    page_now: u64,
) -> Result<bool, QueryError> {
    if reply_nonce != nonce || reply_page != page || page_now != page {
        return Err(QueryError::Mismatch);
    }
    Ok(range)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 편집 바인딩에 쓰는 키는 WebKitGTK keyCode로 옮기고, 옮길 수 없는 키는 None이다.
    #[test]
    fn bound_keys_map_to_webkit_key_codes() {
        use gtk::gdk::keys::constants as k;
        let code = |key: gtk::gdk::keys::Key| windows_key_code(*key);
        assert_eq!(code(k::c), Some(0x43));
        assert_eq!(code(k::C), Some(0x43));
        assert_eq!(code(k::_7), Some(0x37));
        assert_eq!(code(k::Insert), Some(0x2D));
        assert_eq!(code(k::Delete), Some(0x2E));
        assert_eq!(code(k::F5), Some(0x74));
        assert_eq!(code(k::F24), Some(0x87));
        assert_eq!(code(k::F25), None, "WebKit 표는 F24까지다");
        assert_eq!(code(k::KP_Space), None, "WebKit이 keyCode를 주지 않는다");
        assert_eq!(code(k::KP_Equal), None);
        let key = ArmKey::from_gdk(
            k::c,
            gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SUPER_MASK,
        );
        assert_eq!(
            key,
            Some(ArmKey {
                code: 0x43,
                mods: MOD_CTRL | MOD_META
            }),
            "Super는 Meta 비트로 구분한다"
        );
        let plain = ArmKey::from_gdk(k::c, gtk::gdk::ModifierType::CONTROL_MASK);
        assert_ne!(key, plain, "Ctrl+Super+C와 Ctrl+C는 다른 키다");
        let mod4 = ArmKey::from_gdk(k::c, gtk::gdk::ModifierType::MOD4_MASK);
        assert_eq!(
            mod4,
            Some(ArmKey {
                code: 0x43,
                mods: MOD_META
            })
        );
        let mut state = gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SUPER_MASK;
        assert!(needs_meta(state));
        state |= gtk::gdk::ModifierType::META_MASK;
        assert!(!needs_meta(state), "이미 더했으면 다시 더하지 않는다");
        assert!(!needs_meta(gtk::gdk::ModifierType::CONTROL_MASK));
        let key = ArmKey::from_gdk(
            k::c,
            gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::MOD2_MASK,
        );
        assert_eq!(
            key,
            Some(ArmKey {
                code: 0x43,
                mods: MOD_CTRL
            }),
            "NumLock은 수정자가 아니다"
        );
    }

    /// 바인딩으로 받을 수 있는 이름 키·기호·keypad 키는 WebKitGTK 2.50 표의 keyCode와 같다.
    /// 기호는 Shift 전후 keyval이 같은 코드이고, NumLock 여부에 따라 keypad keyval이 달라도 표를 따른다.
    #[test]
    fn named_symbol_and_keypad_keys_follow_the_webkit_table() {
        use gtk::gdk::keys::constants as k;
        let table = [
            (k::BackSpace, 0x08),
            (k::Tab, 0x09),
            (k::ISO_Left_Tab, 0x09),
            (k::Return, 0x0D),
            (k::KP_Enter, 0x0D),
            (k::Escape, 0x1B),
            (k::space, 0x20),
            (k::Page_Up, 0x21),
            (k::KP_Page_Up, 0x21),
            (k::Page_Down, 0x22),
            (k::KP_Page_Down, 0x22),
            (k::End, 0x23),
            (k::KP_End, 0x23),
            (k::Home, 0x24),
            (k::KP_Home, 0x24),
            (k::Left, 0x25),
            (k::KP_Left, 0x25),
            (k::Up, 0x26),
            (k::KP_Up, 0x26),
            (k::Right, 0x27),
            (k::KP_Right, 0x27),
            (k::Down, 0x28),
            (k::KP_Down, 0x28),
            (k::Insert, 0x2D),
            (k::KP_Insert, 0x2D),
            (k::Delete, 0x2E),
            (k::KP_Delete, 0x2E),
            (k::_0, 0x30),
            (k::parenright, 0x30),
            (k::_1, 0x31),
            (k::exclam, 0x31),
            (k::at, 0x32),
            (k::numbersign, 0x33),
            (k::dollar, 0x34),
            (k::percent, 0x35),
            (k::asciicircum, 0x36),
            (k::ampersand, 0x37),
            (k::asterisk, 0x38),
            (k::_9, 0x39),
            (k::parenleft, 0x39),
            (k::a, 0x41),
            (k::Z, 0x5A),
            (k::KP_0, 0x60),
            (k::KP_9, 0x69),
            (k::KP_Multiply, 0x6A),
            (k::KP_Add, 0x6B),
            (k::KP_Subtract, 0x6D),
            (k::KP_Decimal, 0x6E),
            (k::KP_Divide, 0x6F),
            (k::F1, 0x70),
            (k::F12, 0x7B),
            (k::F13, 0x7C),
            (k::F24, 0x87),
            (k::semicolon, 0xBA),
            (k::colon, 0xBA),
            (k::equal, 0xBB),
            (k::plus, 0xBB),
            (k::comma, 0xBC),
            (k::less, 0xBC),
            (k::minus, 0xBD),
            (k::underscore, 0xBD),
            (k::period, 0xBE),
            (k::greater, 0xBE),
            (k::slash, 0xBF),
            (k::question, 0xBF),
            (k::grave, 0xC0),
            (k::asciitilde, 0xC0),
            (k::bracketleft, 0xDB),
            (k::braceleft, 0xDB),
            (k::backslash, 0xDC),
            (k::bar, 0xDC),
            (k::bracketright, 0xDD),
            (k::braceright, 0xDD),
            (k::apostrophe, 0xDE),
            (k::quotedbl, 0xDE),
        ];
        for (key, vk) in table {
            assert_eq!(windows_key_code(*key), Some(vk), "{key:?}");
        }
        let shifted_one = ArmKey::from_gdk(
            k::exclam,
            gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SHIFT_MASK,
        );
        assert_eq!(
            shifted_one,
            Some(ArmKey {
                code: 0x31,
                mods: MOD_CTRL | MOD_SHIFT
            }),
            "Ctrl+Shift+1은 keyval이 !여도 1 키다"
        );
    }

    /// keyCode가 없는 키릴 문자는 대문자 code point로 맞추고, ASCII나 Unicode가 없는 키는 쓰지 않는다.
    #[test]
    fn keys_without_a_key_code_use_the_identifier_code_point() {
        use gtk::gdk::keys::constants as k;
        let ctrl = gtk::gdk::ModifierType::CONTROL_MASK;
        let es = ArmKey::from_gdk(k::Cyrillic_es, ctrl);
        assert_eq!(
            es,
            Some(ArmKey {
                code: IDENTIFIER_KEY | 0x0421,
                mods: MOD_CTRL
            })
        );
        let shifted = ArmKey::from_gdk(k::Cyrillic_ES, ctrl | gtk::gdk::ModifierType::SHIFT_MASK);
        assert_eq!(shifted.map(|k| k.code), Some(IDENTIFIER_KEY | 0x0421));
        assert_eq!(
            ArmKey::from_gdk(k::Cyrillic_ef, ctrl).map(|k| k.code),
            Some(IDENTIFIER_KEY | 0x0424)
        );
        assert_eq!(
            ArmKey::from_gdk(k::c, ctrl).map(|k| k.code),
            Some(0x43),
            "VK가 먼저다"
        );
        assert_eq!(
            ArmKey::from_gdk(k::slash, ctrl).map(|k| k.code),
            Some(0xBF),
            "ASCII 기호는 VK가 따로 있다"
        );
        assert_eq!(ArmKey::from_gdk(k::dead_acute, ctrl), None);
        assert_eq!(ArmKey::from_gdk(k::Home, ctrl).map(|k| k.code), Some(0x24));
    }

    /// 다른 질의·page의 응답이나 응답 사이에 page가 바뀐 경우는 버린다.
    #[test]
    fn only_the_reply_to_this_query_on_this_page_counts() {
        assert!(matches!(accept(7, 3, true, 7, 3, 3), Ok(true)));
        assert!(matches!(accept(7, 3, false, 7, 3, 3), Ok(false)));
        assert!(matches!(
            accept(6, 3, true, 7, 3, 3),
            Err(QueryError::Mismatch)
        ));
        assert!(matches!(
            accept(7, 4, true, 7, 3, 3),
            Err(QueryError::Mismatch)
        ));
        assert!(matches!(
            accept(7, 3, true, 7, 3, 4),
            Err(QueryError::Mismatch)
        ));
    }

    fn scratch(name: &str) -> PathBuf {
        // 같은 프로세스의 다른 시험과 겹치지 않도록 호출마다 번호를 붙인다.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tasty-selection-ext-{name}-{}-{n}",
            std::process::id()
        ));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("clear scratch");
        }
        dir
    }

    /// 꺼낸 모듈은 사용자 전용 디렉터리에 하나만 있고, 내용이 바뀌면 다른 디렉터리를 쓴다.
    #[test]
    fn the_module_is_extracted_alone_into_a_private_directory() {
        let root = scratch("extract");
        let dir = prepare(&root, b"module one").expect("prepare");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .expect("read")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(names, [MODULE_NAME]);
        assert_eq!(
            std::fs::read(dir.join(MODULE_NAME)).expect("read"),
            b"module one"
        );
        let mode = std::fs::metadata(&dir).expect("meta").mode() & 0o777;
        assert_eq!(mode, 0o700);
        assert_eq!(prepare(&root, b"module one").expect("again"), dir);
        let other = prepare(&root, b"module two").expect("other");
        assert_ne!(other, dir, "다른 내용은 실행 중인 모듈을 덮어쓰지 않는다");
        std::fs::write(dir.join(MODULE_NAME), b"tampered").expect("tamper");
        prepare(&root, b"module one").expect("repair");
        assert_eq!(
            std::fs::read(dir.join(MODULE_NAME)).expect("read"),
            b"module one",
            "바뀐 모듈은 내장본으로 되돌린다"
        );
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    /// WebKit이 함께 불러올 다른 항목이 있거나 다른 사용자가 쓸 수 있는 디렉터리는 쓰지 않는다.
    #[test]
    fn a_directory_with_foreign_entries_or_loose_permissions_is_refused() {
        let root = scratch("refuse");
        let dir = prepare(&root, b"module").expect("prepare");
        std::fs::write(dir.join("other.so"), b"x").expect("plant");
        assert!(prepare(&root, b"module").is_err());
        std::fs::remove_file(dir.join("other.so")).expect("unplant");
        for mode in [0o777, 0o755] {
            std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(mode))
                .expect("chmod");
            assert!(prepare(&root, b"module").is_err(), "mode {mode:o}");
        }
        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
