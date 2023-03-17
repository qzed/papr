use crate::{Error, Result};

pub fn utf16le_from_bytes(bytes: &[u8]) -> Result<String> {
    if bytes.len() & 1 != 0 {
        return Err(Error::InvalidEncoding);
    }

    let mut chars: Vec<u16> = bytes
        .chunks(2)
        .map(|bytes| u16::from_le_bytes(bytes.try_into().unwrap()))
        .collect();

    let n = if let Some(i) = chars.iter().rposition(|c| *c != 0) {
        i + 1
    } else {
        0
    };
    chars.truncate(n);

    let value = String::from_utf16(&chars).map_err(|_| Error::InvalidEncoding)?;
    Ok(value)
}
