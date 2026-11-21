//! Encoding and decoding the records that store state as logged instead of
//! deciding it.

use wf_application::commands::queue::Partition;

use super::{
    GOVERNANCE, GROUP, Logged, Malformed, POLICY, PUT, Reader, SHARED, bin, decode, optional_bin,
    optional_u64,
};

/// Records that store state as logged: a group, a shared value, an
/// after-image, a policy or a governance record; `None` for any other tag.
pub(super) fn decode_stored(tag: u8, reader: &mut Reader<'_>) -> Result<Option<Logged>, Malformed> {
    let logged = match tag {
        GROUP => {
            let count = u32::from_be_bytes(reader.array()?);
            let mut changes = Vec::new();
            for _ in 0..count {
                changes.push(decode(reader.bin()?)?);
            }
            if changes.is_empty() {
                return Err(Malformed);
            }
            Logged::Group(changes)
        }
        SHARED => Logged::Shared {
            partition: Partition::parse(reader.bin()?),
            reference: reader.bin()?.to_vec(),
            bytes: reader.bin()?.to_vec(),
            expires_at: reader.optional_u64()?,
        },
        PUT => Logged::Put {
            partition: Partition::parse(reader.bin()?),
            image: reader.bin()?.to_vec(),
        },
        POLICY => Logged::Policy {
            flow_type: reader.bin()?.to_vec(),
            generation: reader.u64()?,
            policy: reader.bin()?.to_vec(),
        },
        GOVERNANCE => Logged::Governance {
            partition: Partition::parse(reader.bin()?),
            kind: reader.u8()?,
            name: reader.bin()?.to_vec(),
            value: reader.optional_bin()?,
        },
        _ => return Ok(None),
    };
    if reader.0.is_empty() {
        Ok(Some(logged))
    } else {
        Err(Malformed)
    }
}

/// Writes a record that stores state as logged; nothing for any other.
pub(super) fn encode(logged: &Logged, out: &mut Vec<u8>) -> Option<()> {
    match logged {
        Logged::Put { partition, image } => {
            out.push(PUT);
            bin(out, &partition.name())?;
            bin(out, image)?;
        }
        Logged::Group(changes) => {
            out.push(GROUP);
            out.extend_from_slice(&u32::try_from(changes.len()).ok()?.to_be_bytes());
            for change in changes {
                bin(out, &change.encode()?)?;
            }
        }
        Logged::Shared {
            partition,
            reference,
            bytes,
            expires_at,
        } => {
            out.push(SHARED);
            bin(out, &partition.name())?;
            bin(out, reference)?;
            bin(out, bytes)?;
            optional_u64(out, *expires_at);
        }
        Logged::Policy {
            flow_type,
            generation,
            policy,
        } => {
            out.push(POLICY);
            bin(out, flow_type)?;
            out.extend_from_slice(&generation.to_be_bytes());
            bin(out, policy)?;
        }
        Logged::Governance {
            partition,
            kind,
            name,
            value,
        } => {
            out.push(GOVERNANCE);
            bin(out, &partition.name())?;
            out.push(*kind);
            bin(out, name)?;
            optional_bin(out, value.as_deref())?;
        }
        _ => {}
    }
    Some(())
}
