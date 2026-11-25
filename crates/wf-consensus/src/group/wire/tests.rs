use std::collections::BTreeSet;

use openraft::impls::{Entry, EntryPayload, Membership, Vote};
use openraft::raft::{AppendEntriesRequest, AppendError, VoteRequest, VoteResponse};
use wf_ports::consensus::log::EntryId;

use super::super::{Proposal, codec};
use super::{
    Held, Request, Response, decode_request, decode_response, encode_request, encode_response,
};

fn id(term: u64, index: u64) -> EntryId {
    EntryId {
        term,
        node: 2,
        index,
    }
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
    ] {
        let bytes = encode_request(&request).unwrap_or_default();
        let read = decode_request(&bytes).ok();
        let same = match (&request, &read) {
            (Request::Append(sent), Some(Request::Append(read))) => {
                format!("{sent:?}") == format!("{read:?}")
            }
            (Request::Vote(sent), Some(Request::Vote(read)))
            | (Request::PreVote(sent), Some(Request::PreVote(read))) => sent == read,
            _ => false,
        };
        assert!(same, "{bytes:?}");
    }
    assert!(decode_request(&[9]).is_err());
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
    ];
    for response in responses {
        let bytes = encode_response(&response).unwrap_or_default();
        let read = decode_response(&bytes).ok();
        let same = match (&response, &read) {
            (Response::Append(sent), Some(Response::Append(read))) => sent == read,
            (Response::Vote(sent), Some(Response::Vote(read))) => sent == read,
            (Response::Failed(sent), Some(Response::Failed(read))) => sent == read,
            (Response::Status(sent), Some(Response::Status(read))) => sent == read,
            _ => false,
        };
        assert!(same, "{bytes:?}");
    }
    // A status names whether a log is held as 0 or 1.
    assert!(decode_response(&[6, 2, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
}
