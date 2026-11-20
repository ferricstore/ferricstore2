//! Logged shard changes (ADR-011).
//!
//! Each change the domain accepted is logged as one record for one Flow, with
//! its committed time and every input the reducer used, so replay decides it
//! again identically. A retention sweep is logged as one record naming every
//! Flow it removed (ADR-013). Records name partitions, so replay can route
//! them even when the shard count changed. A command beyond the queue fast
//! path logs the Flow's whole after-image instead (`Put`, ADR-014), so replay
//! stores it without deciding anything. Layout: a tag byte, then fields; a
//! binary is a `u32` length and bytes, an option a `0`/`1` byte then the
//! value, and integers are big-endian.

use wf_application::commands::queue::Partition;

const CREATE: u8 = 1;
const CLAIM: u8 = 2;
const COMPLETE: u8 = 3;
const RECLAIM: u8 = 4;
const EXPIRE: u8 = 5;
const MARKER: u8 = 6;
const PUT: u8 = 7;
const SHARED: u8 = 8;
const GROUP: u8 = 9;
const POLICY: u8 = 10;

mod stored;

use stored::decode_stored;

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
    /// Completed Flows removed after their retention.
    Expire {
        now: u64,
        keys: Vec<(Partition, Vec<u8>)>,
    },
    /// Where a shard captured its state for a snapshot: its records logged
    /// before this are in the snapshot, those after it are not.
    Marker { snapshot: u64, shard: u16 },
    /// A Flow's state after a command, as a shard image record.
    Put {
        partition: Partition,
        image: Vec<u8>,
    },
    /// Changes logged as one record so they are all or nothing (a spawn:
    /// the parent and its children; ADR-014 step 8).
    Group(Vec<Logged>),
    /// A Flow type's policy as set on shard 0 (ADR-014 step 9): its
    /// generation and its normalized form as an encoded term.
    Policy {
        flow_type: Vec<u8>,
        generation: u64,
        policy: Vec<u8>,
    },
    /// A shared value stored under its reference (ADR-014 step 5).
    Shared {
        partition: Partition,
        reference: Vec<u8>,
        bytes: Vec<u8>,
        expires_at: Option<u64>,
    },
}

impl Logged {
    /// The partition of a single-Flow change; `None` for a sweep.
    pub(crate) fn partition(&self) -> Option<&Partition> {
        match self {
            Self::Create { partition, .. }
            | Self::Claim { partition, .. }
            | Self::Complete { partition, .. }
            | Self::Put { partition, .. }
            | Self::Shared { partition, .. } => Some(partition),
            Self::Group(changes) => changes.first().and_then(Self::partition),
            Self::Expire { .. } | Self::Marker { .. } | Self::Policy { .. } => None,
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
            Self::Put { .. } | Self::Group(_) | Self::Shared { .. } | Self::Policy { .. } => {
                stored::encode(self, &mut out)?;
            }
            Self::Marker { snapshot, shard } => {
                out.push(MARKER);
                out.extend_from_slice(&snapshot.to_be_bytes());
                out.extend_from_slice(&shard.to_be_bytes());
            }
            Self::Expire { now, keys } => {
                out.push(EXPIRE);
                out.extend_from_slice(&now.to_be_bytes());
                out.extend_from_slice(&u32::try_from(keys.len()).ok()?.to_be_bytes());
                for (partition, id) in keys {
                    bin(&mut out, &partition.name())?;
                    bin(&mut out, id)?;
                }
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
pub(super) struct Malformed;

fn decode(bytes: &[u8]) -> Result<Logged, Malformed> {
    let mut reader = Reader(bytes);
    let tag = reader.u8()?;
    if let Some(logged) = decode_stored(tag, &mut reader)? {
        return Ok(logged);
    }
    let now = reader.u64()?;
    if tag == MARKER {
        let shard = u16::from_be_bytes(reader.array()?);
        return if reader.0.is_empty() {
            Ok(Logged::Marker {
                snapshot: now,
                shard,
            })
        } else {
            Err(Malformed)
        };
    }
    if tag == EXPIRE {
        let count = u32::from_be_bytes(reader.array()?);
        let mut keys = Vec::new();
        for _ in 0..count {
            let partition = Partition::parse(reader.bin()?);
            keys.push((partition, reader.bin()?.to_vec()));
        }
        return if reader.0.is_empty() && !keys.is_empty() {
            Ok(Logged::Expire { now, keys })
        } else {
            Err(Malformed)
        };
    }
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

pub(super) fn bin(out: &mut Vec<u8>, bytes: &[u8]) -> Option<()> {
    out.extend_from_slice(&u32::try_from(bytes.len()).ok()?.to_be_bytes());
    out.extend_from_slice(bytes);
    Some(())
}

pub(super) fn optional_bin(out: &mut Vec<u8>, bytes: Option<&[u8]>) -> Option<()> {
    match bytes {
        None => out.push(0),
        Some(bytes) => {
            out.push(1);
            bin(out, bytes)?;
        }
    }
    Some(())
}

pub(super) fn optional_u64(out: &mut Vec<u8>, value: Option<u64>) {
    match value {
        None => out.push(0),
        Some(value) => {
            out.push(1);
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
}

pub(super) struct Reader<'a>(pub(super) &'a [u8]);

impl<'a> Reader<'a> {
    pub(super) fn take(&mut self, length: usize) -> Result<&'a [u8], Malformed> {
        let (head, rest) = self.0.split_at_checked(length).ok_or(Malformed)?;
        self.0 = rest;
        Ok(head)
    }

    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N], Malformed> {
        self.take(N)?.try_into().map_err(|_| Malformed)
    }

    pub(super) fn u8(&mut self) -> Result<u8, Malformed> {
        let [byte] = self.array()?;
        Ok(byte)
    }

    pub(super) fn u64(&mut self) -> Result<u64, Malformed> {
        self.array().map(u64::from_be_bytes)
    }

    pub(super) fn bin(&mut self) -> Result<&'a [u8], Malformed> {
        let length = u32::from_be_bytes(self.array()?);
        self.take(usize::try_from(length).map_err(|_| Malformed)?)
    }

    pub(super) fn optional_bin(&mut self) -> Result<Option<Vec<u8>>, Malformed> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.bin()?.to_vec())),
            _ => Err(Malformed),
        }
    }

    pub(super) fn optional_u64(&mut self) -> Result<Option<u64>, Malformed> {
        match self.u8()? {
            0 => Ok(None),
            1 => self.u64().map(Some),
            _ => Err(Malformed),
        }
    }
}
