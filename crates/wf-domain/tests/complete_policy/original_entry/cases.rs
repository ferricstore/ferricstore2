//! Independent ETF recipes, never a candidate encoder or accepted-entry input.

use super::{Error, OriginalEtfSafety, PolicySchemaCharge, TestResult, decode, limits};

pub(super) const TAG: &str = "flow_policy_v1";
pub(super) const MIN: &str = "836803770e666c6f775f706f6c6963795f763161007400000000";
pub(super) const TYPE_T: &str =
    "836803770e666c6f775f706f6c6963795f7631610074000000017704747970656d0000000174";
pub(super) const SOURCE_603: &str = concat!(
    "836802770e666c6f775f706f6c6963795f76317400000003770572657472797400000001",
    "770c6d61785f617474656d7074736105770673746174657374000000016d0000000b",
    "6368617267655f636172647400000001770572657472797400000001770c6d6178",
    "5f617474656d70747361027704747970656d00000008636865636b6f7574"
);
pub(super) const COMPRESSED: &str = concat!(
    "835000000025789ccb602ee74bcbc92f8f2fc8cfc94cae8c2f334c6428616060602c",
    "6729a92c48cd05314b00e04b0a93"
);

pub(super) fn hex(value: &str) -> TestResult<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return Err("odd independent hex literal".into());
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

pub(super) fn atom(value: &str) -> TestResult<Vec<u8>> {
    let mut bytes = vec![119, u8::try_from(value.len())?];
    bytes.extend_from_slice(value.as_bytes());
    Ok(bytes)
}

pub(super) fn binary(value: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![109];
    bytes.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(bytes)
}

pub(super) fn integer(value: i64) -> TestResult<Vec<u8>> {
    if let Ok(small) = u8::try_from(value) {
        return Ok(vec![97, small]);
    }
    if let Ok(signed) = i32::try_from(value) {
        let mut bytes = vec![98];
        bytes.extend_from_slice(&signed.to_be_bytes());
        return Ok(bytes);
    }
    let mut magnitude = value.unsigned_abs().to_le_bytes().to_vec();
    while magnitude.len() > 1 && magnitude.last() == Some(&0) {
        magnitude.pop();
    }
    let mut bytes = vec![110, u8::try_from(magnitude.len())?, u8::from(value < 0)];
    bytes.extend_from_slice(&magnitude);
    Ok(bytes)
}

pub(super) fn map(rows: &[(Vec<u8>, Vec<u8>)]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![116];
    bytes.extend_from_slice(&u32::try_from(rows.len())?.to_be_bytes());
    for (key, value) in rows {
        bytes.extend_from_slice(key);
        bytes.extend_from_slice(value);
    }
    Ok(bytes)
}

pub(super) fn tuple(values: &[Vec<u8>]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![104, u8::try_from(values.len())?];
    for value in values {
        bytes.extend_from_slice(value);
    }
    Ok(bytes)
}

pub(super) fn list(values: &[Vec<u8>], tail: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![108];
    bytes.extend_from_slice(&u32::try_from(values.len())?.to_be_bytes());
    for value in values {
        bytes.extend_from_slice(value);
    }
    bytes.extend_from_slice(tail);
    Ok(bytes)
}

