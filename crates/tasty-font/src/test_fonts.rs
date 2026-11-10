//! 시험에서 쓰는 글꼴 입력. 묶음 D2Coding을 고쳐 만들어 저장소에 바이너리를 넣지 않는다.

use crate::D2CODING_REGULAR_TTF as VALID;

pub(crate) fn table_record(font: &[u8], tag: &[u8; 4]) -> usize {
    let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
    (0..tables)
        .map(|i| 12 + 16 * i)
        .find(|&r| &font[r..r + 4] == tag)
        .unwrap()
}

pub(crate) fn table(font: &[u8], tag: &[u8; 4]) -> (usize, usize) {
    let r = table_record(font, tag);
    let word = |at: usize| u32::from_be_bytes(font[at..at + 4].try_into().unwrap()) as usize;
    (word(r + 8), word(r + 12))
}

/// 표 레코드 이름을 바꿔 파서가 그 표를 찾지 못하게 한다.
pub(crate) fn without_table(tag: &[u8; 4]) -> Vec<u8> {
    remove_table(VALID.to_vec(), tag)
}

/// `without_table` 을 다른 글꼴에 적용한다.
pub(crate) fn remove_table(mut font: Vec<u8>, tag: &[u8; 4]) -> Vec<u8> {
    let record = table_record(&font, tag);
    font[record] = b'x';
    font
}

/// name 표의 글꼴 이름을 바꾼다. `D2Coding` 이 시스템 목록과 겹치지 않게 하는 데 쓴다.
pub(crate) fn renamed(font: Vec<u8>) -> Vec<u8> {
    renamed_as(font, "D2Codinh")
}

/// name 표의 `D2Coding` 을 같은 길이의 `to` 로 바꾼다.
pub(crate) fn renamed_as(mut font: Vec<u8>, to: &str) -> Vec<u8> {
    assert_eq!(
        to.len(),
        "D2Coding".len(),
        "이름 길이가 같아야 표 길이가 그대로다"
    );
    let (name, len) = table(&font, b"name");
    let utf16 = |s: &str| {
        s.encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<u8>>()
    };
    let pairs = [
        (b"D2Coding".to_vec(), to.as_bytes().to_vec()),
        (utf16("D2Coding"), utf16(to)),
    ];
    for (from, to) in pairs {
        for start in name..name + len - from.len() {
            if font[start..start + from.len()] == from[..] {
                font[start..start + to.len()].copy_from_slice(&to);
            }
        }
    }
    font
}

/// `renamed` 가 만든 family.
pub(crate) const RENAMED_FAMILY: &str = "D2Codinh ligature";

/// head 표의 unitsPerEm 을 절반으로 줄여 글자가 두 배로 그려지게 한다. 칸 폭으로 묶음 D2Coding
/// 과 구별할 수 있다.
pub(crate) fn half_units_per_em(mut font: Vec<u8>) -> Vec<u8> {
    let (head, _) = table(&font, b"head");
    let units = u16::from_be_bytes([font[head + 18], font[head + 19]]);
    font[head + 18..head + 20].copy_from_slice(&(units / 2).to_be_bytes());
    font
}

/// 글꼴 파일들을 묶음 파일(ttc) 하나로 합친다. 각 면의 표 위치를 파일 처음 기준으로 옮긴다.
pub(crate) fn collection(fonts: &[Vec<u8>]) -> Vec<u8> {
    let header = 12 + 4 * fonts.len();
    let mut out = Vec::new();
    out.extend_from_slice(b"ttcf");
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&(fonts.len() as u32).to_be_bytes());
    let mut base = header;
    for font in fonts {
        out.extend_from_slice(&(base as u32).to_be_bytes());
        base += font.len().next_multiple_of(4);
    }
    for font in fonts {
        let start = out.len();
        out.extend_from_slice(font);
        let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
        for i in 0..tables {
            let at = start + 12 + 16 * i + 8;
            let offset = u32::from_be_bytes(out[at..at + 4].try_into().unwrap());
            out[at..at + 4].copy_from_slice(&(offset + start as u32).to_be_bytes());
        }
        out.resize(out.len().next_multiple_of(4), 0);
    }
    out
}

/// cmap 을 출력 가능한 ASCII(U+0020~U+007E)만 남긴 형식 4 표로 바꾼다. 한글·가나가 없는 라틴
/// 전용 파일의 대역이다. 글리프 번호는 원본 cmap 에서 읽는다.
pub(crate) fn latin_only(mut font: Vec<u8>) -> Vec<u8> {
    let mut db = cosmic_text::fontdb::Database::new();
    let id = db.load_font_source(cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(
        font.clone(),
    )))[0];
    let parsed = cosmic_text::Font::new(&db, id, cosmic_text::fontdb::Weight::NORMAL).unwrap();
    let glyphs: Vec<u16> = (0x20u32..=0x7e)
        .map(|code| parsed.as_swash().charmap().map(code))
        .collect();
    let mut words: Vec<u16> = vec![0, 1, 3, 1, 0, 12]; // cmap 머리와 레코드 하나
    let length = 16 + 2 * 2 * 4 + 2 * glyphs.len();
    words.extend([4, length as u16, 0, 4, 4, 1, 0]); // 형식 4 머리, 구간 둘
    words.extend([0x7e, 0xffff, 0, 0x20, 0xffff, 0, 1, 4, 0]);
    words.extend(&glyphs);
    let (cmap, _) = table(&font, b"cmap");
    for (i, word) in words.iter().enumerate() {
        font[cmap + i * 2..cmap + i * 2 + 2].copy_from_slice(&word.to_be_bytes());
    }
    let record = table_record(&font, b"cmap");
    font[record + 12..record + 16].copy_from_slice(&((12 + length) as u32).to_be_bytes());
    font
}

pub(crate) struct TempFont(tempfile::TempDir);

impl TempFont {
    pub(crate) fn new(bytes: &[u8]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("font.ttf"), bytes).unwrap();
        Self(dir)
    }

    pub(crate) fn path(&self) -> String {
        self.0.path().join("font.ttf").display().to_string()
    }
}
