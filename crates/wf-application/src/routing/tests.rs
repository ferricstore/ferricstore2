//! Private routing helpers against Python zlib and base64 vectors.

use super::{base64url_nopad, crc32};

#[test]
fn crc32_matches_zlib() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
}

#[test]
fn base64url_has_no_padding_and_the_url_alphabet() {
    for (input, expected) in [
        (&b""[..], ""),
        (b"f", "Zg"),
        (b"fo", "Zm8"),
        (b"foo", "Zm9v"),
        (b"\xff\xfe\xfd\xfc", "__79_A"),
    ] {
        assert_eq!(base64url_nopad(input), expected);
    }
}
