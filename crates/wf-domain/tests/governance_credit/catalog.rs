use super::TestResult;
mod ack;
use wf_domain::governance::catalog::{
    CreditCatalogError as E, CreditCatalogMeta as Meta, CreditCatalogObservation as Observation,
};
use wf_domain::workflow::MAX_EXACT_MILLIS as MAX;

#[test]
fn catalog_append_preserves_monotone_original_sequences_and_binary_owner_bounds() -> TestResult {
    let (first, sequence) = Meta::empty().append(b"\0owner\xff", 8)?;
    assert_eq!((first.head(), first.tail(), sequence), (1, 1, 1));
    let (second, sequence) = first.append(b"other", 8)?;
    assert_eq!((second.head(), second.tail(), sequence), (1, 2, 2));
    assert_eq!(second.append(b"", 8), Err(E::CorruptMetadata));
    assert_eq!(second.append(b"123456789", 8), Err(E::CorruptMetadata));
    assert!(second.append(b"12345678", 8).is_ok());
    assert_eq!(second.append(b"owner", 0), Err(E::CorruptMetadata));
    Ok(())
}

#[test]
fn catalog_headers_preserve_the_last_exact_sequence_and_maximum_ack_head() -> TestResult {
    assert_eq!(Meta::restore(1, 0)?, Meta::empty());
    assert_eq!(
        (
            Meta::restore(MAX + 1, MAX)?.head(),
            Meta::restore(MAX + 1, MAX)?.tail()
        ),
        (MAX + 1, MAX)
    );
    for (head, tail) in [
        (0, 0),
        (3, 1),
        (MAX + 2, MAX),
        (1, MAX + 1),
        (u64::MAX, u64::MAX),
    ] {
        assert_eq!(Meta::restore(head, tail), Err(E::CorruptMetadata));
    }
    let before_last = Meta::restore(MAX, MAX - 1)?;
    let (last, sequence) = before_last.append(b"scope", 32)?;
    assert_eq!(sequence, MAX);
    let ack = last.acknowledge(MAX, MAX)?;
    assert_eq!(ack.sequences().collect::<Vec<_>>(), [MAX]);
    assert_eq!((ack.next().head(), ack.next().tail()), (MAX + 1, MAX));
    assert_eq!(ack.next().append(b"scope", 32), Err(E::SequenceExhausted));
    Ok(())
}

#[test]
fn catalog_ack_retries_precede_stale_head_checks_without_rewinding_the_tail() -> TestResult {
    let meta = Meta::restore(10, 20)?;
    let ack = meta.acknowledge(10, 12)?;
    assert_eq!(
        (ack.len(), ack.next().head(), ack.next().tail()),
        (3, 13, 20)
    );
    assert_eq!(ack.sequences().collect::<Vec<_>>(), [10, 11, 12]);
    let retry = ack.next().acknowledge(1, 12)?;
    assert!(retry.is_empty());
    assert_eq!(retry.sequences().count(), 0);
    assert_eq!(retry.next(), ack.next());
    assert_eq!(ack.next().acknowledge(10, 13), Err(E::StaleCursor));
    let all = ack.next().acknowledge(13, 20)?;
    assert_eq!((all.next().head(), all.next().tail()), (21, 20));
    let (next, sequence) = all.next().append(b"next", 8)?;
    assert_eq!((next.head(), next.tail(), sequence), (21, 21, 21));
    Ok(())
}

#[test]
fn catalog_ack_has_exact_256_entry_and_valid_scalar_boundaries() -> TestResult {
    let meta = Meta::restore(1, 300)?;
    let ack = meta.acknowledge(1, 256)?;
    assert_eq!((ack.len(), ack.sequences().count()), (256, 256));
    assert_eq!(ack.sequences().last(), Some(256));
    assert_eq!(meta.acknowledge(1, 257), Err(E::InvalidAcknowledgement));
    assert_eq!(meta.acknowledge(1, 301), Err(E::InvalidAcknowledgement));
    for (head, end) in [(0, 1), (MAX + 2, 1), (1, 0), (1, MAX + 1)] {
        assert_eq!(meta.acknowledge(head, end), Err(E::CorruptMetadata));
    }
    Ok(())
}

#[test]
fn catalog_pending_bound_has_source_priority_over_sequence_exhaustion() -> TestResult {
    let (last_room, sequence) = Meta::restore(1, 65_535)?.append(b"owner", 32)?;
    assert_eq!(sequence, 65_536);
    assert_eq!(last_room.append(b"owner", 32), Err(E::BacklogFull));
    assert_eq!(
        Meta::restore(MAX - 65_535, MAX)?.append(b"owner", 32),
        Err(E::BacklogFull)
    );
    assert_eq!(
        Meta::restore(MAX + 1, MAX)?.append(b"owner", 32),
        Err(E::SequenceExhausted)
    );
    assert_eq!(last_room.append(b"", 32), Err(E::CorruptMetadata));
    Ok(())
}

#[test]
fn catalog_pending_tail_reuse_precedes_backlog_but_requires_the_exact_point() -> TestResult {
    let meta = Meta::restore(1, 65_536)?;
    assert_eq!(
        Observation {
            meta,
            tail_scope: Some(b"owner")
        }
        .publication(b"owner", 32)?,
        None
    );
    assert_eq!(
        Observation {
            meta,
            tail_scope: Some(b"other")
        }
        .publication(b"owner", 32),
        Err(E::BacklogFull)
    );
    assert_eq!(
        Observation {
            meta,
            tail_scope: None
        }
        .publication(b"owner", 32),
        Err(E::CorruptEntry)
    );
    assert_eq!(
        Observation {
            meta,
            tail_scope: Some(b"")
        }
        .publication(b"owner", 32),
        Err(E::CorruptEntry)
    );
    let empty = Observation {
        meta: Meta::empty(),
        tail_scope: None,
    };
    assert_eq!(empty.publication(b"owner", 32)?, Some(Meta::restore(1, 1)?));
    assert_eq!(
        Observation {
            meta: Meta::empty(),
            tail_scope: Some(b"owner")
        }
        .publication(b"owner", 32),
        Err(E::CorruptEntry)
    );
    Ok(())
}
