//! One-time codes for `login(site)` (RFC 6238 TOTP, HMAC-SHA1, as nearly
//! every site uses), computed by the gateway so the seed never leaves it.

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

/// Decodes an RFC 4648 base32 seed, as sites show it (spaces, lower case
/// and padding allowed).
pub fn base32(s: &str) -> Option<Vec<u8>> {
    let mut bits: u64 = 0;
    let mut n = 0u32;
    let mut out = vec![];
    for c in s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '=' && *c != '-')
    {
        let v = match c.to_ascii_uppercase() {
            x @ 'A'..='Z' => x as u64 - 'A' as u64,
            x @ '2'..='7' => x as u64 - '2' as u64 + 26,
            _ => return None,
        };
        bits = (bits << 5) | v;
        n += 5;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
            bits &= (1 << n) - 1;
        }
    }
    (!out.is_empty()).then_some(out)
}

/// The code for `key` at `unix_secs`, 30-second steps.
pub fn code(key: &[u8], unix_secs: u64, digits: u32) -> String {
    let counter = (unix_secs / 30).to_be_bytes();
    let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(key).expect("any key length");
    mac.update(&counter);
    let h = mac.finalize().into_bytes();
    let off = (h[h.len() - 1] & 0x0f) as usize;
    let bin = u32::from_be_bytes([h[off] & 0x7f, h[off + 1], h[off + 2], h[off + 3]]);
    format!(
        "{:0width$}",
        bin % 10u32.pow(digits),
        width = digits as usize
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_6238_sha1_vectors() {
        let key = b"12345678901234567890";
        for (t, want) in [
            (59, "94287082"),
            (1111111109, "07081804"),
            (1111111111, "14050471"),
            (1234567890, "89005924"),
            (2000000000, "69279037"),
            (20000000000, "65353130"),
        ] {
            assert_eq!(code(key, t, 8), want, "t={t}");
        }
        assert_eq!(code(key, 59, 6), "287082");
    }

    #[test]
    fn base32_seeds_decode_as_sites_show_them() {
        // "12345678901234567890" in base32.
        let want = b"12345678901234567890".to_vec();
        assert_eq!(
            base32("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"),
            Some(want.clone())
        );
        assert_eq!(
            base32("gezd gnbv gy3t qojq gezd gnbv gy3t qojq"),
            Some(want)
        );
        assert_eq!(base32("not base32!"), None);
        assert_eq!(base32(""), None);
    }
}
