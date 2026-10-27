//! Lossless-index SF03 text listing. Only section 3 is decoded; scene bytecode
//! stays in the original PAK for a future rebuild.

use encoding_rs::SHIFT_JIS;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug)]
pub enum KgoError {
    Invalid(&'static str),
}
impl fmt::Display for KgoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "SF03 inválido: {message}"),
        }
    }
}
impl std::error::Error for KgoError {}
type Result<T> = std::result::Result<T, KgoError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEntry {
    pub index: usize,
    pub rel_offset: usize,
    pub raw_len: u16,
    pub japanese: String,
    pub spanish: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Script {
    pub filename: String,
    pub script_id: u32,
    pub string_count: usize,
    pub strings: Vec<TextEntry>,
}

fn number(bytes: &[u8], offset: usize) -> Result<u32> {
    let raw: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(KgoError::Invalid("cabecera truncada"))?
        .try_into()
        .unwrap();
    Ok(u32::from_le_bytes(raw))
}

pub fn extract(bytes: &[u8], filename: &str) -> Result<Script> {
    if bytes.len() < 64 || bytes.get(..4) != Some(b"SF03") || !filename.ends_with(".kgo") {
        return Err(KgoError::Invalid("firma o nombre"));
    }
    if number(bytes, 4)? as usize != bytes.len() {
        return Err(KgoError::Invalid("tamaño del archivo"));
    }
    let start = number(bytes, 52)? as usize;
    let size = number(bytes, 56)? as usize;
    let count = number(bytes, 60)? as usize;
    let end = start
        .checked_add(size)
        .ok_or(KgoError::Invalid("sección de texto"))?;
    if start < 64 || end > bytes.len() || count > size / 4 || bytes[end..].iter().any(|&b| b != 0) {
        return Err(KgoError::Invalid("sección de texto o relleno"));
    }
    let mut pos = start;
    let mut strings = Vec::with_capacity(count);
    for index in 0..count {
        let head = bytes
            .get(pos..pos + 2)
            .ok_or(KgoError::Invalid("registro truncado"))?;
        let len = u16::from_le_bytes(head.try_into().unwrap());
        let next = pos
            .checked_add(len as usize)
            .ok_or(KgoError::Invalid("registro"))?;
        if len < 4 || next > end {
            return Err(KgoError::Invalid("longitud de registro"));
        }
        let payload = &bytes[pos + 2..next];
        let text_bytes = payload
            .strip_suffix(&[0, 0])
            .or_else(|| payload.strip_suffix(&[0]))
            .ok_or(KgoError::Invalid("registro sin terminador"))?;
        if text_bytes.contains(&0) {
            return Err(KgoError::Invalid("NUL dentro de un texto"));
        }
        let (decoded, _, errors) = SHIFT_JIS.decode(text_bytes);
        if errors {
            return Err(KgoError::Invalid("texto no válido en CP932"));
        }
        strings.push(TextEntry {
            index,
            rel_offset: pos - start,
            raw_len: len,
            japanese: decoded.into_owned(),
            spanish: String::new(),
        });
        pos = next;
    }
    if pos != end {
        return Err(KgoError::Invalid("recuento o tamaño de cadenas"));
    }
    Ok(Script {
        filename: filename.into(),
        script_id: number(bytes, 8)?,
        string_count: count,
        strings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_minimal_script_and_checks_bounds() {
        let mut bytes = vec![0; 64];
        bytes[..4].copy_from_slice(b"SF03");
        bytes[52..56].copy_from_slice(&64u32.to_le_bytes());
        bytes[56..60].copy_from_slice(&6u32.to_le_bytes());
        bytes[60..64].copy_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[6, 0, b'A', b' ', 0, 0]);
        let file_len = bytes.len() as u32;
        bytes[4..8].copy_from_slice(&file_len.to_le_bytes());
        let script = extract(&bytes, "scr00001.kgo").unwrap();
        assert_eq!(script.strings[0].japanese, "A ");
        assert_eq!(script.strings[0].raw_len, 6);
        bytes[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert!(extract(&bytes, "scr00001.kgo").is_err());
    }

    #[test]
    fn rejects_missing_nul_and_non_cp932() {
        let mut bytes = vec![0; 64];
        bytes[..4].copy_from_slice(b"SF03");
        bytes[52..56].copy_from_slice(&64u32.to_le_bytes());
        bytes[56..60].copy_from_slice(&6u32.to_le_bytes());
        bytes[60..64].copy_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[6, 0, b'A', b'B', b'C', b'D']);
        let file_len = bytes.len() as u32;
        bytes[4..8].copy_from_slice(&file_len.to_le_bytes());
        assert!(extract(&bytes, "scr00001.kgo").is_err());
    }

    #[test]
    fn all_original_sf03_scripts_when_supplied_locally() {
        let Ok(path) = std::env::var("LAFINALE_TEST_PAK") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let archive = crate::pak::Archive::parse(&bytes).unwrap();
        let mut total = 0;
        let mut decoded_text = Vec::new();
        for (index, entry) in archive.entries.iter().enumerate().skip(1) {
            let raw = archive.unpack(index).unwrap();
            let script = extract(&raw, &entry.name).unwrap();
            assert_eq!(script.strings.len(), script.string_count);
            if index == 2 {
                assert_eq!(
                    script.strings[0].japanese,
                    "【大企業重役】　「……今度の契約が……」"
                );
            }
            for string in &script.strings {
                decoded_text.extend_from_slice(string.japanese.as_bytes());
                decoded_text.push(0);
            }
            total += script.string_count;
        }
        assert_eq!(total, 77258);
        assert_eq!(
            crate::exe_patch::sha256(&decoded_text),
            "d9e7873e8ef9289e91ffee22a8c178c09f65381669ed935e338b6c4d11ba2fa5"
        );
    }
}
