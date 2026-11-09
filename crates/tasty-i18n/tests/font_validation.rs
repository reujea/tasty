#[test]
fn font_bytes_are_validated_before_they_leave_the_reader() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("font.ttf");
    assert!(matches!(
        tasty_i18n::font::read_validated(&path, 0),
        Err(tasty_i18n::font::LocaleFontError::Read(_))
    ));
    std::fs::write(&path, b"broken font").unwrap();
    assert!(matches!(
        tasty_i18n::font::read_validated(&path, 0),
        Err(tasty_i18n::font::LocaleFontError::Parse)
    ));
    let valid = include_bytes!("../../tasty-font/assets/D2Coding-ligature-Regular.ttf");
    std::fs::write(&path, valid).unwrap();
    assert_eq!(tasty_i18n::font::read_validated(&path, 0).unwrap(), valid);
}

const VALID: &[u8] = include_bytes!("../../tasty-font/assets/D2Coding-ligature-Regular.ttf");

/// Returns the font with its `hhea` ascender and descender replaced.
fn with_hhea_height(ascender: i16, descender: i16) -> Vec<u8> {
    let mut font = VALID.to_vec();
    let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
    let record = (0..tables)
        .map(|i| 12 + 16 * i)
        .find(|&r| &font[r..r + 4] == b"hhea")
        .unwrap();
    let hhea = u32::from_be_bytes(font[record + 8..record + 12].try_into().unwrap()) as usize;
    font[hhea + 4..hhea + 6].copy_from_slice(&ascender.to_be_bytes());
    font[hhea + 6..hhea + 8].copy_from_slice(&descender.to_be_bytes());
    font
}

// epaint scales glyphs by this height and asserts the result is positive when
// it first lays out text, so a parser-only check lets these files through.
#[test]
fn a_negative_line_height_is_refused_by_the_reader() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("font.ttf");
    std::fs::write(&path, with_hhea_height(-1000, 1000)).unwrap();
    assert!(matches!(
        tasty_i18n::font::read_validated(&path, 0),
        Err(tasty_i18n::font::LocaleFontError::NoLineHeight)
    ));
}

#[test]
fn a_zero_line_height_is_refused() {
    let mut font = with_hhea_height(0, 0);
    // A zero hhea height falls back to OS/2, so both tables have to be empty.
    let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
    let record = (0..tables)
        .map(|i| 12 + 16 * i)
        .find(|&r| &font[r..r + 4] == b"OS/2")
        .unwrap();
    let os2 = u32::from_be_bytes(font[record + 8..record + 12].try_into().unwrap()) as usize;
    for field in [68, 70, 74, 76] {
        font[os2 + field..os2 + field + 2].fill(0);
    }
    assert!(matches!(
        tasty_i18n::font::validate(&font),
        Err(tasty_i18n::font::LocaleFontError::NoLineHeight)
    ));
}

#[test]
fn a_small_positive_line_height_is_accepted() {
    assert!(tasty_i18n::font::validate(&with_hhea_height(1, -1)).is_ok());
}
