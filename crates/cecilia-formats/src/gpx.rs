//! Guitar Pro 6 (`.gpx`) container: a BCFZ-compressed BCFS sector filesystem holding
//! `score.gpif` and a few auxiliary files.

use crate::{Error, Result};

const SECTOR: usize = 0x1000;

fn err(message: impl Into<String>) -> Error {
    Error::Parse {
        offset: 0,
        location: Some("gpx container".into()),
        message: message.into(),
    }
}

/// Extracts the files of a GPX container as (name, contents).
pub fn read_container(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    match bytes.get(..4) {
        Some(b"BCFZ") => {
            let fs = decompress(&bytes[4..])?;
            match fs.get(..4) {
                Some(b"BCFS") => read_filesystem(&fs[4..]),
                _ => Err(err("decompressed data is not a BCFS filesystem")),
            }
        }
        Some(b"BCFS") => read_filesystem(&bytes[4..]),
        _ => Err(err("not a GPX file")),
    }
}

/// The decompressed BCFS filesystem image (after the "BCFS" magic), for inspection.
pub fn filesystem_image(bytes: &[u8]) -> Result<Vec<u8>> {
    match bytes.get(..4) {
        Some(b"BCFZ") => Ok(decompress(&bytes[4..])?
            .get(4..)
            .unwrap_or_default()
            .to_vec()),
        Some(b"BCFS") => Ok(bytes[4..].to_vec()),
        _ => Err(err("not a GPX file")),
    }
}

/// Reads bits most-significant first.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn bit(&mut self) -> Result<u32> {
        let byte = *self
            .data
            .get(self.pos / 8)
            .ok_or_else(|| err("truncated BCFZ stream"))?;
        let bit = (byte >> (7 - (self.pos % 8))) & 1;
        self.pos += 1;
        Ok(bit as u32)
    }

    fn bits(&mut self, n: u32) -> Result<u32> {
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Ok(v)
    }

    /// Reads `n` bits least-significant first.
    fn bits_reversed(&mut self, n: u32) -> Result<u32> {
        let mut v = 0;
        for i in 0..n {
            v |= self.bit()? << i;
        }
        Ok(v)
    }
}

fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    let len = i32::from_le_bytes(
        data.get(..4)
            .ok_or_else(|| err("truncated"))?
            .try_into()
            .unwrap(),
    );
    if !(0..=256 << 20).contains(&len) {
        return Err(err(format!("implausible decompressed size {len}")));
    }
    let len = len as usize;
    let mut out = Vec::with_capacity(len);
    let mut bits = Bits {
        data: &data[4..],
        pos: 0,
    };
    // Some writers end the bit stream before the declared length; stop at end of data.
    while out.len() < len && bits.pos < bits.data.len() * 8 {
        if bits.bit()? == 1 {
            let Ok(word) = bits.bits(4) else { break };
            let (Ok(offset), Ok(size)) = (bits.bits_reversed(word), bits.bits_reversed(word))
            else {
                break;
            };
            let (offset, size) = (offset as usize, size as usize);
            if offset == 0 || offset > out.len() {
                return Err(err("invalid back-reference"));
            }
            let start = out.len() - offset;
            for i in 0..size.min(offset) {
                out.push(out[start + i]);
            }
        } else {
            let Ok(size) = bits.bits_reversed(2) else {
                break;
            };
            for _ in 0..size {
                let Ok(byte) = bits.bits(8) else { break };
                out.push(byte as u8);
            }
        }
    }
    out.truncate(len);
    Ok(out)
}

fn int_at(data: &[u8], at: usize) -> Option<usize> {
    let b = data.get(at..at + 4)?;
    Some(i32::from_le_bytes(b.try_into().unwrap()).max(0) as usize)
}

fn read_filesystem(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut files = Vec::new();
    let mut offset = SECTOR;
    while offset + SECTOR <= data.len() {
        if int_at(data, offset) == Some(2) {
            let name_bytes = &data[offset + 4..offset + 4 + 127];
            let end = name_bytes
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(name_bytes.len());
            let name = String::from_utf8_lossy(&name_bytes[..end]).into_owned();
            let size = int_at(data, offset + 0x8C).ok_or_else(|| err("truncated file entry"))?;
            let mut contents = Vec::with_capacity(size);
            let mut pointer = offset + 0x94;
            while contents.len() < size {
                let Some(block) = int_at(data, pointer).filter(|&b| b != 0) else {
                    break;
                };
                let start = block * SECTOR;
                let chunk = data
                    .get(start..(start + SECTOR).min(data.len()))
                    .ok_or_else(|| err("block out of range"))?;
                contents.extend_from_slice(chunk);
                pointer += 4;
            }
            contents.truncate(size);
            files.push((name, contents));
        }
        offset += SECTOR;
    }
    Ok(files)
}

/// Largest file a BCFS entry can describe: its block list fills the rest of the entry sector.
const MAX_FILE: usize = ((SECTOR - 0x94) / 4 - 1) * SECTOR;

