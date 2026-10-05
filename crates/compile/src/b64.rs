//! Base64, because two callers need it and neither should carry a dependency
//! for twenty lines.
//!
//! The compiler inlines a picture as a `data:` URI with it, and a caller can
//! encode any other bytes with it, such as a digest.

/// The standard base64 alphabet.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode bytes as base64, padded.
///
/// Twenty lines, so the pipeline carries no dependency for it. Base64 costs
/// +33%, which is why assets are the one thing that makes a domain large in
/// bytes (CONTENT-MODEL.md §1.4).
#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3).saturating_mul(4));
    for chunk in bytes.chunks(3) {
        let (a, b, c) = (
            u32::from(chunk.first().copied().unwrap_or(0)),
            u32::from(chunk.get(1).copied().unwrap_or(0)),
            u32::from(chunk.get(2).copied().unwrap_or(0)),
        );
        let triple = (a << 16) | (b << 8) | c;
        let mut push = |shift: u32| {
            let index = usize::try_from((triple >> shift) & 0x3f).unwrap_or(0);
            out.push(char::from(ALPHABET.get(index).copied().unwrap_or(b'A')));
        };
        push(18);
        push(12);
        match chunk.len() {
            1 => out.push_str("=="),
            2 => {
                push(6);
                out.push('=');
            }
            _ => {
                push(6);
                push(0);
            }
        }
    }
    out
}

/// Decode base64 back to bytes, for a caller holding an encoded asset.
///
/// # Errors
/// Returns a message if the text is not base64.
pub fn decode(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len().saturating_mul(3).saturating_div(4));
    let mut held: u32 = 0;
    let mut bits: u32 = 0;
    for byte in text.bytes() {
        if byte == b'=' || byte.is_ascii_whitespace() {
            continue;
        }
        let Some(index) = ALPHABET.iter().position(|c| *c == byte) else {
            return Err(format!("`{}` is not base64", char::from(byte)));
        };
        held = (held << 6) | u32::try_from(index).unwrap_or(0);
        bits = bits.saturating_add(6);
        if bits >= 8 {
            bits = bits.saturating_sub(8);
            out.push(u8::try_from((held >> bits) & 0xff).unwrap_or(0));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn it_matches_the_standard() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn it_round_trips_binary() {
        let bytes: Vec<u8> = (0..=255_u8).collect();
        assert_eq!(decode(&encode(&bytes)).expect("decodes"), bytes);
    }
}
