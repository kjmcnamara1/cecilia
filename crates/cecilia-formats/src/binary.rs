//! Bounds-checked little-endian reader for binary formats.

use crate::{Error, Result};

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    /// Human-readable position (e.g. "track 2, bar 14") attached to errors.
    pub location: Option<String>,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            location: None,
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub fn error(&self, message: impl Into<String>) -> Error {
        Error::Parse {
            offset: self.pos,
            location: self.location.clone(),
            message: message.into(),
        }
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        match end {
            Some(end) => {
                let s = &self.data[self.pos..end];
                self.pos = end;
                Ok(s)
            }
            None => Err(self.error(format!("unexpected end of file reading {n} bytes"))),
        }
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }

    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    pub fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }

    /// Reads an `i32` count, rejecting negative or implausibly large values.
    pub fn count(&mut self, max: usize, what: &str) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 || n as usize > max {
            return Err(self.error(format!("invalid {what} count {n}")));
        }
        Ok(n as usize)
    }
}

/// Decodes legacy Guitar Pro text: UTF-8 when valid, otherwise Windows-1252.
pub fn decode_text(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.trim_end_matches('\0').to_string();
    }
    bytes
        .iter()
        .map(|&b| cp1252(b))
        .collect::<String>()
        .trim_end_matches('\0')
        .to_string()
}

fn cp1252(b: u8) -> char {
    const HIGH: [u16; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013,
        0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
    ];
    match b {
        0x80..=0x9F => char::from_u32(HIGH[(b - 0x80) as usize] as u32).unwrap_or('?'),
        _ => b as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_little_endian_and_reports_eof() {
        let mut r = Reader::new(&[1, 0, 0, 0, 0xFF]);
        assert_eq!(r.i32().unwrap(), 1);
        assert_eq!(r.i8().unwrap(), -1);
        assert!(r.u8().is_err());
    }

    #[test]
    fn decodes_cp1252() {
        assert_eq!(decode_text(b"caf\xe9 \x93x\x94"), "café \u{201C}x\u{201D}");
        assert_eq!(decode_text("naïve".as_bytes()), "naïve");
    }
}
