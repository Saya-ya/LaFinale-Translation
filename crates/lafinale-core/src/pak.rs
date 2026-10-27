//! HyPack v3 reader for the known Script.pak. No extracted game data is stored
//! in this repository. Cocotte decoding is ported from the local Python tests.

use std::{collections::HashSet, fmt};

pub const ORIGINAL_SHA256: &str =
    "b5666dc0155b7534b9455100b39b6e8c40e28051f47aa208517a2b645e1bb40e";
const HEADER: usize = 16;
const ENTRY_SIZE: usize = 48;
const MAX_ENTRY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug)]
pub enum PakError {
    Invalid(&'static str),
    Unsupported(String),
}

impl fmt::Display for PakError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(what) => write!(f, "PAK no válido: {what}"),
            Self::Unsupported(what) => write!(f, "PAK no compatible: {what}"),
        }
    }
}
impl std::error::Error for PakError {}

type Result<T> = std::result::Result<T, PakError>;

fn u16le(data: &[u8], at: usize) -> Result<u16> {
    let bytes: [u8; 2] = data
        .get(at..at.checked_add(2).ok_or(PakError::Invalid("offset"))?)
        .ok_or(PakError::Invalid("campo truncado"))?
        .try_into()
        .unwrap();
    Ok(u16::from_le_bytes(bytes))
}
fn u32le(data: &[u8], at: usize) -> Result<u32> {
    let bytes: [u8; 4] = data
        .get(at..at.checked_add(4).ok_or(PakError::Invalid("offset"))?)
        .ok_or(PakError::Invalid("campo truncado"))?
        .try_into()
        .unwrap();
    Ok(u32::from_le_bytes(bytes))
}

pub(crate) fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for &byte in data {
        crc ^= byte as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x8408
            } else {
                crc >> 1
            };
        }
    }
    crc
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    offset: usize,
    packed_size: usize,
    unpacked_size: usize,
    method: u8,
    has_checksum: bool,
    checksum: u16,
}

pub struct Archive<'a> {
    bytes: &'a [u8],
    pub entries: Vec<Entry>,
}

impl<'a> Archive<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        if bytes.get(..8) != Some(b"HyPack\0\x03") {
            return Err(PakError::Invalid("firma o versión HyPack v3"));
        }
        if bytes.len() < HEADER + 2 {
            return Err(PakError::Invalid("archivo truncado"));
        }
        let tail = bytes.len() - 2;
        if u16le(bytes, tail)? != crc16(&bytes[..tail]) {
            return Err(PakError::Invalid("CRC16 del archivo"));
        }
        let count = u32le(bytes, 12)? as usize;
        if count != 118 {
            return Err(PakError::Invalid("se esperaban 118 entradas"));
        }
        let toc = HEADER
            .checked_add(u32le(bytes, 8)? as usize)
            .ok_or(PakError::Invalid("TOC overflow"))?;
        if toc < HEADER + 16 {
            return Err(PakError::Invalid("TOC sin preámbulo"));
        }
        let end = toc
            .checked_add(count * ENTRY_SIZE)
            .ok_or(PakError::Invalid("TOC overflow"))?;
        if end != tail {
            return Err(PakError::Invalid("tamaño de tabla de entradas"));
        }
        let mut entries = Vec::with_capacity(count);
        let mut names = HashSet::new();
        for index in 0..count {
            let at = toc + index * ENTRY_SIZE;
            let name = safe_name(&bytes[at..at + 21], &bytes[at + 21..at + 24])?;
            if !names.insert(name.clone()) {
                return Err(PakError::Invalid("nombres duplicados"));
            }
            let offset = HEADER
                .checked_add(u32le(bytes, at + 24)? as usize)
                .ok_or(PakError::Invalid("offset de datos"))?;
            let unpacked_size = u32le(bytes, at + 28)? as usize;
            let packed_size = u32le(bytes, at + 32)? as usize;
            let method = bytes[at + 36];
            if packed_size > MAX_ENTRY_BYTES
                || unpacked_size > MAX_ENTRY_BYTES
                || offset < HEADER
                || offset
                    .checked_add(packed_size)
                    .is_none_or(|finish| finish > toc)
            {
                return Err(PakError::Invalid("datos de entrada fuera del PAK"));
            }
            if !matches!(method, 0 | 2) {
                return Err(PakError::Invalid("método de compresión no soportado"));
            }
            if bytes[at + 37] > 1 {
                return Err(PakError::Invalid("indicador de checksum"));
            }
            entries.push(Entry {
                name,
                offset,
                packed_size,
                unpacked_size,
                method,
                has_checksum: bytes[at + 37] == 1,
                checksum: u16le(bytes, at + 38)?,
            });
        }
        if entries[0].name != "Scene.tbl"
            || entries[1].name != "scr00000.kgo"
            || entries.iter().skip(1).any(|e| !e.name.ends_with(".kgo"))
        {
            return Err(PakError::Invalid("lista de archivos de script"));
        }
        Ok(Self { bytes, entries })
    }

    pub fn unpack(&self, index: usize) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .get(index)
            .ok_or(PakError::Invalid("índice de entrada"))?;
        let packed = &self.bytes[entry.offset..entry.offset + entry.packed_size];
        if entry.has_checksum && crc16(packed) != entry.checksum {
            return Err(PakError::Invalid("CRC16 de entrada"));
        }
        let raw = match entry.method {
            0 => packed.to_vec(),
            2 => cocotte_decode(packed, entry.unpacked_size)?,
            _ => return Err(PakError::Invalid("método")),
        };
        if raw.len() != entry.unpacked_size {
            return Err(PakError::Invalid("tamaño descomprimido"));
        }
        Ok(raw)
    }

    pub(crate) fn plain_entry_offset(&self, index: usize) -> Result<usize> {
        let entry = self
            .entries
            .get(index)
            .ok_or(PakError::Invalid("índice de entrada"))?;
        if entry.method != 0 || entry.packed_size != entry.unpacked_size {
            return Err(PakError::Invalid(
                "la prueba requiere un PAK sin compresión",
            ));
        }
        Ok(entry.offset)
    }

    /// 16 bytes immediately before the entry table, preserved by the proven
    /// Python repacker (the original's last data block overlaps this region).
    pub(crate) fn preamble(&self) -> [u8; 16] {
        let toc = HEADER + u32le(self.bytes, 8).expect("archive already parsed") as usize;
        self.bytes[toc - 16..toc].try_into().unwrap()
    }
}