/// Builds a BCFZ-compressed GPX container holding `files`.
pub fn write_container(files: &[(&str, &[u8])]) -> Result<Vec<u8>> {
    if let Some((name, data)) = files.iter().find(|(_, d)| d.len() > MAX_FILE) {
        return Err(err(format!(
            "{name} is too large for a GPX file ({} bytes)",
            data.len()
        )));
    }
    let mut fs = b"BCFS".to_vec();
    fs.extend(build_filesystem(files));
    Ok(compress(&fs))
}

/// BCFS layout: sector 0 is a free-sector bitmap, sector 1 the root directory entry, sector 2
/// the directory's list of entry sectors, then each file's entry sector and data sectors.
fn build_filesystem(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut sectors: Vec<[u8; SECTOR]> = vec![[0; SECTOR]; 3];
    let put = |sec: &mut [u8; SECTOR], at: usize, v: u32| {
        sec[at..at + 4].copy_from_slice(&v.to_le_bytes())
    };
    let mut entries = Vec::new();
    for (name, data) in files {
        let entry_index = sectors.len();
        entries.push(entry_index as u32);
        let mut entry = [0u8; SECTOR];
        put(&mut entry, 0, 2);
        let name = &name.as_bytes()[..name.len().min(126)];
        entry[4..4 + name.len()].copy_from_slice(name);
        put(&mut entry, 0x88, 1);
        put(&mut entry, 0x8C, data.len() as u32);
        sectors.push(entry);
        for (i, chunk) in data.chunks(SECTOR).enumerate() {
            let block = sectors.len() as u32;
            put(&mut sectors[entry_index], 0x94 + 4 * i, block);
            let mut sec = [0u8; SECTOR];
            sec[..chunk.len()].copy_from_slice(chunk);
            sectors.push(sec);
        }
    }
    put(&mut sectors[1], 0, 1);
    sectors[1][4] = b'/';
    put(&mut sectors[1], 0x88, 1);
    put(&mut sectors[1], 0x8C, 4 * entries.len() as u32);
    put(&mut sectors[1], 0x94, 2);
    for (i, e) in entries.iter().enumerate() {
        put(&mut sectors[2], 4 * i, *e);
    }
    // Bitmap: bit n (LSB first) is sector n + 1; 0 = used, 1 = free.
    let mut bitmap = [0xFFu8; SECTOR];
    for n in 0..sectors.len() - 1 {
        bitmap[n / 8] &= !(1 << (n % 8));
    }
    sectors[0] = bitmap;
    sectors.concat()
}

/// BCFZ compression using literal blocks only (valid for any BCFZ decoder).
fn compress(data: &[u8]) -> Vec<u8> {
    let mut out = b"BCFZ".to_vec();
    out.extend((data.len() as i32).to_le_bytes());
    let mut bits = BitWriter::default();
    for chunk in data.chunks(3) {
        bits.push(0, 1);
        bits.push_reversed(chunk.len() as u32, 2);
        for &b in chunk {
            bits.push(b as u32, 8);
        }
    }
    out.extend(bits.finish());
    out
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    used: u32,
}

impl BitWriter {
    fn bit(&mut self, b: u32) {
        if self.used % 8 == 0 {
            self.bytes.push(0);
        }
        if b != 0 {
            *self.bytes.last_mut().unwrap() |= 1 << (7 - self.used % 8);
        }
        self.used += 1;
    }

    fn push(&mut self, v: u32, n: u32) {
        for i in (0..n).rev() {
            self.bit((v >> i) & 1);
        }
    }

    fn push_reversed(&mut self, v: u32, n: u32) {
        for i in 0..n {
            self.bit((v >> i) & 1);
        }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes bits for a tiny BCFZ stream by hand: literal "ab", then a back-reference of 2.
    #[test]
    fn decompresses_literals_and_back_references() {
        let mut bits: Vec<u8> = vec![];
        let push = |bits: &mut Vec<u8>, v: u32, n: u32, reversed: bool| {
            for i in 0..n {
                let bit = if reversed {
                    (v >> i) & 1
                } else {
                    (v >> (n - 1 - i)) & 1
                };
                bits.push(bit as u8);
            }
        };
        push(&mut bits, 0, 1, false); // literal block
        push(&mut bits, 2, 2, true); // two bytes
        push(&mut bits, b'a' as u32, 8, false);
        push(&mut bits, b'b' as u32, 8, false);
        push(&mut bits, 1, 1, false); // back-reference
        push(&mut bits, 2, 4, false); // word size 2
        push(&mut bits, 2, 2, true); // offset 2
        push(&mut bits, 2, 2, true); // size 2
        let mut packed = vec![0u8; bits.len().div_ceil(8)];
        for (i, b) in bits.iter().enumerate() {
            packed[i / 8] |= b << (7 - i % 8);
        }
        let mut data = 4i32.to_le_bytes().to_vec();
        data.extend(packed);
        assert_eq!(decompress(&data).unwrap(), b"abab");
    }

    #[test]
    fn container_round_trips() {
        let big: Vec<u8> = (0..10_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let bytes = write_container(&[("score.gpif", &big), ("misc.xml", b"<x/>")]).unwrap();
        let files = read_container(&bytes).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], ("score.gpif".to_string(), big));
        assert_eq!(files[1].1, b"<x/>");
    }
}
