//! QuickLZ level 1 decompression. Ported from `tslib/quicklz.ts`.
//! Unlike the old code, a stream that reads or writes out of bounds is an error.

use crate::{err, Error, Result};

const TABLE_SIZE: usize = 4096;
const MAX_DECOMPRESSED_SIZE: usize = 1024 * 1024;

fn hash(value: u32) -> usize {
    (((value >> 12) ^ value) & 0xfff) as usize
}

fn truncated() -> Error {
    Error("Truncated QuickLZ data".into())
}

pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    let flags = *data.first().ok_or_else(truncated)?;
    let level = (flags >> 2) & 0x03;
    if level != 1 {
        return err(format!("QuickLZ level {level} not supported"));
    }
    let long_header = flags & 0x02 != 0;
    let header_len = if long_header { 9 } else { 3 };
    let size = if long_header {
        let bytes: [u8; 4] = data.get(5..9).ok_or_else(truncated)?.try_into().expect("4 bytes");
        i32::from_le_bytes(bytes)
    } else {
        i32::from(*data.get(2).ok_or_else(truncated)?)
    };
    let size = usize::try_from(size).or_else(|_| err("Negative QuickLZ size"))?;
    if size > MAX_DECOMPRESSED_SIZE {
        return err(format!("Decompressed size {size} exceeds max"));
    }
    if flags & 0x01 == 0 {
        return Ok(data.get(header_len..header_len + size).ok_or_else(truncated)?.to_vec());
    }

    let mut dest = vec![0u8; size];
    let mut table = vec![0usize; TABLE_SIZE];
    let mut control = 1u32;
    let mut src = header_len;
    let mut pos = 0usize;
    let mut next_hashed = 0usize;
    let next = |src: &mut usize| -> Result<u8> {
        let byte = *data.get(*src).ok_or_else(truncated)?;
        *src += 1;
        Ok(byte)
    };

    loop {
        if control == 1 {
            let bytes: [u8; 4] = data.get(src..src + 4).ok_or_else(truncated)?.try_into().expect("4 bytes");
            control = u32::from_le_bytes(bytes);
            src += 4;
        }
        if control & 1 != 0 {
            control >>= 1;
            let first = next(&mut src)?;
            let second = next(&mut src)?;
            let index = usize::from(first >> 4) | (usize::from(second) << 4);
            let mut len = usize::from(first & 0x0f);
            if len != 0 {
                len += 2;
            } else {
                len = usize::from(next(&mut src)?);
            }
            let offset = table[index];
            // The old code always copies at least three bytes; a shorter match does not exist.
            let copy = len.max(3);
            if pos + copy > size || offset + copy > size {
                return err("QuickLZ reference out of bounds");
            }
            for i in 0..copy {
                dest[pos + i] = dest[offset + i];
            }
            pos += len;
            update_table(&mut table, &dest, next_hashed, (pos + 1).saturating_sub(len))?;
            next_hashed = pos;
        } else if pos >= size.max(10) - 10 {
            while pos < size {
                if control == 1 {
                    src += 4;
                }
                control >>= 1;
                dest[pos] = next(&mut src)?;
                pos += 1;
            }
            return Ok(dest);
        } else {
            dest[pos] = next(&mut src)?;
            pos += 1;
            control >>= 1;
            let end = pos.saturating_sub(2);
            update_table(&mut table, &dest, next_hashed, end)?;
            next_hashed = next_hashed.max(end);
        }
    }
}

fn update_table(table: &mut [usize], dest: &[u8], from: usize, end: usize) -> Result<()> {
    if from >= end {
        return Ok(());
    }
    if end >= dest.len() || from + 2 >= dest.len() {
        return err("QuickLZ hash out of bounds");
    }
    let mut value = u32::from(dest[from]) | u32::from(dest[from + 1]) << 8 | u32::from(dest[from + 2]) << 16;
    table[hash(value)] = from;
    for i in from + 1..end {
        value = (value >> 8) | u32::from(*dest.get(i + 2).ok_or_else(truncated)?) << 16;
        table[hash(value)] = i;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_streams_are_errors_not_panics() {
        assert!(decompress(&[]).is_err());
        assert!(decompress(&[0x05]).is_err());
        assert!(decompress(&[0x00, 3, 3]).is_err(), "level 0");
        assert!(decompress(&[0x05, 3, 200, 1, 0]).is_err(), "control word cut short");
        // A back-reference past the end of the output.
        assert!(decompress(&[0x05, 9, 4, 0xff, 0xff, 0xff, 0xff, 0x0f, 0x00]).is_err());
    }
}
