//! 프로세스 인자를 UTF-8 문자열로 받는다.
//!
//! 모든 명령 인자는 `String`이고 IPC로 JSON 문자열을 보내므로, UTF-8이 아닌 인자는 손실 변환
//! (U+FFFD 치환)하지 않고 거절한다. ID·경로·옵션 값이 조용히 바뀌면 다른 대상에 작용할 수 있다.
//! 오류에는 인자 원문 대신 위치만 쓴다. 원문에 메시지 본문이나 토큰이 있을 수 있고, 깨진 바이트를
//! 터미널에 그대로 쓰면 출력도 깨진다.

use std::ffi::OsString;

/// UTF-8이 아닌 첫 인자의 위치.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NonUtf8Arg {
    /// argv[0](실행 이름).
    Program,
    /// 실행 이름 뒤 n번째 인자. 셸의 `$n`처럼 1부터 센다.
    Argument(usize),
}

fn utf8_args(args: impl IntoIterator<Item = OsString>) -> Result<Vec<String>, NonUtf8Arg> {
    args.into_iter()
        .enumerate()
        .map(|(i, arg)| {
            arg.into_string().map_err(|_| match i {
                0 => NonUtf8Arg::Program,
                n => NonUtf8Arg::Argument(n),
            })
        })
        .collect()
}

fn message(err: NonUtf8Arg) -> String {
    match err {
        NonUtf8Arg::Program => tasty_i18n::t("cli.argv.program_not_utf8").to_string(),
        NonUtf8Arg::Argument(n) => tasty_i18n::t_fmt("cli.argv.not_utf8", &n.to_string()),
    }
}

/// 프로세스 인자를 읽는다. UTF-8이 아닌 인자가 있으면 그 위치를 stderr에 알리고
/// 사용 오류(종료 코드 2)로 끝낸다. 도움말·명령 해석·호스트 시작보다 먼저 부른다.
pub fn process_args_or_exit() -> Vec<String> {
    match utf8_args(std::env::args_os()) {
        Ok(args) => args,
        Err(err) => {
            crate::out::errln!("{}", message(err));
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn broken(bytes: &[u8]) -> OsString {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(bytes.to_vec())
    }

    #[cfg(unix)]
    #[test]
    fn reports_the_position_of_a_non_utf8_argument_counting_from_one() {
        let argv = vec![
            OsString::from("tasty"),
            OsString::from("send"),
            broken(b"a\xc2b"),
        ];
        assert_eq!(utf8_args(argv), Err(NonUtf8Arg::Argument(2)));
    }

    #[cfg(unix)]
    #[test]
    fn reports_the_first_of_several_non_utf8_arguments() {
        let argv = vec![
            OsString::from("tasty"),
            OsString::from("send"),
            broken(b"\xff"),
            broken(b"\xc2"),
        ];
        assert_eq!(utf8_args(argv), Err(NonUtf8Arg::Argument(2)));
    }

    #[cfg(unix)]
    #[test]
    fn reports_a_non_utf8_program_name_apart_from_arguments() {
        let argv = vec![broken(b"tas\xc2ty"), broken(b"\xc2")];
        assert_eq!(utf8_args(argv), Err(NonUtf8Arg::Program));
    }

    #[cfg(windows)]
    #[test]
    fn reports_the_position_of_an_unpaired_surrogate() {
        use std::os::windows::ffi::OsStringExt;
        let argv = vec![
            OsString::from("tasty"),
            OsString::from_wide(&[0x61, 0xD800]),
        ];
        assert_eq!(utf8_args(argv), Err(NonUtf8Arg::Argument(1)));
    }

    #[test]
    fn keeps_valid_non_ascii_arguments_unchanged() {
        let argv = ["tasty", "send", "·", "한글"].map(OsString::from);
        assert_eq!(
            utf8_args(argv),
            Ok(vec![
                "tasty".to_string(),
                "send".to_string(),
                "·".to_string(),
                "한글".to_string()
            ])
        );
    }

    #[test]
    fn message_names_the_position_without_the_argument() {
        let text = message(NonUtf8Arg::Argument(5));
        assert!(text.contains('5'), "{text}");
        assert_ne!(message(NonUtf8Arg::Program), text);
    }
}