pub(super) fn string(value: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![107];
    bytes.extend_from_slice(&u16::try_from(value.len())?.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(bytes)
}

pub(super) fn bits(value: &[u8], last_bits: u8) -> TestResult<Vec<u8>> {
    let mut bytes = vec![77];
    bytes.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    bytes.push(last_bits);
    bytes.extend_from_slice(value);
    Ok(bytes)
}

pub(super) fn float(bits: u64) -> Vec<u8> {
    let mut bytes = vec![70];
    bytes.extend_from_slice(&bits.to_be_bytes());
    bytes
}

pub(super) fn legacy_float(text: &[u8]) -> TestResult<Vec<u8>> {
    if text.len() >= 31 {
        return Err("legacy literal needs a terminator".into());
    }
    let mut bytes = vec![99];
    bytes.extend_from_slice(text);
    bytes.resize(32, 0);
    Ok(bytes)
}

pub(super) fn envelope(policy: &[u8], generation: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![131, 104, 3];
    bytes.extend_from_slice(&atom(TAG)?);
    bytes.extend_from_slice(generation);
    bytes.extend_from_slice(policy);
    Ok(bytes)
}

pub(super) fn pid(node: &str, id: u32, serial: u32, creation: u32) -> TestResult<Vec<u8>> {
    let mut bytes = vec![88];
    bytes.extend_from_slice(&atom(node)?);
    for word in [id, serial, creation] {
        bytes.extend_from_slice(&word.to_be_bytes());
    }
    Ok(bytes)
}

pub(super) fn export(arity: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![113];
    bytes.extend_from_slice(&atom("m")?);
    bytes.extend_from_slice(&atom("f")?);
    bytes.extend_from_slice(arity);
    Ok(bytes)
}

pub(super) fn fun(
    fields: (u8, i32, i64, i64),
    uniq: [u8; 16],
    creator: &[u8],
    environment: &[Vec<u8>],
    size: u32,
) -> TestResult<Vec<u8>> {
    let (arity, index, old_index, old_uniq) = fields;
    let mut bytes = vec![112];
    bytes.extend_from_slice(&size.to_be_bytes());
    bytes.push(arity);
    bytes.extend_from_slice(&uniq);
    bytes.extend_from_slice(&index.to_be_bytes());
    bytes.extend_from_slice(&u32::try_from(environment.len())?.to_be_bytes());
    bytes.extend_from_slice(&atom("m")?);
    bytes.extend_from_slice(&integer(old_index)?);
    bytes.extend_from_slice(&integer(old_uniq)?);
    bytes.extend_from_slice(creator);
    for value in environment {
        bytes.extend_from_slice(value);
    }
    Ok(bytes)
}

pub(super) fn reference(node: &str, creation: u32, words: &[u32]) -> TestResult<Vec<u8>> {
    let mut bytes = vec![90];
    bytes.extend_from_slice(&u16::try_from(words.len())?.to_be_bytes());
    bytes.extend_from_slice(&atom(node)?);
    bytes.extend_from_slice(&creation.to_be_bytes());
    for word in words {
        bytes.extend_from_slice(&word.to_be_bytes());
    }
    Ok(bytes)
}

pub(super) fn check(bytes: &[u8], safety: &OriginalEtfSafety<'_>, accepted: bool) -> TestResult {
    let profile = limits(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let result = decode(bytes, safety, profile, &mut charge)?;
    assert_eq!(result.is_some(), accepted, "raw {bytes:02x?}");
    if let Some(entry) = result {
        assert_eq!(entry.encoded(), bytes);
        assert_ne!(entry.policy_bytes(), []);
    }
    Ok(())
}

pub(super) fn paired(
    left: Vec<u8>,
    right: Vec<u8>,
    safety: &OriginalEtfSafety<'_>,
    duplicate: bool,
) -> TestResult {
    for operand in [&left, &right] {
        let single = map(&[(operand.clone(), vec![106])])?;
        check(&envelope(&single, &[97, 0])?, safety, true)?;
    }
    let mut rows = [(left, vec![106]), (right, vec![97, 0])];
    for reversed in [false, true] {
        if reversed {
            rows.reverse();
        }
        let policy = map(&rows)?;
        let bytes = envelope(&policy, &[97, 0])?;
        check(&bytes, safety, !duplicate)?;
        assert_eq!(&bytes[21..], policy);
        assert_eq!(rows.len(), 2);
    }
    Ok(())
}

pub(super) fn work(target: u64, maximum: u64) -> Error {
    Error::Schema(super::PolicySchemaError::WorkBytes {
        bytes: target,
        maximum,
    })
}
