//! Fixed header and item fields shared by borrowed validation and owned decoding.

use std::fmt;

use wf_domain::workflow::request::ClientIntent;
use wf_types::version::{CommittedTime, HlcTimestamp, TimestampMillis};

use super::{CodecLimits, reader::Reader};

// Only the explicit atomic-start and Step codecs use this header. Recovery and legacy
// generic preparation retain their independent closed profile allowlists.
pub(super) fn singleton_header<'input>(
    reader: &mut Reader<'input>,
    expected_intent: u16,
) -> Result<RawHeader<'input>, CodecError> {
    if reader.take(4)? != b"FWCP" {
        return Err(CodecError::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    if reader.byte()? != 1 {
        return Err(CodecError::UnsupportedTag);
    }
    let tenant = reader.nonempty(8_192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let request = reader.nonempty(256)?;
    if reader.u16()? != expected_intent {
        return Err(CodecError::UnsupportedVersion);
    }
    let intent = match expected_intent {
        7 => ClientIntent::atomic_start(reader.fixed()?),
        10 => ClientIntent::step_continue(reader.fixed()?),
        11 => ClientIntent::stored_step_continue(reader.fixed()?),
        _ => return Err(CodecError::UnsupportedVersion),
    };
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| CodecError::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| CodecError::Malformed)?;
    Ok(RawHeader {
        operation_tag: 1,
        tenant,
        group,
        recovery,
        request,
        intent,
        stamp,
    })
}

/// Borrows supported Normal profile/scope/stamp facts without validating its body.
/// Complete intent, operations, derived keys and EOF remain the driver's duty.
/// # Errors
/// Refuses a header/profile/framing error or the explicit encoded byte cap.
pub fn normal_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), CodecError> {
    recovery_header(encoded, limits, false, (false, false))
}

/// Borrows Normal header facts with explicit stored-value intent9 recognition.
/// Complete body validation and actual persistent capability remain mandatory.
/// # Errors
/// Refuses unsupported headers, framing and the configured encoded byte cap.
pub fn stored_value_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), CodecError> {
    recovery_header(encoded, limits, true, (false, false))
}

/// Borrows header facts with explicit Step and optional stored-value recognition.
/// Complete body validation and actual persistent capability remain mandatory.
/// # Errors
/// Refuses unsupported selection, framing and the configured encoded byte cap.
pub fn step_continue_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
    stored: bool,
) -> Result<(u16, [&[u8]; 3], CommittedTime), CodecError> {
    recovery_header(encoded, limits, stored, (true, false))
}

/// Borrows explicit stored Step headers with separately selected older profiles.
/// Complete body validation and persistent capability remain mandatory.
/// # Errors
/// Refuses unsupported headers, framing and the configured encoded byte cap.
pub fn stored_step_continue_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
    stored_values: bool,
    plain_step: bool,
) -> Result<(u16, [&[u8]; 3], CommittedTime), CodecError> {
    recovery_header(encoded, limits, stored_values, (plain_step, true))
}

fn recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
    stored: bool,
    step: (bool, bool),
) -> Result<(u16, [&[u8]; 3], CommittedTime), CodecError> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    if encoded.get(6) == Some(&9) {
        return original_policy_header(encoded, limits);
    }
    let mut reader = Reader::new(encoded, limits);
    if reader.take(4)? != b"FWCP" {
        return Err(CodecError::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    let operation_tag = reader.byte()?;
    if !matches!(operation_tag, 1..=5) {
        return Err(CodecError::UnsupportedTag);
    }
    let tenant = reader.nonempty(8_192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let _request = reader.nonempty(256)?;
    let version = reader.u16()?;
    let _digest = reader.take(32)?;
    let allowed = matches!((version, operation_tag), (1, _) | (5..=7, 1) | (2, 5))
        || (version, operation_tag) == (8, 1)
        || (stored && (version, operation_tag) == (9, 1))
        || (step.0 && (version, operation_tag) == (10, 1))
        || (step.1 && (version, operation_tag) == (11, 1));
    if !allowed {
        return Err(CodecError::UnsupportedVersion);
    }
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| CodecError::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| CodecError::Malformed)?;
    let profile = match (operation_tag, version) {
        (1, 5) => 5,
        (1, 6) => 6,
        (1, 7) => 7,
        (1, 8) => 8,
        (1, 9) => 9,
        (1, 10) => 10,
        (1, 11) => 11,
        (4, 1) => 1,
        (3, 1) => 3,
        (5, 2) => 2,
        _ => return Err(CodecError::UnsupportedTag),
    };
    Ok((profile, [tenant, group, recovery], stamp))
}

fn original_policy_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), CodecError> {
    let mut reader = Reader::new(encoded, limits);
    if reader.take(4)? != b"FWCP" {
        return Err(CodecError::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    if reader.byte()? != 9 {
        return Err(CodecError::UnsupportedTag);
    }
    let tenant = reader.nonempty(8192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let _request = reader.nonempty(256)?;
    if reader.u16()? != 4 {
        return Err(CodecError::UnsupportedTag);
    }
    let _digest = reader.take(32)?;
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| CodecError::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| CodecError::Malformed)?;
    Ok((4, [tenant, group, recovery], stamp))
}

pub(super) struct RawHeader<'a> {
    pub(super) operation_tag: u8,
    pub(super) tenant: &'a [u8],
    pub(super) group: &'a [u8],
    pub(super) recovery: &'a [u8],
    pub(super) request: &'a [u8],
    pub(super) intent: ClientIntent,
    pub(super) stamp: CommittedTime,
}

impl RawHeader<'_> {
    pub(super) const fn scope(&self) -> [&[u8]; 3] {
        [self.tenant, self.group, self.recovery]
    }
}

