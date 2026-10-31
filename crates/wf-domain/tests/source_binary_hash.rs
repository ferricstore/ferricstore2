//! Portable OTP29 binary recipe fixtures; these are not native VM captures.
#![forbid(unsafe_code)]
use wf_domain::retry::source_binary_hash;

#[test]
fn standalone_binary_hash_keeps_the_full_32_bit_value_and_twelve_byte_boundaries() {
    // Literal results from an independent integer-arithmetic transcription of
    // pinned erl_term_hashing.c block_hash, with HCONST_13 and initial hash zero.
    for (bytes, expected) in [
        (b"".as_slice(), 147_926_629),
        (b"new", 3_596_244_618),
        (b"i", 1_541_010_383),
        (b"abc", 1_306_188_027),
        (b"\0\xff\0\x80", 3_342_255_898),
        (&[1; 11], 3_296_452_022),
        (&[1; 12], 706_224_998),
        (&[1; 13], 57_310_419),
    ] {
        assert_eq!(source_binary_hash(bytes), expected, "binary {bytes:?}");
    }
    assert_eq!(u64::from(source_binary_hash(b"new")) % 4, 2);
    assert_eq!(u64::from(source_binary_hash(b"new")) % 7, 1);
}
