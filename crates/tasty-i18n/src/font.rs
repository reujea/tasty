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
    let font = ab_glyph::FontRef::try_from_slice(bytes).map_err(|_| LocaleFontError::Parse)?;
    if font.height_unscaled() > 0.0 {
        Ok(())
    } else {
        Err(LocaleFontError::NoLineHeight)
    }
}

/// Read and validate the exact bytes the UI will insert. Does not resolve
/// families, reorder fallback fonts, or shape RTL text.
pub fn read_validated(path: &std::path::Path) -> Result<Vec<u8>, LocaleFontError> {
    let bytes = std::fs::read(path).map_err(LocaleFontError::Read)?;
    validate(&bytes)?;
    Ok(bytes)
}
