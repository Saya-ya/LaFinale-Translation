//! Exact port of the three *diagnostic* EXE patches. Only one known NoDVD
//! version is accepted; these changes do not implement pixel width or paging.

use sha2::{Digest, Sha256};
use std::fmt;

pub const SOURCE_SHA256: &str = "a7e4999ff54e56499130a45427ab0273bc6b9d475e03c93f493cb6b592a13a79";
const SBCS_SHA256: &str = "ac6038f81ffcb7126b356928f7d758ff28ef923ec9948db4121d8b900e3dc4ea";
const SPACE_SHA256: &str = "5226c336f2d8e886eaa827e527ecd543f377a5aaa213d3421ebc74a581a1a44b";
pub const DIAGNOSTIC_SHA256: &str =
    "9f5aefe32d61a60c77a400b628224bbfc878033028ee8c1f16ffc2b4aca3c17b";
const IMAGE_BASE: u32 = 0x400000;

#[derive(Debug)]
pub enum PatchError {
    UnsupportedExe(String),
    InvalidPe(&'static str),
    Mismatch(&'static str),
}

impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedExe(hash) => write!(
                f,
                "EXE NoDVD no compatible (SHA-256: {hash}); no se modificó nada"
            ),
            Self::InvalidPe(message) => write!(f, "PE32 inválido: {message}"),
            Self::Mismatch(message) => {
                write!(f, "La firma o salida de prueba no coincide: {message}")
            }
        }
    }
}

impl std::error::Error for PatchError {}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn supported(bytes: &[u8]) -> bool {
    sha256(bytes) == SOURCE_SHA256
}

/// Returns a new diagnostic EXE in memory. The caller must write it to a new
/// path, never to the input. Intermediate hashes guard against porting errors.
pub fn build_diagnostic(original: &[u8]) -> Result<Vec<u8>, PatchError> {
    if !supported(original) {
        return Err(PatchError::UnsupportedExe(sha256(original)));
    }
    let mut bytes = original.to_vec();
    patch_sbcs(&mut bytes)?;
    verify_hash(&bytes, SBCS_SHA256, "SBCS/DBCS")?;
    patch_spaces(&mut bytes)?;
    verify_hash(&bytes, SPACE_SHA256, "espacios ASCII")?;
    patch_slot_limit(&mut bytes)?;
    verify_hash(&bytes, DIAGNOSTIC_SHA256, "límite 52 bytes")?;
    Ok(bytes)
}

fn verify_hash(bytes: &[u8], expected: &str, stage: &'static str) -> Result<(), PatchError> {
    if sha256(bytes) == expected {
        Ok(())
    } else {
        Err(PatchError::Mismatch(stage))
    }
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, PatchError> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at.checked_add(2).ok_or(PatchError::InvalidPe("offset"))?)
            .ok_or(PatchError::InvalidPe("offset fuera del archivo"))?
            .try_into()
            .unwrap(),
    ))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, PatchError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at.checked_add(4).ok_or(PatchError::InvalidPe("offset"))?)
            .ok_or(PatchError::InvalidPe("offset fuera del archivo"))?
            .try_into()
            .unwrap(),
    ))
}

