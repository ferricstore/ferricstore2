//! FSNP v1 frame header (ADR-011).
//!
//! Every frame starts with 24 big-endian bytes: magic `FSNP`, version (`0x01`
//! for requests, `0x81` for responses), flags, lane `u32`, opcode `u16`,
//! request id `u64` and body length `u32`. The request id only correlates a
//! reply with its request on one connection; it is not a durable identity.

use std::fmt;

/// Header length in bytes.
pub const HEADER_LEN: usize = 24;
/// Largest body length accepted or produced.
pub const MAX_BODY_BYTES: u32 = 16 * 1024 * 1024;
/// The body uses a compact (non-typed) payload layout.
pub const FLAG_CUSTOM_PAYLOAD: u8 = 0x02;
/// The body is compressed; this server never negotiates compression.
pub const FLAG_COMPRESSED: u8 = 0x08;
/// The client expects no reply.
pub const FLAG_NO_REPLY: u8 = 0x10;
/// More chunks of the same body follow; this server never negotiates chunking.
pub const FLAG_MORE_CHUNKS: u8 = 0x20;

const MAGIC: [u8; 4] = *b"FSNP";
const REQUEST_VERSION: u8 = 0x01;
const RESPONSE_VERSION: u8 = 0x81;

/// Opcodes this adapter implements. Every other opcode is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opcode {
    /// Capability negotiation.
    Hello,
    /// Liveness check.
    Ping,
    /// Orderly close.
    Quit,
    /// Capability query.
    Options,
    /// The slot table.
    Shards,
    /// Connection start.
    Startup,
    /// `FLOW.CLAIM_DUE` with a compact or typed-map body.
    FlowClaimDue,
    /// `FLOW.CREATE_MANY` with a typed-map body.
    FlowCreateMany,
    /// `FLOW.COMPLETE_MANY` with a compact body.
    FlowCompleteMany,
    /// `FLOW.RECLAIM` with a typed-map body.
    FlowReclaim,
    /// Several commands in one typed frame.
    Pipeline,
    /// A command sent as text: its name and arguments.
    CommandExec,
    /// Any other Flow command with a typed-map body, by name (ADR-014).
    Flow(&'static str),
    /// A member role command with an `args` list body, by name (ADR-020).
    Cluster(&'static str),
}

/// Member role commands whose `args` list the session reads (ADR-020).
const CLUSTER_COMMANDS: [(&str, u16); 3] = [
    ("CLUSTER.PROMOTE", 0x0309),
    ("CLUSTER.DEMOTE", 0x030A),
    ("CLUSTER.ROLE", 0x030B),
];

/// Flow commands whose typed-map body the application reads (ADR-014).
const FLOW_COMMANDS: [(&str, u16); 62] = [
    ("FLOW.CREATE", 0x0201),
    ("FLOW.GET", 0x0202),
    ("FLOW.COMPLETE", 0x0204),
    ("FLOW.TRANSITION", 0x0205),
    ("FLOW.RETRY", 0x0206),
    ("FLOW.FAIL", 0x0207),
    ("FLOW.CANCEL", 0x0208),
    ("FLOW.EXTEND_LEASE", 0x0209),
    ("FLOW.HISTORY", 0x020A),
    ("FLOW.VALUE.PUT", 0x020B),
    ("FLOW.VALUE.MGET", 0x020C),
    ("FLOW.SIGNAL", 0x020D),
    ("FLOW.TRANSITION_MANY", 0x0211),
    ("FLOW.RETRY_MANY", 0x0212),
    ("FLOW.FAIL_MANY", 0x0213),
    ("FLOW.CANCEL_MANY", 0x0214),
    ("FLOW.REWIND", 0x0216),
    ("FLOW.INFO", 0x021C),
    ("FLOW.POLICY.SET", 0x021E),
    ("FLOW.POLICY.GET", 0x021F),
    ("FLOW.SPAWN_CHILDREN", 0x0220),
    ("FLOW.RETENTION_CLEANUP", 0x0221),
    ("FLOW.STEP_CONTINUE", 0x0222),
    ("FLOW.START_AND_CLAIM", 0x0223),
    ("FLOW.RUN_STEPS_MANY", 0x0224),
    ("FLOW.SCHEDULE.CREATE", 0x0225),
    ("FLOW.SCHEDULE.GET", 0x0226),
    ("FLOW.SCHEDULE.DELETE", 0x0227),
    ("FLOW.SCHEDULE.FIRE_DUE", 0x0228),
    ("FLOW.SCHEDULE.LIST", 0x0229),
    ("FLOW.SCHEDULE.FIRE", 0x022A),
    ("FLOW.SCHEDULE.PAUSE", 0x022B),
    ("FLOW.SCHEDULE.RESUME", 0x022C),
    ("FLOW.EFFECT.RESERVE", 0x0240),
    ("FLOW.EFFECT.CONFIRM", 0x0241),
    ("FLOW.EFFECT.FAIL", 0x0242),
    ("FLOW.EFFECT.COMPENSATE", 0x0243),
    ("FLOW.EFFECT.GET", 0x0244),
    ("FLOW.GOVERNANCE.LEDGER", 0x0245),
    ("FLOW.APPROVAL.REQUEST", 0x0246),
    ("FLOW.APPROVAL.APPROVE", 0x0247),
    ("FLOW.APPROVAL.REJECT", 0x0248),
    ("FLOW.APPROVAL.GET", 0x0249),
    ("FLOW.CIRCUIT.OPEN", 0x024A),
    ("FLOW.CIRCUIT.CLOSE", 0x024B),
    ("FLOW.CIRCUIT.GET", 0x024C),
    ("FLOW.BUDGET.RESERVE", 0x024D),
    ("FLOW.BUDGET.GET", 0x024E),
    ("FLOW.LIMIT.LEASE", 0x024F),
    ("FLOW.LIMIT.SPEND", 0x0250),
    ("FLOW.LIMIT.RELEASE", 0x0251),
    ("FLOW.LIMIT.GET", 0x0252),
    ("FLOW.LIMIT.LIST", 0x0256),
    ("FLOW.BUDGET.LIST", 0x0255),
    ("FLOW.BUDGET.COMMIT", 0x0257),
    ("FLOW.BUDGET.RELEASE", 0x0258),
    ("FLOW.APPROVAL.LIST", 0x0253),
    ("FLOW.GOVERNANCE.OVERVIEW", 0x0254),
    ("FLOW.STATS", 0x022D),
    ("FLOW.ATTRIBUTES", 0x022E),
    ("FLOW.ATTRIBUTE_VALUES", 0x022F),
    ("FLOW.QUERY", 0x0231),
];

const OPCODES: [(Opcode, u16); 12] = [
    (Opcode::Hello, 0x0001),
    (Opcode::Shards, 0x0007),
    (Opcode::Ping, 0x0003),
    (Opcode::Quit, 0x0009),
    (Opcode::Options, 0x000B),
    (Opcode::Startup, 0x000C),
    (Opcode::Pipeline, 0x000E),
    (Opcode::CommandExec, 0x0100),
    (Opcode::FlowClaimDue, 0x0203),
    (Opcode::FlowCreateMany, 0x020F),
    (Opcode::FlowCompleteMany, 0x0210),
    (Opcode::FlowReclaim, 0x0215),
];

impl Opcode {
    /// The native protocol code.
    #[must_use]
    pub fn code(self) -> u16 {
        if let Self::Flow(name) | Self::Cluster(name) = self {
            return FLOW_COMMANDS
                .iter()
                .chain(&CLUSTER_COMMANDS)
                .find(|(known, _)| *known == name)
                .map_or(0, |(_, code)| *code);
        }
        OPCODES
            .iter()
            .find(|(opcode, _)| *opcode == self)
            .map_or(0, |(_, code)| *code)
    }

    /// The implemented opcode for `code`, if any.
    #[must_use]
    pub fn from_code(code: u16) -> Option<Self> {
        OPCODES
            .iter()
            .find(|(_, known)| *known == code)
            .map(|(opcode, _)| *opcode)
            .or_else(|| {
                FLOW_COMMANDS
                    .iter()
                    .find(|(_, known)| *known == code)
                    .map(|(name, _)| Self::Flow(name))
            })
            .or_else(|| {
                CLUSTER_COMMANDS
                    .iter()
                    .find(|(_, known)| *known == code)
                    .map(|(name, _)| Self::Cluster(name))
            })
    }
}

/// A parsed request header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Flag bits.
    pub flags: u8,
    /// Lane; replies on one lane keep request order.
    pub lane: u32,
    /// Raw opcode, implemented or not.
    pub opcode: u16,
    /// Connection-local correlation number echoed in the reply.
    pub request_id: u64,
    /// Body length in bytes, at most [`MAX_BODY_BYTES`].
    pub body_len: u32,
}

