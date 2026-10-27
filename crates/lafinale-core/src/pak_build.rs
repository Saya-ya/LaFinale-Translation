//! Diagnostic Script.pak builder. Mirrors the previously tested Python
//! no-compression HyPack re-pack and 48-byte ASCII-space-aware line splitting.
//! `\p`, five-row paging, menu layout and pixel width remain unverified.

use crate::{
    kgo,
    pak::{self, Archive},
};
use encoding_rs::SHIFT_JIS;
use std::{collections::HashMap, fmt};

const ROW_BYTES: usize = 48;

#[derive(Debug)]
pub enum BuildPakError {
    Invalid(String),
}
impl fmt::Display for BuildPakError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "No se generó el PAK: {message}"),
        }
    }
}
impl std::error::Error for BuildPakError {}
type Result<T> = std::result::Result<T, BuildPakError>;
fn invalid(message: impl Into<String>) -> BuildPakError {
    BuildPakError::Invalid(message.into())
}

#[derive(Debug, Clone)]
pub struct BuildReport {
    pub translated_strings: usize,
    pub wrapped_strings: usize,
    pub normalized_characters: usize,
    pub maximum_rows: usize,
}

fn normalize(text: &str) -> Result<(String, usize)> {
    if text.contains('\u{3000}')
        || text
            .chars()
            .any(|c| c == '\0' || c == '\n' || c == '\r' || c == '\t')
        || text.contains("\\n")
        || text.contains("\\p")
    {
        return Err(invalid(
            "espacio U+3000 o control/salto manual en una traducción",
        ));
    }
    let mut out = String::with_capacity(text.len());
    let mut replaced = 0;
    for c in text.chars() {
        let change = match c {
            'á' => Some("a"),
            'é' => Some("e"),
            'í' => Some("i"),
            'ó' => Some("o"),
            'ú' => Some("u"),
            'Á' => Some("A"),
            'É' => Some("E"),
            'Í' => Some("I"),
            'Ó' => Some("O"),
            'Ú' => Some("U"),
            'ñ' => Some("n"),
            'Ñ' => Some("N"),
            'ü' => Some("u"),
            'Ü' => Some("U"),
            'è' => Some("e"),
            'ō' => Some("o"),
            'Ō' => Some("O"),
            'ū' => Some("u"),
            '«' | '»' => Some("\""),
            '¿' | '¡' => Some(""),
            _ => None,
        };
        if let Some(change) = change {
            out.push_str(change);
            replaced += 1;
        } else {
            out.push(c);
        }
    }
    if SHIFT_JIS.encode(&out).2 {
        return Err(invalid(
            "carácter no representable en CP932; no se sustituirá por ?",
        ));
    }
    Ok((out, replaced))
}

fn bytes_cp932(text: &str) -> usize {
    SHIFT_JIS.encode(text).0.len()
}

/// Returns native literal `\n` markers, preserving all characters except
/// each ASCII space replaced by one marker. Never splits a CP932 byte pair.
fn wrap(text: &str) -> Result<(String, usize)> {
    let mut output = String::new();
    let mut remaining = text;
    let mut rows = 1;
    while bytes_cp932(remaining) > ROW_BYTES {
        let mut offset = 0;
        for (at, c) in remaining.char_indices() {
            let end = at + c.len_utf8();
            if bytes_cp932(&remaining[..end]) > ROW_BYTES {
                break;
            }
            offset = end;
        }
        if offset == 0 {
            return Err(invalid("carácter mayor que 48 bytes CP932"));
        }
        // Match Python rfind(' ', 1, offset+1): prefer the last ASCII space
        // *inside* the current segment; otherwise cut at a char boundary.
        let cut = if remaining[offset..].starts_with(' ') {
            Some(offset)
        } else {
            remaining[..offset].rfind(' ').filter(|&at| at > 0)
        };
        let (piece, rest) = if let Some(at) = cut {
            (&remaining[..at], &remaining[at + 1..])
        } else {
            (&remaining[..offset], &remaining[offset..])
        };
        if piece.is_empty() {
            return Err(invalid("línea vacía"));
        }
        output.push_str(piece);
        output.push_str("\\n");
        remaining = rest;
        rows += 1;
        if rows > 4 {
            return Err(invalid(
                "el diálogo necesitaría cinco filas o paginación \\p no verificada",
            ));
        }
    }
    output.push_str(remaining);
    Ok((output, rows))
}

