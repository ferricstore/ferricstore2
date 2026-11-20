//! Logged shard changes (ADR-011).
//!
//! Each change the domain accepted is logged as one record for one Flow, with
//! its committed time and every input the reducer used, so replay decides it
//! again identically. Records name their partition, so replay can route them
//! even when the shard count changed. Layout: a tag byte, then fields; a
//! binary is a `u32` length and bytes, an option a `0`/`1` byte then the
//! value, and integers are big-endian.

use wf_application::commands::queue::Partition;

const CREATE: u8 = 1;
const CLAIM: u8 = 2;
const COMPLETE: u8 = 3;
const RECLAIM: u8 = 4;

/// One logged change to one Flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Logged {
    Create {
        now: u64,
        partition: Partition,
        id: Vec<u8>,
        flow_type: Vec<u8>,
        state: Vec<u8>,
        run_at: Option<u64>,
        payload: Option<Vec<u8>>,
        priority: u8,
    },
    Claim {
        now: u64,
        partition: Partition,
        id: Vec<u8>,
        expected_state: Vec<u8>,
        worker: Vec<u8>,
        token: Vec<u8>,
        lease_ms: u64,
        /// Taken from an expired lease; logged with its own tag.
        reclaim: bool,
    },
    Complete {
        now: u64,
        partition: Partition,
        id: Vec<u8>,
        token: Vec<u8>,
        fence: u64,
        result: Option<Vec<u8>>,
    },
}

impl Logged {
    /// The partition this change belongs to.
    pub(crate) const fn partition(&self) -> &Partition {
        match self {
            Self::Create { partition, .. }
            | Self::Claim { partition, .. }
            | Self::Complete { partition, .. } => partition,
        }
    }

    /// Encodes the change; `None` when a field is longer than `u32::MAX`.
    pub(crate) fn encode(&self) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        match self {
            Self::Create {
                now,
                partition,
                id,
                flow_type,
                state,
                run_at,
                payload,
                priority,
            } => {
                out.push(CREATE);
                head(&mut out, *now, partition, id)?;
                bin(&mut out, flow_type)?;
                bin(&mut out, state)?;
                optional_u64(&mut out, *run_at);
                optional_bin(&mut out, payload.as_deref())?;
                out.push(*priority);
            }
            Self::Claim {
                now,
                partition,
                id,
                expected_state,
                worker,
                token,
                lease_ms,
                reclaim,
            } => {
                out.push(if *reclaim { RECLAIM } else { CLAIM });
                head(&mut out, *now, partition, id)?;
                bin(&mut out, expected_state)?;
                bin(&mut out, worker)?;
                bin(&mut out, token)?;
                out.extend_from_slice(&lease_ms.to_be_bytes());
            }
            Self::Complete {
                now,
                partition,
                id,
                token,
                fence,
                result,
            } => {
                out.push(COMPLETE);
                head(&mut out, *now, partition, id)?;
                bin(&mut out, token)?;
                out.extend_from_slice(&fence.to_be_bytes());
                optional_bin(&mut out, result.as_deref())?;
            }
        }
        Some(out)
    }

    /// Decodes one record; `None` when it is not exactly one valid change.
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        decode(bytes).ok()
    }
}

/// A record that is not exactly one valid change.
struct Malformed;

fn decode(bytes: &[u8]) -> Result<Logged, Malformed> {
    let mut reader = Reader(bytes);
    let tag = reader.u8()?;
    let now = reader.u64()?;
    let partition = Partition::parse(reader.bin()?);
    let id = reader.bin()?.to_vec();
    let logged = match tag {
        CREATE => Logged::Create {
            now,
            partition,
            id,
            flow_type: reader.bin()?.to_vec(),
            state: reader.bin()?.to_vec(),
            run_at: reader.optional_u64()?,
            payload: reader.optional_bin()?,
            priority: reader.u8()?,
        },
        CLAIM | RECLAIM => Logged::Claim {
            now,
            partition,
            id,
            expected_state: reader.bin()?.to_vec(),
            worker: reader.bin()?.to_vec(),
            token: reader.bin()?.to_vec(),
            lease_ms: reader.u64()?,
            reclaim: tag == RECLAIM,
        },
        COMPLETE => Logged::Complete {
            now,
            partition,
            id,
            token: reader.bin()?.to_vec(),
            fence: reader.u64()?,
            result: reader.optional_bin()?,
        },
        _ => return Err(Malformed),
    };
    if reader.0.is_empty() {
        Ok(logged)
    } else {
        Err(Malformed)
    }
}

fn head(out: &mut Vec<u8>, now: u64, partition: &Partition, id: &[u8]) -> Option<()> {
    out.extend_from_slice(&now.to_be_bytes());
    bin(out, &partition.name())?;
    bin(out, id)
}

fn bin(out: &mut Vec<u8>, bytes: &[u8]) -> Option<()> {
    out.extend_from_slice(&u32::try_from(bytes.len()).ok()?.to_be_bytes());
    out.extend_from_slice(bytes);
    Some(())
}

fn optional_bin(out: &mut Vec<u8>, bytes: Option<&[u8]>) -> Option<()> {
    match bytes {
        None => out.push(0),
        Some(bytes) => {
            out.push(1);
            bin(out, bytes)?;
        }
    }
    Some(())
}

fn optional_u64(out: &mut Vec<u8>, value: Option<u64>) {
    match value {
        None => out.push(0),
        Some(value) => {
            out.push(1);
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], Malformed> {
        let (head, rest) = self.0.split_at_checked(length).ok_or(Malformed)?;
        self.0 = rest;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Malformed> {
        self.take(N)?.try_into().map_err(|_| Malformed)
    }

    fn u8(&mut self) -> Result<u8, Malformed> {
        let [byte] = self.array()?;
        Ok(byte)
    }

    fn u64(&mut self) -> Result<u64, Malformed> {
        self.array().map(u64::from_be_bytes)
    }

    fn bin(&mut self) -> Result<&'a [u8], Malformed> {
        let length = u32::from_be_bytes(self.array()?);
        self.take(usize::try_from(length).map_err(|_| Malformed)?)
    }

    fn optional_bin(&mut self) -> Result<Option<Vec<u8>>, Malformed> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.bin()?.to_vec())),
            _ => Err(Malformed),
        }
    }

    fn optional_u64(&mut self) -> Result<Option<u64>, Malformed> {
        match self.u8()? {
            0 => Ok(None),
            1 => self.u64().map(Some),
            _ => Err(Malformed),
        }
    }
}
