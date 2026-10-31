//! SHA256/base64url literals computed independently with Python's standard library.
use wf_ports::log::running_claim_scope;

#[test]
fn exact_running_scope_preserves_all_type_bytes_and_unpadded_url_encoding() {
    for (kind, expected) in [
        (
            b"".as_slice(),
            b"flow-running:47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU".as_slice(),
        ),
        (
            b"r",
            b"flow-running:RUNJ5CLwUpcZHq0T4h09tSDlq-9SBV5JZLgvshP1k6E",
        ),
        (
            b"\0\xff",
            b"flow-running:But9amnuGeX733SQGNPSq_oEvL0TZdsxLrhtxxaTibg",
        ),
    ] {
        assert_eq!(running_claim_scope(kind), expected);
    }
}