fn rewrite(raw: &[u8], document: &kgo::Script, report: &mut BuildReport) -> Result<Vec<u8>> {
    let source =
        kgo::extract(raw, &document.filename).map_err(|error| invalid(error.to_string()))?;
    if source.filename != document.filename
        || source.script_id != document.script_id
        || source.string_count != document.string_count
        || document.strings.len() != source.strings.len()
    {
        return Err(invalid(format!(
            "índices o cabecera distintos: {}",
            document.filename
        )));
    }
    let start = u32::from_le_bytes(raw[52..56].try_into().unwrap()) as usize;
    let mut section = Vec::new();
    let mut expected_text = Vec::with_capacity(source.string_count);
    for (old, row) in source.strings.iter().zip(&document.strings) {
        if old.index != row.index
            || old.japanese != row.japanese
            || old.raw_len != row.raw_len
            || old.rel_offset != row.rel_offset
        {
            return Err(invalid(format!(
                "texto original o índice cambiado: {} #{}",
                document.filename, old.index
            )));
        }
        if row.spanish.trim().is_empty() {
            let at = start + old.rel_offset;
            section.extend_from_slice(&raw[at..at + old.raw_len as usize]);
            expected_text.push(old.japanese.clone());
            continue;
        }
        let (normal, count) = normalize(&row.spanish)
            .map_err(|error| invalid(format!("{} #{}: {error}", document.filename, row.index)))?;
        report.normalized_characters += count;
        let (translated, rows) = if document.filename == "scr00000.kgo" {
            (normal, 1) // menu has a different rendering path
        } else {
            wrap(&normal)?
        };
        report.maximum_rows = report.maximum_rows.max(rows);
        report.wrapped_strings += usize::from(rows > 1);
        report.translated_strings += 1;
        expected_text.push(translated.clone());
        let (encoded, _, errors) = SHIFT_JIS.encode(&translated);
        if errors {
            return Err(invalid("CP932 no representable"));
        }
        let len = encoded.len() + 3;
        let len = len + len % 2;
        if len > u16::MAX as usize {
            return Err(invalid("registro SF03 demasiado grande"));
        }
        section.extend_from_slice(&(len as u16).to_le_bytes());
        section.extend_from_slice(&encoded);
        section.resize(section.len() + (len - 2 - encoded.len()), 0);
    }
    let mut rebuilt = raw[..start].to_vec();
    let size = section.len() as u32;
    let total = start + section.len();
    if total > u32::MAX as usize {
        return Err(invalid("script demasiado grande"));
    }
    rebuilt[4..8].copy_from_slice(&(total as u32).to_le_bytes());
    rebuilt[56..60].copy_from_slice(&size.to_le_bytes());
    rebuilt.extend_from_slice(&section);
    // Both old and rebuilt files must be valid, with matching numbers of rows.
    let check =
        kgo::extract(&rebuilt, &document.filename).map_err(|error| invalid(error.to_string()))?;
    if check.string_count != source.string_count {
        return Err(invalid("se perdieron índices SF03"));
    }
    for (row, expected) in check.strings.iter().zip(expected_text) {
        if row.japanese != expected {
            return Err(invalid(format!(
                "texto perdido al reextraer {} #{}",
                document.filename, row.index
            )));
        }
    }
    Ok(rebuilt)
}

pub fn build(
    original: &[u8],
    docs: &HashMap<String, kgo::Script>,
) -> Result<(Vec<u8>, BuildReport)> {
    if crate::exe_patch::sha256(original) != pak::ORIGINAL_SHA256 {
        return Err(invalid("hash del PAK original no reconocido"));
    }
    let archive = Archive::parse(original).map_err(|error| invalid(error.to_string()))?;
    if docs.len() != 117 {
        return Err(invalid(
            "se esperan los 117 JSON originales, sin archivos adicionales",
        ));
    }
    let mut report = BuildReport {
        translated_strings: 0,
        wrapped_strings: 0,
        normalized_characters: 0,
        maximum_rows: 1,
    };
    let mut contents = Vec::with_capacity(118);
    for (index, entry) in archive.entries.iter().enumerate() {
        let raw = archive
            .unpack(index)
            .map_err(|error| invalid(error.to_string()))?;
        if index == 0 {
            contents.push(raw);
            continue;
        }
        let doc = docs
            .get(&entry.name)
            .ok_or_else(|| invalid(format!("falta JSON para {}", entry.name)))?;
        if doc.filename != entry.name {
            return Err(invalid("nombre de JSON distinto al script"));
        }
        if doc.strings.iter().all(|row| row.spanish.trim().is_empty()) {
            // Still validate untouched JSON against the original KGO.
            let source =
                kgo::extract(&raw, &entry.name).map_err(|error| invalid(error.to_string()))?;
            if doc.filename != source.filename
                || doc.script_id != source.script_id
                || doc.strings.len() != source.strings.len()
                || doc.strings.iter().zip(&source.strings).any(|(a, b)| {
                    a.index != b.index
                        || a.japanese != b.japanese
                        || a.raw_len != b.raw_len
                        || a.rel_offset != b.rel_offset
                })
            {
                return Err(invalid(format!("JSON original distinto: {}", entry.name)));
            }
            contents.push(raw);
        } else {
            contents.push(rewrite(&raw, doc, &mut report)?);
        }
    }
    if report.translated_strings == 0 {
        return Err(invalid("ninguna cadena española para aplicar"));
    }
    let output = pack(&archive, &contents)?;
    let verified = Archive::parse(&output).map_err(|error| invalid(error.to_string()))?;
    for (index, expected) in contents.iter().enumerate() {
        if verified.entries[index].name != archive.entries[index].name
            || verified
                .unpack(index)
                .map_err(|error| invalid(error.to_string()))?
                != *expected
        {
            return Err(invalid(format!(
                "falló la reextracción de {}",
                archive.entries[index].name
            )));
        }
    }
    Ok((output, report))
}

