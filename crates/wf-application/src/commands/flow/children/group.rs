//! A parent's child group: its policies, its children's statuses and
//! results, Elixir's resolution rules and the reply form.

use std::collections::BTreeMap;

use super::super::term::{Term, map};

/// How many children a group waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// Every child.
    All,
    /// The first child.
    Any,
    /// None: the group is resolved when spawned.
    None,
}

impl Wait {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Any => "any",
            Self::None => "none",
        }
    }
}

/// A finished child's status and its result and error references.
pub type ChildResult = (Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>);

/// One child group of a parent Flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildGroup {
    /// What the group waits for.
    pub wait: Wait,
    /// A failed child fails the group (`fail_parent`) or is counted
    /// (`ignore`).
    pub fail_parent: bool,
    /// Closing the parent cancels running children (`cancel_children`) or
    /// leaves them (`abandon_children`).
    pub cancel_children: bool,
    /// Parent states on success and failure.
    pub success: Vec<u8>,
    /// Parent state on failure.
    pub failure: Vec<u8>,
    /// Hash of the spawn request, for idempotent retries.
    pub request_hash: Vec<u8>,
    /// Child id to status: `running`, then a terminal state.
    pub children: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Child id to partition.
    pub child_partitions: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Finished children: status and result and error references.
    pub results: BTreeMap<Vec<u8>, ChildResult>,
    /// `success` or `failure` once resolved.
    pub resolved: Option<Vec<u8>>,
}

impl ChildGroup {
    fn count(&self, status: &[u8]) -> u64 {
        u64::try_from(
            self.children
                .values()
                .filter(|value| value.as_slice() == status)
                .count(),
        )
        .unwrap_or(u64::MAX)
    }

    /// Whether `child` is a running member of this unresolved group.
    #[must_use]
    pub fn waits_for(&self, child: &[u8]) -> bool {
        self.resolved.is_none()
            && self
                .children
                .get(child)
                .is_some_and(|status| status == b"running")
    }

    /// Records a finished child and resolves the group when it is done.
    pub fn finish(
        &mut self,
        child: &[u8],
        status: &[u8],
        refs: (Option<Vec<u8>>, Option<Vec<u8>>),
    ) {
        self.children.insert(child.to_vec(), status.to_vec());
        self.results
            .insert(child.to_vec(), (status.to_vec(), refs.0, refs.1));
        let total = u64::try_from(self.children.len()).unwrap_or(u64::MAX);
        let completed = self.count(b"completed");
        let terminal = completed
            .saturating_add(self.count(b"failed"))
            .saturating_add(self.count(b"cancelled"));
        let failed = status != b"completed";
        self.resolved = match (self.wait, failed) {
            (Wait::Any, false) => Some(b"success".to_vec()),
            (_, true) if self.fail_parent => Some(b"failure".to_vec()),
            (Wait::Any, true) if terminal >= total => Some(b"failure".to_vec()),
            (Wait::Any, true) => None,
            _ if terminal >= total => Some(b"success".to_vec()),
            _ => None,
        };
    }

    /// The parent state the resolution leads to, if resolved.
    #[must_use]
    pub fn target(&self) -> Option<&[u8]> {
        match self.resolved.as_deref() {
            Some(b"success") => Some(&self.success),
            Some(b"failure") => Some(&self.failure),
            _ => None,
        }
    }

    /// The group as the record reply shows it.
    #[must_use]
    pub fn term(&self) -> Term {
        let texts = |entries: &BTreeMap<Vec<u8>, Vec<u8>>| {
            Term::Map(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), Term::Bin(value.clone())))
                    .collect(),
            )
        };
        let results = self
            .results
            .iter()
            .map(|(id, (status, result, error))| {
                let mut entries = Vec::new();
                if let Some(error) = error {
                    entries.push(("error_ref", Term::Bin(error.clone())));
                }
                if let Some(result) = result {
                    entries.push(("result_ref", Term::Bin(result.clone())));
                }
                entries.push(("status", Term::Bin(status.clone())));
                (id.clone(), map(entries))
            })
            .collect();
        map(vec![
            ("child_partitions", texts(&self.child_partitions)),
            ("children", texts(&self.children)),
            (
                "exhaust_to",
                map(vec![
                    ("failure", Term::Bin(self.failure.clone())),
                    ("success", Term::Bin(self.success.clone())),
                ]),
            ),
            (
                "on_child_failed",
                Term::text(if self.fail_parent {
                    "fail_parent"
                } else {
                    "ignore"
                }),
            ),
            (
                "on_parent_closed",
                Term::text(if self.cancel_children {
                    "cancel_children"
                } else {
                    "abandon_children"
                }),
            ),
            ("request_hash", Term::Bin(self.request_hash.clone())),
            ("resolved", Term::optional_bin(self.resolved.as_deref())),
            ("results", Term::Map(results)),
            (
                "summary",
                map(vec![
                    ("cancelled", Term::int(self.count(b"cancelled"))),
                    ("completed", Term::int(self.count(b"completed"))),
                    ("failed", Term::int(self.count(b"failed"))),
                    (
                        "total",
                        Term::int(u64::try_from(self.children.len()).unwrap_or(u64::MAX)),
                    ),
                ]),
            ),
            ("wait", Term::text(self.wait.as_str())),
        ])
    }
}

fn texts_of(term: Option<&Term>) -> Option<BTreeMap<Vec<u8>, Vec<u8>>> {
    match term? {
        Term::Map(entries) => entries
            .iter()
            .map(|(key, value)| match value {
                Term::Bin(bytes) => Some((key.clone(), bytes.clone())),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

fn bin_of(term: Option<&Term>) -> Option<Vec<u8>> {
    match term? {
        Term::Bin(bytes) => Some(bytes.clone()),
        _ => None,
    }
}

impl ChildGroup {
    /// Reads a group back from [`ChildGroup::term`].
    #[must_use]
    pub fn from_term(term: &Term) -> Option<Self> {
        let wait = match bin_of(term.get(b"wait"))?.as_slice() {
            b"any" => Wait::Any,
            b"none" => Wait::None,
            _ => Wait::All,
        };
        let exhaust = term.get(b"exhaust_to")?;
        let results = match term.get(b"results")? {
            Term::Map(entries) => entries
                .iter()
                .map(|(id, result)| {
                    Some((
                        id.clone(),
                        (
                            bin_of(result.get(b"status"))?,
                            bin_of(result.get(b"result_ref")),
                            bin_of(result.get(b"error_ref")),
                        ),
                    ))
                })
                .collect::<Option<BTreeMap<_, _>>>()?,
            _ => return None,
        };
        Some(Self {
            wait,
            fail_parent: bin_of(term.get(b"on_child_failed"))?.as_slice() == b"fail_parent",
            cancel_children: bin_of(term.get(b"on_parent_closed"))?.as_slice()
                == b"cancel_children",
            success: bin_of(exhaust.get(b"success"))?,
            failure: bin_of(exhaust.get(b"failure"))?,
            request_hash: bin_of(term.get(b"request_hash"))?,
            children: texts_of(term.get(b"children"))?,
            child_partitions: texts_of(term.get(b"child_partitions"))?,
            results,
            resolved: bin_of(term.get(b"resolved")),
        })
    }
}