/// A header that cannot start a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// The first four bytes are not `FSNP`.
    BadMagic,
    /// The version byte is not the request version.
    BadVersion(u8),
    /// The body is longer than [`MAX_BODY_BYTES`].
    TooLarge(u64),
}

impl fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => formatter.write_str("frame magic is not FSNP"),
            Self::BadVersion(version) => {
                write!(formatter, "frame version {version:#04x} is not a request")
            }
            Self::TooLarge(length) => {
                write!(formatter, "frame body of {length} bytes exceeds the limit")
            }
        }
    }
}

impl std::error::Error for FrameError {}

impl Header {
    /// Parses the 24-byte header of a request frame.
    ///
    /// # Errors
    /// Refuses another magic, a non-request version or an oversized body.
    pub fn parse_request(bytes: [u8; HEADER_LEN]) -> Result<Self, FrameError> {
        let [
            m0,
            m1,
            m2,
            m3,
            version,
            flags,
            l0,
            l1,
            l2,
            l3,
            o0,
            o1,
            r0,
            r1,
            r2,
            r3,
            r4,
            r5,
            r6,
            r7,
            b0,
            b1,
            b2,
            b3,
        ] = bytes;
        if [m0, m1, m2, m3] != MAGIC {
            return Err(FrameError::BadMagic);
        }
        if version != REQUEST_VERSION {
            return Err(FrameError::BadVersion(version));
        }
        let body_len = u32::from_be_bytes([b0, b1, b2, b3]);
        if body_len > MAX_BODY_BYTES {
            return Err(FrameError::TooLarge(u64::from(body_len)));
        }
        Ok(Self {
            flags,
            lane: u32::from_be_bytes([l0, l1, l2, l3]),
            opcode: u16::from_be_bytes([o0, o1]),
            request_id: u64::from_be_bytes([r0, r1, r2, r3, r4, r5, r6, r7]),
            body_len,
        })
    }

    /// Appends the reply frame for this request: the same lane, opcode and
    /// request id, the response version, `flags` and `body`.
    ///
    /// # Errors
    /// Refuses a body longer than [`MAX_BODY_BYTES`]; nothing is appended.
    pub fn encode_response(
        &self,
        flags: u8,
        body: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<(), FrameError> {
        let body_len = u32::try_from(body.len())
            .ok()
            .filter(|length| *length <= MAX_BODY_BYTES)
            .ok_or_else(|| FrameError::TooLarge(u64::try_from(body.len()).unwrap_or(u64::MAX)))?;
        out.reserve(HEADER_LEN.saturating_add(body.len()));
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&[RESPONSE_VERSION, flags]);
        out.extend_from_slice(&self.lane.to_be_bytes());
        out.extend_from_slice(&self.opcode.to_be_bytes());
        out.extend_from_slice(&self.request_id.to_be_bytes());
        out.extend_from_slice(&body_len.to_be_bytes());
        out.extend_from_slice(body);
        Ok(())
    }
}