fn pack(archive: &Archive<'_>, contents: &[Vec<u8>]) -> Result<Vec<u8>> {
    let mut out = b"HyPack\0\x03".to_vec();
    out.extend_from_slice(&[0; 8]); // TOC offset, count
    out[12..16].copy_from_slice(&(contents.len() as u32).to_le_bytes());
    let mut toc = Vec::with_capacity(contents.len() * 48);
    for (entry, data) in archive.entries.iter().zip(contents) {
        if data.len() > 32 * 1024 * 1024 {
            return Err(invalid("entrada demasiado grande"));
        }
        let data_offset = out
            .len()
            .checked_add(15)
            .ok_or_else(|| invalid("PAK demasiado grande"))?
            & !15;
        out.resize(data_offset, 0);
        let relative = (out.len() - 16) as u32;
        out.extend_from_slice(data);
        let (base, ext) = entry
            .name
            .rsplit_once('.')
            .ok_or_else(|| invalid("nombre de entrada"))?;
        if base.len() > 20 || ext.len() > 3 {
            return Err(invalid("nombre de entrada largo"));
        }
        let mut record = [0u8; 48];
        record[..base.len()].copy_from_slice(base.as_bytes());
        record[21..21 + ext.len()].copy_from_slice(ext.as_bytes());
        record[24..28].copy_from_slice(&relative.to_le_bytes());
        record[28..32].copy_from_slice(&(data.len() as u32).to_le_bytes());
        record[32..36].copy_from_slice(&(data.len() as u32).to_le_bytes());
        record[40..48].copy_from_slice(&0x01CA4E1F00000000u64.to_le_bytes());
        toc.extend_from_slice(&record);
    }
    let toc_field = out.len() as u32;
    out[8..12].copy_from_slice(&toc_field.to_le_bytes());
    out.extend_from_slice(&archive.preamble());
    out.extend_from_slice(&toc);
    let crc = pak::crc16(&out);
    out.extend_from_slice(&crc.to_le_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    #[test]
    fn preserves_ascii_spaces_and_splits_at_48_bytes() {
        let (line, rows) = wrap("uno ".repeat(15).trim_end()).unwrap();
        assert!(line.contains("\\n"));
        assert!(rows > 1);
        assert!(line.split("\\n").all(|piece| bytes_cp932(piece) <= 48));
        assert_eq!(
            normalize("¿Dónde está Ñandú?").unwrap().0,
            "Donde esta Nandu?"
        );
        assert!(normalize("A　B").is_err());
        assert!(normalize("🙂").is_err());
    }

    #[test]
    fn rejects_five_rows_until_paging_is_verified() {
        assert!(wrap(&"a".repeat(200)).is_err());
    }

    #[test]
    fn rebuilds_all_translations_when_sources_supplied_locally() {
        let (Ok(pak_path), Ok(json_dir)) = (
            std::env::var("LAFINALE_TEST_PAK"),
            std::env::var("LAFINALE_TEST_TRANSLATIONS"),
        ) else {
            return;
        };
        let original = fs::read(pak_path).unwrap();
        let mut docs = HashMap::new();
        for entry in fs::read_dir(Path::new(&json_dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let doc: kgo::Script = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
                docs.insert(doc.filename.clone(), doc);
            }
        }
        let (output, report) = build(&original, &docs).unwrap();
        assert_eq!(report.translated_strings, 9673);
        assert_eq!(report.wrapped_strings, 3577);
        assert_eq!(report.maximum_rows, 3);
        // Unlike the old prototype this version does not silently substitute
        // guillemets or macron vowels with '?'; hence a different output hash.
        assert_eq!(
            crate::exe_patch::sha256(&output),
            "8234861e352d578d7ec1f3e1e1c277faf5cff20e66ef15960b01a7b989c89453"
        );
    }
}
