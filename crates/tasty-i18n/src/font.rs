//! Font validation shared by language-pack loading, authoring and UI insertion.

use ab_glyph::Font as _;

/// Failure before a font can enter a renderer.
#[derive(Debug)]
pub enum LocaleFontError {
    Read(std::io::Error),
    Parse,
    NoLineHeight,
}

impl std::fmt::Display for LocaleFontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(f, "cannot read font file: {error}"),
            Self::Parse => write!(f, "file is not a valid font"),
            Self::NoLineHeight => write!(f, "font has no positive line height"),
        }
    }
}

impl std::error::Error for LocaleFontError {}

/// Refuse bytes that hit two egui panics. epaint panics when it cannot parse a
/// font, and asserts a positive scale, derived from this height, when it first
/// lays out text with the font. Only these two are checked: a font with glyphs
/// wider than the texture atlas passes and can still panic in epaint, depending
/// on the text size and atlas width. Uses epaint's parser without depending on
/// egui or a graphics context.
pub fn validate(bytes: &[u8]) -> Result<(), LocaleFontError> {
    validate_face(bytes, 0)
}

/// [`validate`] for one face of a font collection. egui reads the face given by
/// `FontData::index`, and faces in a collection have their own tables, so the
/// check has to read the same face. A number past the last face is a parse failure.
pub fn validate_face(bytes: &[u8], face: u32) -> Result<(), LocaleFontError> {
    let font = ab_glyph::FontRef::try_from_slice_and_index(bytes, face)
        .map_err(|_| LocaleFontError::Parse)?;
    if font.height_unscaled() > 0.0 {
        Ok(())
    } else {
        Err(LocaleFontError::NoLineHeight)
    }
}

/// Read and validate one face of the exact bytes the UI will insert. Does not
/// resolve families, reorder fallback fonts, or shape RTL text.
pub fn read_validated(path: &std::path::Path, face: u32) -> Result<Vec<u8>, LocaleFontError> {
    let bytes = std::fs::read(path).map_err(LocaleFontError::Read)?;
    validate_face(&bytes, face)?;
    Ok(bytes)
}

/// Number of the face in a collection whose family name is `family`, or `None`
/// when the bytes hold no such face or are not a collection. The number is read
/// from the file because the face order differs between font releases.
pub fn collection_face_by_family(bytes: &[u8], family: &str) -> Option<u32> {
    let count = ttf_parser::fonts_in_collection(bytes)?;
    (0..count).find(|&face| {
        ttf_parser::Face::parse(bytes, face).is_ok_and(|parsed| {
            parsed.names().into_iter().any(|name| {
                (name.name_id == ttf_parser::name_id::FAMILY
                    || name.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
                    && name.to_string().as_deref() == Some(family)
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGULAR: &[u8] = include_bytes!("../../tasty-font/assets/D2Coding-ligature-Regular.ttf");

    /// Wraps fonts in a collection. Table offsets count from the start of the
    /// file, so each font's table records move with it.
    fn collection(fonts: &[Vec<u8>]) -> Vec<u8> {
        let header = 12 + 4 * fonts.len();
        let mut out = b"ttcf\0\x01\0\0".to_vec();
        out.extend_from_slice(&(fonts.len() as u32).to_be_bytes());
        let mut at = header;
        for font in fonts {
            out.extend_from_slice(&(at as u32).to_be_bytes());
            at += font.len();
        }
        let mut at = header;
        for font in fonts {
            let mut font = font.clone();
            let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
            for record in (0..tables).map(|i| 12 + 16 * i) {
                let offset = record + 8;
                let old = u32::from_be_bytes(font[offset..offset + 4].try_into().unwrap());
                font[offset..offset + 4].copy_from_slice(&(old + at as u32).to_be_bytes());
            }
            at += font.len();
            out.extend_from_slice(&font);
        }
        out
    }

    /// Same font under another family name of the same length.
    fn renamed(from: &str, to: &str) -> Vec<u8> {
        assert_eq!(from.len(), to.len());
        let utf16 = |s: &str| s.bytes().flat_map(|b| [0, b]).collect::<Vec<u8>>();
        let mut font = REGULAR.to_vec();
        let (from, to) = (utf16(from), utf16(to));
        let mut replaced = 0usize;
        for at in 0..font.len() - from.len() {
            if font[at..at + from.len()] == from[..] {
                font[at..at + to.len()].copy_from_slice(&to);
                replaced += 1;
            }
        }
        assert!(
            replaced > 0,
            "the name is not stored as UTF-16 in this font"
        );
        font
    }

    fn without_line_height(font: &[u8]) -> Vec<u8> {
        let mut font = font.to_vec();
        let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
        let hhea = (0..tables)
            .map(|i| 12 + 16 * i)
            .find(|&record| &font[record..record + 4] == b"hhea")
            .map(|record| u32::from_be_bytes(font[record + 8..record + 12].try_into().unwrap()))
            .unwrap() as usize;
        font[hhea + 4..hhea + 6].copy_from_slice(&(-1000i16).to_be_bytes());
        font[hhea + 6..hhea + 8].copy_from_slice(&1000i16.to_be_bytes());
        font
    }

    #[test]
    fn validation_reads_the_face_it_is_given() {
        let bytes = collection(&[REGULAR.to_vec(), without_line_height(REGULAR)]);
        assert!(validate_face(&bytes, 0).is_ok());
        assert!(matches!(
            validate_face(&bytes, 1),
            Err(LocaleFontError::NoLineHeight)
        ));
        assert!(matches!(
            validate_face(&bytes, 2),
            Err(LocaleFontError::Parse)
        ));
    }

    #[test]
    fn a_collection_face_is_found_by_family_name() {
        let bytes = collection(&[REGULAR.to_vec(), renamed("D2Coding", "X2Coding")]);
        assert_eq!(
            collection_face_by_family(&bytes, "X2Coding ligature"),
            Some(1)
        );
        assert_eq!(
            collection_face_by_family(&bytes, "D2Coding ligature"),
            Some(0)
        );
        assert_eq!(collection_face_by_family(&bytes, "Other"), None);
    }

    #[test]
    fn a_single_font_file_has_no_collection_face() {
        assert_eq!(
            collection_face_by_family(REGULAR, "D2Coding ligature"),
            None
        );
    }
}
