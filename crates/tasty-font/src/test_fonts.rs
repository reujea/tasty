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
    let mut font = VALID.to_vec();
    let record = table_record(&font, tag);
    font[record] = b'x';
    font
}

/// name 표의 글꼴 이름을 바꾼다. `D2Coding` 이 시스템 목록과 겹치지 않게 하는 데 쓴다.
pub(crate) fn renamed(mut font: Vec<u8>) -> Vec<u8> {
    let (name, len) = table(&font, b"name");
    let utf16 = |s: &str| {
        s.encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<u8>>()
    };
    let pairs = [
        (b"D2Coding".to_vec(), b"D2Codinh".to_vec()),
        (utf16("D2Coding"), utf16("D2Codinh")),
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
