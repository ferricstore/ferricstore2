use super::*;
#[derive(Default)]
pub(super) struct CaptureInput {
    pub bytes: Vec<u8>,
    pub nodes: Vec<Node>,
    pub edges: Vec<usize>,
    pub root: usize,
}
impl CaptureInput {
    pub fn push(&mut self, node: Node) -> usize {
        let id = self.nodes.len();
        self.nodes.push(node);
        id
    }
    pub fn binary(&mut self, bytes: &[u8]) -> usize {
        let start = self.bytes.len();
        self.bytes.extend_from_slice(bytes);
        self.push(Node::Binary {
            bytes: start..self.bytes.len(),
        })
    }
    pub fn atom(&mut self, bytes: &[u8]) -> usize {
        let start = self.bytes.len();
        self.bytes.extend_from_slice(bytes);
        self.push(Node::Atom {
            bytes: start..self.bytes.len(),
            utf8: true,
        })
    }
    pub fn map(&mut self, pairs: &[(usize, usize)]) -> usize {
        let start = self.edges.len();
        for &(key, value) in pairs {
            self.edges.extend([key, value]);
        }
        self.push(Node::Map {
            entries: start..self.edges.len(),
        })
    }
    pub fn tuple(&mut self, children: &[usize]) -> usize {
        let start = self.edges.len();
        self.edges.extend_from_slice(children);
        self.push(Node::Tuple {
            children: start..self.edges.len(),
        })
    }
    pub fn list(&mut self, values: &[usize]) -> usize {
        if values.is_empty() {
            return self.push(Node::EmptyList);
        }
        let tail = self.push(Node::EmptyList);
        let start = self.edges.len();
        self.edges.extend_from_slice(values);
        self.push(Node::List {
            heads: start..self.edges.len(),
            tail,
        })
    }
    pub fn envelope(&mut self, pairs: &[(usize, usize)], version: i64) {
        let tag = self.binary(b"wf.record.metadata");
        let version = self.push(Node::Integer(version));
        let map = self.map(pairs);
        self.root = self.tuple(&[tag, version, map]);
    }
    pub fn input(&self) -> OriginalValueInput<'_> {
        OriginalValueInput {
            bytes: &self.bytes,
            nodes: &self.nodes,
            edges: &self.edges,
            root: self.root,
        }
    }
}
pub(super) fn encode_limits() -> EncodeLimits {
    EncodeLimits {
        max_input_bytes: 1_048_576,
        max_nodes: 10000,
        max_edges: 20000,
        max_depth: 64,
        max_encoded_bytes: 2_097_152,
        max_work_units: 128 * 1_048_576,
        max_temporary_bytes: 32 * 1_048_576,
    }
}
pub(super) fn limits() -> Limits {
    Limits {
        max_encoded_bytes: 2_097_152,
        max_nodes: 10000,
        max_edges: 20000,
        max_depth: 64,
        max_work_units: 128 * 1_048_576,
    }
}
pub(super) fn context() -> Context<'static> {
    Context {
        safe_atoms: Some(&["nil", "true", "false"]),
        active_exports: Some(&[]),
        local_node: None,
        local_functions: None,
    }
}
pub(super) fn empty() -> CaptureInput {
    let mut input = CaptureInput::default();
    input.envelope(&[], 1);
    input
}

pub(super) fn wire(version: u8, entries: &[(&[u8], u8)]) -> TestResult<Vec<u8>> {
    let mut bytes = b"FSV2\x02\x83\x68\x03".to_vec();
    let tag = b"wf.record.metadata";
    bytes.push(109);
    bytes.extend_from_slice(&u32::try_from(tag.len())?.to_be_bytes());
    bytes.extend_from_slice(tag);
    bytes.extend_from_slice(&[97, version, 116]);
    bytes.extend_from_slice(&u32::try_from(entries.len())?.to_be_bytes());
    for (key, value) in entries {
        bytes.push(109);
        bytes.extend_from_slice(&u32::try_from(key.len())?.to_be_bytes());
        bytes.extend_from_slice(key);
        bytes.extend_from_slice(&[97, *value]);
    }
    Ok(bytes)
}

pub(super) fn is_limit(error: &E) -> bool {
    matches!(
        error,
        E::Limit
            | E::Encoding(wf_domain::value::OriginalValueEncodeError::Limit)
            | E::Decoding(wf_domain::value::OriginalFlowValueError::Limit)
    )
}
