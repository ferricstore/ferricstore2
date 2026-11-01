//! Source global/automatic/SHA partition tags with prospective output caps.

use sha2::{Digest, Sha256};

use crate::blob::reference::{base64, parse_version, put, put_version};
use crate::blob::{OriginalValueBuildError as E, OriginalValuePartition as P};

const AUTO_PARTITION_PREFIX: &[u8] = b"__flow_auto__:";

pub(in crate::blob::reference) fn valid_tag(input: &[u8]) -> bool {
    if input == b"f" {
        return true;
    }
    if let Some(bucket) = input.strip_prefix(b"fa:") {
        return auto_bucket(bucket).is_some();
    }
    if let Some(digest) = input.strip_prefix(b"f:") {
        return digest.len() == 43 && base64::valid(digest);
    }
    false
}

pub(in crate::blob::reference) fn render_tag(
    partition: P<'_>,
    id: &[u8],
    maximum_tag_bytes: usize,
    output: &mut [u8; 49],
) -> Result<usize, E> {
    let bucket = match partition {
        P::Automatic => {
            if maximum_tag_bytes < 8 {
                return Err(E::Limit);
            }
            Some(crc_bucket(id))
        }
        P::Explicit(bytes) => bytes
            .strip_prefix(AUTO_PARTITION_PREFIX)
            .and_then(auto_bucket),
        P::Global => None,
    };
    let mut cursor = 0_usize;
    if let Some(bucket) = bucket {
        let length = 7 + if bucket < 10 {
            1
        } else if bucket < 100 {
            2
        } else {
            3
        };
        if length > maximum_tag_bytes {
            return Err(E::Limit);
        }
        put(output, &mut cursor, b"f:{fa:")?;
        put_version(output, &mut cursor, u64::from(bucket))?;
        put(output, &mut cursor, b"}")?;
    } else {
        match partition {
            P::Global => {
                if maximum_tag_bytes < 5 {
                    return Err(E::Limit);
                }
                put(output, &mut cursor, b"f:{f}")?;
            }
            P::Explicit(bytes) => {
                if maximum_tag_bytes < 49 {
                    return Err(E::Limit);
                }
                let digest = Sha256::digest(bytes);
                put(output, &mut cursor, b"f:{f:")?;
                let written =
                    base64::encode_into(digest.as_ref(), output.get_mut(5..48).ok_or(E::Limit)?)?;
                cursor = cursor.checked_add(written).ok_or(E::Limit)?;
                put(output, &mut cursor, b"}")?;
            }
            P::Automatic => return Err(E::InvalidInput),
        }
    }
    Ok(cursor)
}

pub(in crate::blob::reference) fn crc_bucket(input: &[u8]) -> u8 {
    let mut crc = u32::MAX;
    for byte in input {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let polynomial = if crc & 1 == 0 { 0 } else { 0xedb8_8320 };
            crc = (crc >> 1) ^ polynomial;
        }
    }
    (!crc).to_le_bytes()[0]
}

fn auto_bucket(input: &[u8]) -> Option<u8> {
    u8::try_from(parse_version(input)?).ok()
}