fn write_u16(bytes: &mut [u8], at: usize, value: u16) -> Result<(), PatchError> {
    checked_mut(bytes, at, 2)?.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u32(bytes: &mut [u8], at: usize, value: u32) -> Result<(), PatchError> {
    checked_mut(bytes, at, 4)?.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn checked_mut(bytes: &mut [u8], at: usize, len: usize) -> Result<&mut [u8], PatchError> {
    bytes
        .get_mut(at..at.checked_add(len).ok_or(PatchError::InvalidPe("offset"))?)
        .ok_or(PatchError::InvalidPe("offset fuera del archivo"))
}

fn align(value: u32, alignment: u32) -> Result<u32, PatchError> {
    if !alignment.is_power_of_two() {
        return Err(PatchError::InvalidPe("alineación"));
    }
    value
        .checked_add(alignment - 1)
        .map(|n| n & !(alignment - 1))
        .ok_or(PatchError::InvalidPe("overflow de alineación"))
}

struct Section {
    name: [u8; 8],
    rva: u32,
    raw_size: u32,
    raw_offset: u32,
    flags: u32,
}
struct Pe {
    coff: usize,
    optional: usize,
    table: usize,
    count: u16,
    file_align: u32,
    section_align: u32,
    image_size: u32,
    headers_size: u32,
    sections: Vec<Section>,
}

impl Pe {
    fn parse(bytes: &[u8]) -> Result<Self, PatchError> {
        if bytes.get(..2) != Some(b"MZ") {
            return Err(PatchError::InvalidPe("MZ"));
        }
        let pe_offset = read_u32(bytes, 0x3c)? as usize;
        if bytes.get(
            pe_offset
                ..pe_offset
                    .checked_add(4)
                    .ok_or(PatchError::InvalidPe("PE"))?,
        ) != Some(b"PE\0\0")
        {
            return Err(PatchError::InvalidPe("firma PE"));
        }
        let coff = pe_offset + 4;
        if read_u16(bytes, coff)? != 0x14c {
            return Err(PatchError::InvalidPe("no es x86"));
        }
        let count = read_u16(bytes, coff + 2)?;
        let optional = coff + 20;
        let table = optional + read_u16(bytes, coff + 16)? as usize;
        if read_u16(bytes, optional)? != 0x10b || read_u32(bytes, optional + 28)? != IMAGE_BASE {
            return Err(PatchError::InvalidPe("PE32 o ImageBase"));
        }
        let file_align = read_u32(bytes, optional + 36)?;
        let section_align = read_u32(bytes, optional + 32)?;
        let image_size = read_u32(bytes, optional + 56)?;
        let headers_size = read_u32(bytes, optional + 60)?;
        let mut sections = Vec::new();
        for n in 0..count as usize {
            let at = table + n * 40;
            let name: [u8; 8] = bytes
                .get(at..at + 8)
                .ok_or(PatchError::InvalidPe("tabla de secciones"))?
                .try_into()
                .unwrap();
            sections.push(Section {
                name,
                rva: read_u32(bytes, at + 12)?,
                raw_size: read_u32(bytes, at + 16)?,
                raw_offset: read_u32(bytes, at + 20)?,
                flags: read_u32(bytes, at + 36)?,
            });
        }
        Ok(Self {
            coff,
            optional,
            table,
            count,
            file_align,
            section_align,
            image_size,
            headers_size,
            sections,
        })
    }

    fn va_offset(&self, bytes: &[u8], va: u32, section: &[u8]) -> Result<usize, PatchError> {
        let rva = va
            .checked_sub(IMAGE_BASE)
            .ok_or(PatchError::InvalidPe("VA"))?;
        let part = self
            .sections
            .iter()
            .find(|s| {
                s.name.split(|&b| b == 0).next() == Some(section)
                    && rva >= s.rva
                    && rva - s.rva < s.raw_size
            })
            .ok_or(PatchError::InvalidPe("VA/sección"))?;
        let at = part
            .raw_offset
            .checked_add(rva - part.rva)
            .ok_or(PatchError::InvalidPe("offset"))? as usize;
        if at >= bytes.len() {
            return Err(PatchError::InvalidPe("sección truncada"));
        }
        Ok(at)
    }

    fn append_section(
        &self,
        bytes: &mut Vec<u8>,
        name: &[u8; 8],
        code: &[u8],
    ) -> Result<u32, PatchError> {
        let at = self.table + self.count as usize * 40;
        if at + 40 > self.headers_size as usize || bytes.get(at..at + 40) != Some(&[0; 40]) {
            return Err(PatchError::InvalidPe("sin cabecera de sección libre"));
        }
        let raw = align(
            bytes
                .len()
                .try_into()
                .map_err(|_| PatchError::InvalidPe("archivo demasiado grande"))?,
            self.file_align,
        )?;
        let rva = align(self.image_size, self.section_align)?;
        let raw_size = align(code.len() as u32, self.file_align)?;
        let new_image_size = align(rva + code.len() as u32, self.section_align)?;
        bytes.resize(raw as usize, 0);
        bytes.extend_from_slice(code);
        bytes.resize(raw as usize + raw_size as usize, 0);
        checked_mut(bytes, at, 8)?.copy_from_slice(name);
        for (offset, value) in [
            (8, code.len() as u32),
            (12, rva),
            (16, raw_size),
            (20, raw),
            (36, 0x60000020),
        ] {
            write_u32(bytes, at + offset, value)?;
        }
        write_u16(bytes, self.coff + 2, self.count + 1)?;
        let size_of_code = read_u32(bytes, self.optional + 4)?;
        write_u32(bytes, self.optional + 4, size_of_code + raw_size)?;
        write_u32(bytes, self.optional + 56, new_image_size)?;
        Ok(IMAGE_BASE + rva)
    }

    fn checksum(&self, bytes: &mut [u8]) -> Result<(), PatchError> {
        let offset = self.optional + 64;
        write_u32(bytes, offset, 0)?;
        let mut sum = 0u32;
        for pair in bytes.chunks(2) {
            let word = u32::from(pair[0]) + u32::from(pair.get(1).copied().unwrap_or(0)) * 256;
            sum += word;
            sum = (sum & 0xffff) + (sum >> 16);
        }
        sum = (sum & 0xffff) + (sum >> 16);
        write_u32(bytes, offset, sum.wrapping_add(bytes.len() as u32))
    }
}

fn expect_replace(bytes: &mut [u8], at: usize, old: &[u8], new: &[u8]) -> Result<(), PatchError> {
    if old.len() != new.len() || bytes.get(at..at + old.len()) != Some(old) {
        return Err(PatchError::Mismatch("bytes previos inesperados"));
    }
    checked_mut(bytes, at, new.len())?.copy_from_slice(new);
    Ok(())
}

fn rel32(from: u32, size: u32, target: u32) -> [u8; 4] {
    (i64::from(target) - i64::from(from) - i64::from(size) as i64).to_le_bytes()[..4]
        .try_into()
        .unwrap()
}

fn jump(from: u32, target: u32) -> [u8; 5] {
    let mut out = [0; 5];
    out[0] = 0xe9;
    out[1..].copy_from_slice(&rel32(from, 5, target));
    out
}

struct Asm {
    base: u32,
    bytes: Vec<u8>,
    fixups: Vec<(usize, &'static str)>,
    labels: Vec<(&'static str, u32)>,
}
impl Asm {
    fn new(base: u32) -> Self {
        Self {
            base,
            bytes: Vec::new(),
            fixups: Vec::new(),
            labels: Vec::new(),
        }
    }
    fn label(&mut self, name: &'static str) {
        self.labels
            .push((name, self.base + self.bytes.len() as u32));
    }
    fn raw(&mut self, data: &[u8]) {
        self.bytes.extend_from_slice(data);
    }
    fn branch(&mut self, opcode: &[u8], target: &'static str) {
        self.raw(opcode);
        let at = self.bytes.len();
        self.raw(&[0; 4]);
        self.fixups.push((at, target));
    }
    fn to(&mut self, opcode: &[u8], target: u32) {
        self.raw(opcode);
        let at = self.bytes.len();
        self.raw(&rel32(
            self.base + at as u32 - opcode.len() as u32,
            (opcode.len() + 4) as u32,
            target,
        ));
    }
    fn finish(mut self) -> Result<(Vec<u8>, Vec<(&'static str, u32)>), PatchError> {
        for (at, label) in self.fixups {
            let target = self
                .labels
                .iter()
                .find(|(n, _)| *n == label)
                .ok_or(PatchError::Mismatch("etiqueta de ensamblado"))?
                .1;
            let displacement = rel32(self.base + at as u32, 4, target);
            self.bytes[at..at + 4].copy_from_slice(&displacement);
        }
        Ok((self.bytes, self.labels))
    }
}

fn patch_sbcs(bytes: &mut Vec<u8>) -> Result<(), PatchError> {
    let pe = Pe::parse(bytes)?;
    if !pe
        .sections
        .iter()
        .any(|s| s.name.starts_with(b".text\0") && s.rva == 0x1000 && s.flags & 0x20000000 != 0)
    {
        return Err(PatchError::InvalidPe(".text no ejecutable"));
    }
    let start = pe.va_offset(bytes, 0x48a5da, b".text")?;
    let class = pe.va_offset(bytes, 0x48a5f2, b".text")?;
    let bound = pe.va_offset(bytes, 0x48a652, b".text")?;
    let old_start = [0x8b, 0x75, 0xd8, 0x03, 0xf6, 0x03, 0xf3];
    let old_class = [0x3c, 0x01, 0x74, 0x03, 0x43, 0xeb, 0x51];
    let old_bound = [0x8d, 0x53, 0x01];
    if bytes[start..start + 7] != old_start
        || bytes[class..class + 7] != old_class
        || bytes[bound..bound + 3] != old_bound
    {
        return Err(PatchError::Mismatch("firmas SBCS originales"));
    }
    let base = IMAGE_BASE + align(pe.image_size, pe.section_align)?;
    let mut cursor = Asm::new(base);
    cursor.raw(&[0x8b, 0xc3, 0x50]);
    cursor.to(&[0xe8], 0x563ab4);
    cursor.raw(&[0x83, 0xc4, 0x04, 0x8d, 0x34, 0x03]);
    cursor.to(&[0xe9], 0x48a5e1);
    let mut classifier = Asm::new(base + 0x20);
    classifier.raw(&[0x3c, 0x01]);
    classifier.branch(&[0x0f, 0x85], "not_lead");
    classifier.raw(&[
        0x8b, 0x45, 0x08, 0x8b, 0x48, 0xfc, 0x8d, 0x53, 0x01, 0x3b, 0xd1,
    ]);
    classifier.branch(&[0x0f, 0x87], "skip");
    classifier.to(&[0xe9], 0x48a5f9);
    classifier.label("not_lead");
    classifier.raw(&[0x84, 0xc0]);
    classifier.branch(&[0x0f, 0x85], "skip");
    classifier.raw(&[
        0x8b, 0x45, 0x08, 0x8a, 0x4c, 0x18, 0xff, 0x88, 0x0e, 0xc6, 0x46, 0x01, 0x00, 0x46, 0x43,
        0xff, 0x45, 0xd8,
    ]);
    classifier.to(&[0xe9], 0x48a64a);
    classifier.label("skip");
    classifier.raw(&[0x43]);
    classifier.to(&[0xe9], 0x48a64a);
    let (start_code, _) = cursor.finish()?;
    let (class_code, _) = classifier.finish()?;
    if start_code.len() > 0x20 {
        return Err(PatchError::Mismatch("stubs solapados"));
    }
    let mut section = vec![0u8; 0x20];
    section[..start_code.len()].copy_from_slice(&start_code);
    section.extend_from_slice(&class_code);
    let installed = pe.append_section(bytes, b".patch\0\0", &section)?;
    if installed != base {
        return Err(PatchError::Mismatch("VA del stub"));
    }
    let mut start_jump = jump(0x48a5da, base).to_vec();
    start_jump.extend_from_slice(&[0x90; 2]);
    let mut class_jump = jump(0x48a5f2, base + 0x20).to_vec();
    class_jump.extend_from_slice(&[0x90; 2]);
    expect_replace(bytes, start, &old_start, &start_jump)?;
    expect_replace(bytes, class, &old_class, &class_jump)?;
    expect_replace(bytes, bound, &old_bound, &[0x8b, 0xd3, 0x90])?;
    pe.checksum(bytes)
}

fn patch_spaces(bytes: &mut [u8]) -> Result<(), PatchError> {
    let pe = Pe::parse(bytes)?;
    let offset = pe.va_offset(bytes, 0x5d559a, b".data")?;
    expect_replace(bytes, offset, b" \t\n\r\0", b"\t\t\n\r\0")?;
    pe.checksum(bytes)
}

fn patch_slot_limit(bytes: &mut Vec<u8>) -> Result<(), PatchError> {
    let pe = Pe::parse(bytes)?;
    let base = IMAGE_BASE + align(pe.image_size, pe.section_align)?;
    let mut code = Asm::new(base);
    code.label("start");
    code.raw(&[0x89, 0x5d, 0xfc]);
    code.to(&[0xe9], 0x70e000);
    code.label("loop");
    code.raw(&[0x8b, 0x45, 0xd8, 0x83, 0xf8, 0x34]);
    code.to(&[0x0f, 0x8d], 0x48a66e);
    code.raw(&[0x8b, 0xce, 0x2b, 0x4d, 0xfc, 0x83, 0xf9, 0x34]);
    code.to(&[0x0f, 0x8d], 0x48a66e);
    code.to(&[0xe9], 0x48a652);
    code.label("class");
    code.raw(&[0x8b, 0xce, 0x2b, 0x4d, 0xfc, 0x83, 0xf9, 0x34]);
    code.to(&[0x0f, 0x8d], 0x48a66e);
    code.raw(&[0x83, 0xf9, 0x33]);
    code.branch(&[0x0f, 0x85], "old_class");
    code.raw(&[0x3c, 0x01]);
    code.to(&[0x0f, 0x84], 0x48a66e);
    code.label("old_class");
    code.to(&[0xe9], 0x70e020);
    let (section, labels) = code.finish()?;
    let label = |name| {
        labels
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, va)| *va)
            .ok_or(PatchError::Mismatch("etiqueta del límite"))
    };
    let sites = [
        (0x48a5da, label("start")?, {
            let mut b = jump(0x48a5da, 0x70e000).to_vec();
            b.extend_from_slice(&[0x90; 2]);
            b
        }),
        (0x48a5f2, label("class")?, {
            let mut b = jump(0x48a5f2, 0x70e020).to_vec();
            b.extend_from_slice(&[0x90; 2]);
            b
        }),
        (
            0x48a64a,
            label("loop")?,
            vec![0x8b, 0x45, 0xd8, 0x83, 0xf8, 0x1a, 0x7d, 0x1c],
        ),
    ];
    let offsets: Vec<_> = sites
        .iter()
        .map(|(va, _, old)| {
            let at = pe.va_offset(bytes, *va, b".text")?;
            if bytes.get(at..at + old.len()) != Some(old.as_slice()) {
                return Err(PatchError::Mismatch("firma del límite"));
            }
            Ok(at)
        })
        .collect::<Result<_, PatchError>>()?;
    let installed = pe.append_section(bytes, b".limtest", &section)?;
    if installed != base {
        return Err(PatchError::Mismatch("VA del límite"));
    }
    for ((site, target, old), at) in sites.iter().zip(offsets) {
        let mut new = jump(*site, *target).to_vec();
        new.resize(old.len(), 0x90);
        expect_replace(bytes, at, old, &new)?;
    }
    pe.checksum(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_and_truncated_files() {
        for input in [b"".as_slice(), b"MZ".as_slice(), b"PE\0\0".as_slice()] {
            assert!(matches!(
                build_diagnostic(input),
                Err(PatchError::UnsupportedExe(_))
            ));
        }
    }

    #[test]
    fn known_input_round_trip_when_supplied_locally() {
        let Ok(path) = std::env::var("LAFINALE_TEST_EXE") else {
            return;
        };
        let original = std::fs::read(path).unwrap();
        assert_eq!(sha256(&original), SOURCE_SHA256);
        let output = build_diagnostic(&original).unwrap();
        assert_eq!(sha256(&output), DIAGNOSTIC_SHA256);
    }
}
