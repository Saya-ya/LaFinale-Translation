//! Rules observed in the SF03/CP932 dialogue path. Not a pixel-width estimator.

use encoding_rs::SHIFT_JIS;

pub mod exe_patch;
pub mod kgo;
pub mod pak;
pub mod pak_build;
pub mod workspace;

/// One engine slot contains at most 52 CP932 bytes plus its NUL terminator.
pub const SLOT_PAYLOAD_BYTES: usize = 52;
/// Conservative *test* threshold, not a verified number of pixels.
pub const PREVIEW_LIMIT_BYTES: usize = 48;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    FullWidthSpace,
    UnrepresentableInCp932,
    EmbeddedNul,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FullWidthSpace => write!(f, "No se admite el espacio japonés U+3000; usa U+0020."),
            Self::UnrepresentableInCp932 => write!(f, "El texto contiene caracteres no representables en CP932; no se sustituirán en silencio."),
            Self::EmbeddedNul => write!(f, "La cadena contiene un NUL que cortaría el texto del juego."),
        }
    }
}

impl std::error::Error for ValidationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capacity {
    pub encoded_bytes: usize,
    pub preview_limit_bytes: usize,
    pub slot_payload_bytes: usize,
}

impl Capacity {
    pub fn fits_preview(&self) -> bool {
        self.encoded_bytes <= self.preview_limit_bytes
    }

    pub fn fits_slot(&self) -> bool {
        self.encoded_bytes <= self.slot_payload_bytes
    }
}

/// Validates one literal string. `\\n`/`\\p` control sequences will be parsed
/// separately once the page/line behavior has been verified in-game.
pub fn check_capacity(text: &str) -> Result<Capacity, ValidationError> {
    if text.contains('\u{3000}') {
        return Err(ValidationError::FullWidthSpace);
    }
    if text.contains('\0') {
        return Err(ValidationError::EmbeddedNul);
    }
    let (bytes, _, had_errors) = SHIFT_JIS.encode(text);
    if had_errors {
        return Err(ValidationError::UnrepresentableInCp932);
    }
    Ok(Capacity {
        encoded_bytes: bytes.len(),
        preview_limit_bytes: PREVIEW_LIMIT_BYTES,
        slot_payload_bytes: SLOT_PAYLOAD_BYTES,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_space_is_a_real_byte() {
        let result = check_capacity("Uno dos tres").unwrap();
        assert_eq!(result.encoded_bytes, 12);
        assert!(result.fits_preview());
    }

    #[test]
    fn mixed_cp932_is_counted_in_bytes() {
        assert_eq!(check_capacity("a 日本").unwrap().encoded_bytes, 6);
    }

    #[test]
    fn hard_slot_boundary() {
        assert!(check_capacity(&"a".repeat(52)).unwrap().fits_slot());
        assert!(!check_capacity(&"a".repeat(53)).unwrap().fits_slot());
        assert!(!check_capacity(&"a".repeat(49)).unwrap().fits_preview());
    }

    #[test]
    fn unsafe_text_is_rejected() {
        assert_eq!(check_capacity("a　b"), Err(ValidationError::FullWidthSpace));
        assert_eq!(check_capacity("a\0b"), Err(ValidationError::EmbeddedNul));
        assert_eq!(
            check_capacity("🙂"),
            Err(ValidationError::UnrepresentableInCp932)
        );
    }
}