pub(super) struct RawItem<'a> {
    pub(super) partition: &'a [u8],
    pub(super) id: &'a [u8],
    pub(super) priority: u8,
    pub(super) now: TimestampMillis,
    pub(super) tag: u8,
}

pub(super) fn header<'a>(reader: &mut Reader<'a>) -> Result<RawHeader<'a>, CodecError> {
    read_header(reader, false)
}

pub(super) fn stored_header<'a>(reader: &mut Reader<'a>) -> Result<RawHeader<'a>, CodecError> {
    read_header(reader, true)
}

fn read_header<'a>(reader: &mut Reader<'a>, stored: bool) -> Result<RawHeader<'a>, CodecError> {
    if reader.take(4)? != b"FWCP" {
        return Err(CodecError::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    let operation_tag = reader.byte()?;
    if !matches!(operation_tag, 1..=5) {
        return Err(CodecError::UnsupportedTag);
    }
    let tenant = reader.nonempty(8_192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let request = reader.nonempty(256)?;
    let version = reader.u16()?;
    let digest = reader.fixed()?;
    let intent = match (version, operation_tag) {
        (1, _) => ClientIntent::new(1, digest).map_err(|_| CodecError::UnsupportedVersion)?,
        (5, 1) => ClientIntent::record_batch(digest),
        (6, 1) => ClientIntent::independent_record_batch(digest),
        (8, 1) => ClientIntent::workflow_value_batch(digest),
        (9, 1) if stored => ClientIntent::workflow_stored_value_batch(digest),
        (2, 5) => ClientIntent::complete_core_policy_patch(digest),
        _ => return Err(CodecError::UnsupportedVersion),
    };
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| CodecError::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| CodecError::Malformed)?;
    Ok(RawHeader {
        operation_tag,
        tenant,
        group,
        recovery,
        request,
        intent,
        stamp,
    })
}

pub(super) fn count(reader: &mut Reader<'_>) -> Result<usize, CodecError> {
    let count = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    if count == 0 || count > reader.limits.admission.batch_items() {
        return Err(CodecError::ItemLimit);
    }
    // Each nonempty pair of IDs plus priority/time/tag consumes at least20 bytes.
    if count > reader.remaining() / 20 {
        return Err(CodecError::Malformed);
    }
    Ok(count)
}

pub(super) fn item<'a>(reader: &mut Reader<'a>) -> Result<RawItem<'a>, CodecError> {
    Ok(RawItem {
        partition: reader.nonempty(65_535)?,
        id: reader.nonempty(65_535)?,
        priority: reader.byte()?,
        now: TimestampMillis::new(reader.u64()?),
        tag: reader.byte()?,
    })
}

/// Operational preparation/corruption failure; never a retained business outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    /// Private command version is unsupported.
    UnsupportedVersion,
    /// An operation/backoff discriminator is unknown.
    UnsupportedTag,
    /// Structurally valid original ETF needs another captured atom profile.
    UnsupportedPolicyProfile,
    /// A present/absent tag is neither zero nor one.
    InvalidOptionality,
    /// Fixed fields, typed identities or input continuity are invalid.
    Malformed,
    /// Bytes remain after the complete represented command.
    TrailingBytes,
    /// Complete input/output or derived-key aggregate exceeds its profile.
    ByteLimit,
    /// A batch is empty or a batch/filter count exceeds its structural profile.
    ItemLimit,
    /// A length-prefixed variable/nested field exceeds its configured byte cap.
    ValueLimit,
    /// Complete derived state key exceeds 65,535 bytes.
    KeyLimit,
    /// Complete derived isolation scope exceeds 8,192 bytes.
    ScopeLimit,
    /// A checked length/count cannot fit its explicit representation.
    LengthOverflow,
    /// The complete logical work cohort exceeds its explicit cap.
    WorkLimit,
    /// Requested simultaneous logical ownership exceeds its explicit cap.
    OwnedLimit,
    /// A safe fallible builder reservation failed.
    Allocation,
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedVersion => "unsupported private prepared version",
            Self::UnsupportedTag => "unsupported private prepared tag",
            Self::UnsupportedPolicyProfile => "unsupported original policy profile",
            Self::InvalidOptionality => "invalid private prepared optionality",
            Self::Malformed => "malformed private prepared command",
            Self::TrailingBytes => "trailing private prepared bytes",
            Self::ByteLimit => "private prepared byte budget exceeded",
            Self::ItemLimit => "private prepared item budget exceeded",
            Self::ValueLimit => "private prepared field budget exceeded",
            Self::KeyLimit => "private prepared state key too large",
            Self::ScopeLimit => "private prepared isolation scope too large",
            Self::LengthOverflow => "private prepared length overflow",
            Self::WorkLimit => "private prepared work budget exceeded",
            Self::OwnedLimit => "private prepared owned-byte budget exceeded",
            Self::Allocation => "private prepared allocation failed",
        })
    }
}
impl std::error::Error for CodecError {}
