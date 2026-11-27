use std::collections::BTreeSet;

use openraft::impls::{Entry, EntryPayload, Membership, Vote};
use openraft::raft::{AppendEntriesRequest, AppendError, VoteRequest, VoteResponse};
use openraft::type_config::alias::SnapshotOf;
use openraft::{Snapshot, SnapshotMeta, StoredMembership};
use wf_ports::consensus::log::EntryId;

use super::super::{Proposal, codec};
use super::{
    Held, Recipient, Request, Response, decode_request, decode_response, encode_request,
    encode_response, group_id,
};

fn id(term: u64, index: u64) -> EntryId {
    EntryId {
        term,
        node: 2,
        index,
    }
}

/// A snapshot at index 9 of a three-member group, its membership from 4.
fn taken() -> SnapshotOf<super::Config, Vec<u8>> {
    let membership = Membership::new_with_defaults(vec![BTreeSet::from([1, 2, 3])], []);
    Snapshot {
        meta: SnapshotMeta {
            last_log_id: Some(codec::log_id(id(3, 9))),
            last_membership: StoredMembership::new(Some(codec::log_id(id(2, 4))), membership),
        },
        snapshot: b"image".to_vec(),
    }
}

fn same_snapshot(
    sent: &SnapshotOf<super::Config, Vec<u8>>,
    read: &SnapshotOf<super::Config, Vec<u8>>,
) -> bool {
    sent.meta == read.meta && sent.snapshot == read.snapshot
}

#[test]
fn requests_read_back_as_sent() {
    let membership = Membership::new_with_defaults(
        vec![BTreeSet::from([1, 2, 3])],
        BTreeSet::from([1, 2, 3, 4]),
    );
    let append = AppendEntriesRequest {
        vote: Vote::new_committed(3, 2),
        prev_log_id: Some(codec::log_id(id(2, 4))),
        entries: vec![
            Entry {
                log_id: codec::log_id(id(3, 5)),
                payload: EntryPayload::Blank,
            },
            Entry {
                log_id: codec::log_id(id(3, 6)),
                payload: EntryPayload::Normal(Proposal(b"round".to_vec())),
            },
            Entry {
                log_id: codec::log_id(id(3, 7)),
                payload: EntryPayload::Membership(membership),
            },
        ],
        leader_commit: None,
    };
    let vote = VoteRequest::new(Vote::new(4, 1), Some(codec::log_id(id(3, 7))));
    for request in [
        Request::Append(append),
        Request::Vote(vote.clone()),
        Request::PreVote(vote),
        Request::Snapshot(Vote::new_committed(3, 2), taken()),
        Request::Newest { after: 4, upto: 9 },
    ] {
        let bytes = encode_request(RECIPIENT, &request).unwrap_or_default();
        let (recipient, read) = decode_request(&bytes).map_or((None, None), |(recipient, read)| {
            (Some(recipient), Some(read))
        });
        assert_eq!(recipient, Some(RECIPIENT));
        let same = match (&request, &read) {
            (Request::Append(sent), Some(Request::Append(read))) => {
                format!("{sent:?}") == format!("{read:?}")
            }
            (Request::Vote(sent), Some(Request::Vote(read)))
            | (Request::PreVote(sent), Some(Request::PreVote(read))) => sent == read,
            (Request::Snapshot(vote, sent), Some(Request::Snapshot(read_vote, read))) => {
                vote == read_vote && same_snapshot(sent, read)
            }
            (
                Request::Newest { after, upto },
                Some(Request::Newest {
                    after: read_after,
                    upto: read_upto,
                }),
            ) => (after, upto) == (read_after, read_upto),
            _ => false,
        };
        assert!(same, "{bytes:?}");
    }
    assert!(decode_request(&[9]).is_err());
    let mut status = 7_u64.to_be_bytes().to_vec();
    status.extend_from_slice(&3_u64.to_be_bytes());
    status.push(6);
    assert!(matches!(
        decode_request(&status),
        Ok((
            Recipient {
                group: 7,
                member: 3
            },
            Request::Status
        ))
    ));
}

const RECIPIENT: Recipient = Recipient {
    group: 0x0123_4567_89ab_cdef,
    member: 2,
};

/// A group's id is the digest of its member list: the same list gives the
/// same id, and another id, address or member gives another.
#[test]
fn a_group_is_known_by_its_member_list() {
    let list = |entries: &[(u64, &str)]| {
        entries
            .iter()
            .map(|(id, address)| (*id, (*address).to_owned()))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let group = list(&[(1, "a:1"), (2, "b:2"), (3, "c:3")]);
    // The first eight bytes of the SHA-256 of each id, address length and
    // address: 1, 3, "a:1", 2, 3, "b:2", 3, 3, "c:3" (u64s big-endian).
    assert_eq!(group_id(&group), 0x83dd_368e_ce33_933b);
    assert_eq!(group_id(&group), group_id(&group.clone()));
    for other in [
        list(&[(1, "a:1"), (2, "b:2"), (3, "c:4")]),
        list(&[(1, "a:1"), (2, "b:2"), (4, "c:3")]),
        list(&[(1, "a:1"), (2, "b:2")]),
        // The length keeps an address's bytes from running into the next id.
        list(&[(1, "a:1"), (2, "b:2c:3")]),
    ] {
        assert_ne!(group_id(&group), group_id(&other), "{other:?}");
    }
}

#[test]
fn responses_read_back_as_sent() {
    let responses = [
        Response::Append(Ok(Some(codec::log_id(id(3, 7))))),
        Response::Append(Ok(None)),
        Response::Append(Err(AppendError::Conflict {
            log_id: codec::log_id(id(3, 7)),
            hint: None,
        })),
        Response::Append(Err(AppendError::HigherVote(Vote::new(5, 3)))),
        Response::Vote(VoteResponse::new(Vote::new(5, 3), None, true)),
        Response::Failed("stopped".to_owned()),
        Response::Status(Held {
            last_log: Some(7),
            term: 5,
        }),
        Response::Status(Held {
            last_log: Some(0),
            term: 0,
        }),
        Response::Status(Held {
            last_log: None,
            term: 2,
        }),
        Response::Newest(Some(taken())),
        Response::Newest(None),
    ];
    for response in responses {
        let bytes = encode_response(&response).unwrap_or_default();
        let read = decode_response(&bytes).ok();
        let same = match (&response, &read) {
            (Response::Append(sent), Some(Response::Append(read))) => sent == read,
            (Response::Vote(sent), Some(Response::Vote(read))) => sent == read,
            (Response::Failed(sent), Some(Response::Failed(read))) => sent == read,
            (Response::Status(sent), Some(Response::Status(read))) => sent == read,
            (Response::Newest(Some(sent)), Some(Response::Newest(Some(read)))) => {
                same_snapshot(sent, read)
            }
            (Response::Newest(None), Some(Response::Newest(None))) => true,
            _ => false,
        };
        assert!(same, "{bytes:?}");
    }
    // A status names whether a log is held as 0 or 1, as a newest snapshot
    // answer names whether one follows.
    assert!(decode_response(&[6, 2, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
    assert!(decode_response(&[7, 2]).is_err());
}
