//! A single, pinned `\n` -> `\p` experiment in prologue entry #0. This is
//! not general paging support and must never alter the normal PAK builder.

use crate::{
    exe_patch::sha256,
    kgo,
    pak::{crc16, Archive, PakError},
};

pub const BASE_SHA256: &str = "8234861e352d578d7ec1f3e1e1c277faf5cff20e66ef15960b01a7b989c89453";
pub const FIRST_BEFORE: &str = "[Ejecutivo]  \"... Respecto al proximo\\ncontrato...\"";
pub const FIRST_AFTER: &str = "[Ejecutivo]  \"... Respecto al proximo\\pcontrato...\"";

/// Returns a new PAK with exactly one payload byte and the trailing CRC
/// changed. The unchanged ordinary output is required as input.
pub fn build_page_probe(base: &[u8]) -> Result<Vec<u8>, PakError> {
    if sha256(base) != BASE_SHA256 {
        return Err(PakError::Unsupported(format!(
            "se requiere el PAK diagnóstico conocido ({BASE_SHA256})"
        )));
    }
    let archive = Archive::parse(base)?;
    let prologue_index = archive
        .entries
        .iter()
        .position(|e| e.name == "scr00001.kgo")
        .ok_or(PakError::Invalid("prólogo ausente"))?;
    let prologue = archive.unpack(prologue_index)?;
    let first = kgo::extract(&prologue, "scr00001.kgo")
        .map_err(|_| PakError::Invalid("SF03 del prólogo"))?;
    if first
        .strings
        .first()
        .is_none_or(|row| row.japanese != FIRST_BEFORE || row.index != 0 || row.rel_offset != 0)
    {
        return Err(PakError::Invalid("primera frase distinta de la esperada"));
    }
    let section = u32::from_le_bytes(prologue[52..56].try_into().unwrap()) as usize;
    let first_len = first.strings[0].raw_len as usize;
    let first_record = prologue
        .get(section + 2..section + first_len)
        .ok_or(PakError::Invalid("primera cadena fuera de SF03"))?;
    let hits: Vec<_> = first_record
        .windows(2)
        .enumerate()
        .filter_map(|(at, pair)| (pair == b"\\n").then_some(at))
        .collect();
    if hits.len() != 1 {
        return Err(PakError::Invalid(
            "no hay un único salto \\n en la primera frase",
        ));
    }
    let local_offset = section + 2 + hits[0] + 1;
    let absolute = archive.plain_entry_offset(prologue_index)? + local_offset;
    if base[absolute] != b'n' {
        return Err(PakError::Invalid("marcador de salto inesperado"));
    }

    let mut output = base.to_vec();
    output[absolute] = b'p';
    let end = output.len() - 2;
    let crc = crc16(&output[..end]);
    output[end..].copy_from_slice(&crc.to_le_bytes());

    let verified = Archive::parse(&output)?;
    let new_prologue = verified.unpack(prologue_index)?;
    let new_first = kgo::extract(&new_prologue, "scr00001.kgo")
        .map_err(|_| PakError::Invalid("prólogo de prueba ilegible"))?;
    if new_first.strings[0].japanese != FIRST_AFTER
        || first
            .strings
            .iter()
            .skip(1)
            .zip(new_first.strings.iter().skip(1))
            .any(|(a, b)| a.japanese != b.japanese)
    {
        return Err(PakError::Invalid(
            "cadenas no conservadas después del salto",
        ));
    }
    if output.len() != base.len()
        || base
            .iter()
            .zip(&output)
            .enumerate()
            .any(|(at, (before, after))| before != after && at != absolute && at < end)
    {
        return Err(PakError::Invalid("la prueba cambió otros datos"));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_input() {
        assert!(build_page_probe(b"not a pak").is_err());
    }

    #[test]
    fn changes_one_marker_when_known_pak_is_supplied_locally() {
        let Ok(path) = std::env::var("LAFINALE_TEST_DIAGNOSTIC_PAK") else {
            return;
        };
        let original = std::fs::read(path).unwrap();
        let modified = build_page_probe(&original).unwrap();
        assert_ne!(sha256(&modified), BASE_SHA256);
        assert_eq!(modified.len(), original.len());
        let old = Archive::parse(&original).unwrap();
        let new = Archive::parse(&modified).unwrap();
        for (i, entry) in old.entries.iter().enumerate() {
            if entry.name != "scr00001.kgo" {
                assert_eq!(
                    old.unpack(i).unwrap(),
                    new.unpack(i).unwrap(),
                    "{}",
                    entry.name
                );
            }
        }
    }
}