fn safe_name(base: &[u8], extension: &[u8]) -> Result<String> {
    let base = base.split(|&b| b == 0).next().unwrap_or_default();
    let ext = extension.split(|&b| b == 0).next().unwrap_or_default();
    if base.is_empty()
        || !base.iter().all(u8::is_ascii_alphanumeric)
        || !ext.iter().all(u8::is_ascii_alphanumeric)
        || ext.is_empty()
    {
        return Err(PakError::Invalid("nombre inseguro"));
    }
    Ok(format!(
        "{}.{}",
        String::from_utf8_lossy(base),
        String::from_utf8_lossy(ext)
    ))
}

const INITIAL_FREQ: [u16; 64] = [
    1400, 640, 320, 240, 160, 120, 80, 64, 48, 40, 32, 24, 20, 20, 20, 20, 16, 16, 16, 16, 12, 12,
    12, 12, 12, 12, 8, 8, 8, 8, 8, 8, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
];

struct Model {
    cf: [u32; 258],
    freq: [u32; 257],
    search: [usize; 129],
    rescale: usize,
    next_left: usize,
    incr: u32,
    left: usize,
}
impl Model {
    fn new() -> Result<Self> {
        let mut freq = [2u32; 257];
        for (at, &n) in INITIAL_FREQ.iter().enumerate() {
            freq[at] = n as u32;
        }
        freq[64..96].fill(4);
        freq[96..134].fill(3);
        let mut model = Self {
            cf: [0; 258],
            freq,
            search: [0; 129],
            rescale: (257 >> 4) | 2,
            next_left: 0,
            incr: 0,
            left: 0,
        };
        model.cf[257] = 4096;
        model.rescale()?;
        Ok(model)
    }
    fn rescale(&mut self) -> Result<()> {
        if self.next_left > 0 {
            self.incr += 1;
            self.left = self.next_left;
            self.next_left = 0;
            return Ok(());
        }
        self.rescale = (self.rescale * 2).min(2000);
        let mut cf = self.cf[257];
        let mut missing = cf;
        for i in (1..257).rev() {
            let value = self.freq[i];
            cf = cf
                .checked_sub(value)
                .ok_or(PakError::Invalid("frecuencias Cocotte"))?;
            self.cf[i] = cf;
            self.freq[i] = (value >> 1) | 1;
            missing -= self.freq[i];
        }
        if cf != self.freq[0] {
            return Err(PakError::Invalid("modelo Cocotte"));
        }
        self.freq[0] = (self.freq[0] >> 1) | 1;
        missing -= self.freq[0];
        self.incr = missing / self.rescale as u32;
        self.next_left = missing as usize % self.rescale;
        self.left = self.rescale - self.next_left;
        let mut i = 257;
        self.search[128] = 256;
        while i > 0 {
            let end = (self.cf[i] - 1) >> 5;
            i -= 1;
            let start = self.cf[i] >> 5;
            for idx in start..=end {
                self.search[idx as usize] = i;
            }
        }
        Ok(())
    }
    fn symbol(&self, value: u32) -> Result<usize> {
        let idx = (value >> 5) as usize;
        if idx >= 128 {
            return Err(PakError::Invalid("símbolo Cocotte"));
        }
        let mut lo = self.search[idx];
        let mut hi = self.search[idx + 1] + 1;
        while lo + 1 < hi {
            let mid = (lo + hi) >> 1;
            if value < self.cf[mid] {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        Ok(lo)
    }
    fn update(&mut self, symbol: usize) -> Result<()> {
        if self.left == 0 {
            self.rescale()?;
        }
        self.left -= 1;
        self.freq[symbol] += self.incr;
        Ok(())
    }
}

fn range_decode(src: &[u8], destination_size: usize, model: &mut Model) -> Result<Vec<u8>> {
    if src.len() < 2 {
        return Err(PakError::Invalid("bloque de rango truncado"));
    }
    let mut pos = 2;
    let mut buffer = src[1] as u32;
    let mut low = buffer >> 1;
    let mut range = 128u32;
    let mut output = Vec::with_capacity(destination_size);
    while pos < src.len() && output.len() < destination_size {
        while range <= 0x800000 {
            low = (low << 8) | ((buffer << 7) & 0xff);
            buffer = src.get(pos).copied().unwrap_or(0) as u32;
            pos += usize::from(pos < src.len());
            low |= buffer >> 1;
            range <<= 8;
        }
        let help = range >> 12;
        if help == 0 {
            return Err(PakError::Invalid("rango Cocotte"));
        }
        let cumulative = (low / help).min(4095);
        let symbol = model.symbol(cumulative)?;
        if symbol == 256 {
            break;
        }
        output.push(symbol as u8);
        let lt = model.cf[symbol];
        let freq = model.cf[symbol + 1] - lt;
        low = low.wrapping_sub(help * lt);
        range = if lt + freq < 4096 {
            help * freq
        } else {
            range - help * lt
        };
        model.update(symbol)?;
    }
    if output.len() != destination_size {
        return Err(PakError::Invalid("longitud de bloque de rango"));
    }
    Ok(output)
}

fn cocotte_decode(packed: &[u8], expected: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(expected);
    let mut pos = 0;
    let mut model = Model::new()?;
    let mut mtf: Vec<u8> = (0..=255).collect();
    while pos < packed.len() {
        let size = u16le(packed, pos)? as usize;
        let decoded_size = u16le(packed, pos + 2)? as usize;
        if size < 4
            || decoded_size == 0
            || decoded_size > 8192
            || decoded_size > expected - out.len()
        {
            return Err(PakError::Invalid("tamaño de bloque Cocotte"));
        }
        let block = packed
            .get(pos + 4..pos + size)
            .ok_or(PakError::Invalid("bloque truncado"))?;
        let mut buffer = if block.len() == decoded_size + 2 {
            model = Model::new()?;
            block.to_vec()
        } else {
            range_decode(block, decoded_size + 2, &mut model)?
        };
        for byte in &mut buffer {
            let index = *byte as usize;
            let symbol = mtf[index];
            mtf.copy_within(..index, 1);
            mtf[0] = symbol;
            *byte = symbol;
        }
        if buffer.len() != decoded_size + 2 {
            return Err(PakError::Invalid("MTF truncado"));
        }
        let top = u16le(&buffer, 0)? as usize;
        let data = &buffer[2..];
        if top >= data.len() {
            return Err(PakError::Invalid("índice BWT"));
        }
        let mut count = [0usize; 256];
        for &byte in data {
            count[byte as usize] += 1;
        }
        for i in 1..256 {
            count[i] += count[i - 1];
        }
        let mut sorted = vec![0usize; data.len()];
        for i in (0..data.len()).rev() {
            let byte = data[i] as usize;
            count[byte] -= 1;
            sorted[count[byte]] = i;
        }
        let mut cursor = sorted[top];
        for _ in 0..data.len() {
            out.push(data[cursor]);
            cursor = sorted[cursor];
        }
        pos += size;
    }
    if out.len() != expected {
        return Err(PakError::Invalid("longitud descomprimida Cocotte"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exe_patch::sha256;

    #[test]
    fn invalid_or_truncated_archive_is_rejected() {
        assert!(Archive::parse(b"").is_err());
        assert!(Archive::parse(b"HyPack\0\x03").is_err());
    }

    #[test]
    fn known_archive_when_supplied_locally() {
        let Ok(path) = std::env::var("LAFINALE_TEST_PAK") else {
            return;
        };
        let data = std::fs::read(path).unwrap();
        assert_eq!(sha256(&data), ORIGINAL_SHA256);
        let pak = Archive::parse(&data).unwrap();
        assert_eq!(pak.entries.len(), 118);
        for (index, hash) in [
            (
                0,
                "e2242b614aeee105ba88bb287ca9a49a43638f8cdb3571335b3b721009ff0162",
            ),
            (
                1,
                "639afd236e2d8d25e78a40ab3db18d1b27e22aeadb55301da5e9bbb8afe5b4d2",
            ),
            (
                2,
                "9c84dee215729aae0eff6ea2787709e997a42f25129b251cfb81780b7f70e99f",
            ),
        ] {
            assert_eq!(sha256(&pak.unpack(index).unwrap()), hash);
        }
        for index in 3..118 {
            pak.unpack(index).unwrap();
        }
    }
}
