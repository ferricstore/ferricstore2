//! Independent source-default and ordered-alias observations.

use super::TestResult;
use wf_domain::claim::caller::{
    ClaimCallerLimits, ClaimDueInput, ClaimPartitionMode, ClaimPartitionsInput, ClaimReplyMode,
    ClaimStatesInput, ClaimValuesInput, ClaimValuesMode, normalize_claim_due,
};

#[test]
fn fixed_defaults_match_explicit_caller_defaults() -> TestResult {
    let defaults = normalize_claim_due(
        ClaimDueInput::new(b"job", b"worker"),
        ClaimCallerLimits::default(),
    )?;
    let explicit = normalize_claim_due(
        ClaimDueInput {
            lease_ms: Some(30_000),
            limit: Some(1),
            reclaim_expired: Some(true),
            reclaim_ratio: Some(25),
            reply_mode: Some(ClaimReplyMode::Records),
            full: Some(true),
            payload: Some(true),
            ..ClaimDueInput::new(b"job", b"worker")
        },
        ClaimCallerLimits::default(),
    )?;
    for value in [&defaults, &explicit] {
        assert_eq!(value.workflow_type(), b"job");
        assert_eq!(value.worker(), b"worker");
        assert_eq!(value.states(), (true, [].as_slice()));
        assert_eq!(
            value.partitions(),
            (ClaimPartitionMode::Auto, [].as_slice())
        );
        assert_eq!(value.values(), (ClaimValuesMode::Omitted, [].as_slice()));
        assert_eq!(value.lease_ms(), 30_000);
        assert_eq!(value.limit(), 1);
        assert_eq!(value.priority(), None);
        assert!(value.reclaim_expired());
        assert_eq!(value.reclaim_ratio(), 25);
        assert_eq!(value.reply_mode(), ClaimReplyMode::Records);
        assert!(value.payload_enabled());
        assert_eq!(value.payload_max_bytes(), None);
        assert_eq!(value.governance_limit_scope(), None);
        assert_eq!(value.governance_shard_id(), None);
        assert_eq!(value.requested_now_ms(), None);
        assert_eq!(value.block_ms(), None);
    }
    Ok(())
}

#[test]
fn states_keep_last_and_preserve_any_empty_mixed() -> TestResult {
    let states = [b"queued".as_slice(), b"ready", b"queued"];
    let value = normalize_claim_due(
        ClaimDueInput {
            states: ClaimStatesInput::Many(&states),
            ..ClaimDueInput::new(b"job", b"worker")
        },
        ClaimCallerLimits::default(),
    )?;
    assert_eq!(
        value.states(),
        (false, [b"ready".as_slice(), b"queued"].as_slice())
    );
    let alias = [b"aNy".as_slice()];
    for states in [
        ClaimStatesInput::Any,
        ClaimStatesInput::One(b"ANY"),
        ClaimStatesInput::Many(&alias),
    ] {
        let value = normalize_claim_due(
            ClaimDueInput {
                states,
                ..ClaimDueInput::new(b"job", b"worker")
            },
            ClaimCallerLimits::default(),
        )?;
        assert_eq!(value.states(), (true, [].as_slice()));
    }
    let mixed = [b"Any".as_slice(), b"ready", b"ANY"];
    let empty_binary = [b"".as_slice()];
    for (states, any, expected) in [
        (
            ClaimStatesInput::Many(&mixed),
            true,
            [b"ready".as_slice()].as_slice(),
        ),
        (ClaimStatesInput::Many(&[]), false, [].as_slice()),
        (ClaimStatesInput::One(b""), false, empty_binary.as_slice()),
    ] {
        let value = normalize_claim_due(
            ClaimDueInput {
                states,
                ..ClaimDueInput::new(b"", b"")
            },
            ClaimCallerLimits::default(),
        )?;
        assert_eq!(value.states(), (any, expected));
    }
    Ok(())
}

#[test]
fn partition_aliases_preserve_auto_any_global_and_explicit_lists() -> TestResult {
    for (partitions, mode) in [
        (ClaimPartitionsInput::Auto, ClaimPartitionMode::Auto),
        (ClaimPartitionsInput::Any, ClaimPartitionMode::Any),
        (ClaimPartitionsInput::Global, ClaimPartitionMode::Global),
        (ClaimPartitionsInput::One(b"aUtO"), ClaimPartitionMode::Auto),
        (ClaimPartitionsInput::One(b"AnY"), ClaimPartitionMode::Any),
        (
            ClaimPartitionsInput::One(b"gLoBaL"),
            ClaimPartitionMode::Global,
        ),
    ] {
        let value = normalize_claim_due(
            ClaimDueInput {
                partitions,
                ..ClaimDueInput::new(b"job", b"worker")
            },
            ClaimCallerLimits::default(),
        )?;
        assert_eq!(value.partitions(), (mode, [].as_slice()));
    }
    let keys = [b"p1".as_slice(), b"p2", b"p1"];
    let explicit_alias = [b"ANY".as_slice()];
    let empty = [b"".as_slice()];
    let opaque = [b"\xff\0key".as_slice()];
    for (partitions, expected) in [
        (
            ClaimPartitionsInput::Many(&keys),
            [b"p1".as_slice(), b"p2"].as_slice(),
        ),
        (
            ClaimPartitionsInput::Many(&explicit_alias),
            explicit_alias.as_slice(),
        ),
        (ClaimPartitionsInput::Many(&[]), [].as_slice()),
        (ClaimPartitionsInput::One(b""), empty.as_slice()),
        (ClaimPartitionsInput::One(opaque[0]), opaque.as_slice()),
    ] {
        let value = normalize_claim_due(
            ClaimDueInput {
                partitions,
                ..ClaimDueInput::new(b"job", b"worker")
            },
            ClaimCallerLimits::default(),
        )?;
        assert_eq!(value.partitions(), (ClaimPartitionMode::Only, expected));
    }
    Ok(())
}

#[test]
fn values_keep_first_and_distinguish_omitted_all_empty() -> TestResult {
    for (values, mode) in [
        (ClaimValuesInput::Omitted, ClaimValuesMode::Omitted),
        (ClaimValuesInput::All, ClaimValuesMode::All),
        (ClaimValuesInput::Names(&[]), ClaimValuesMode::Only),
    ] {
        let value = normalize_claim_due(
            ClaimDueInput {
                values,
                ..ClaimDueInput::new(b"job", b"worker")
            },
            ClaimCallerLimits::default(),
        )?;
        assert_eq!(value.values(), (mode, [].as_slice()));
    }
    let names = [b"first".as_slice(), b"\0\xff", b"first", b""];
    let value = normalize_claim_due(
        ClaimDueInput {
            values: ClaimValuesInput::Names(&names),
            ..ClaimDueInput::new(b"job", b"worker")
        },
        ClaimCallerLimits::default(),
    )?;
    assert_eq!(
        value.values(),
        (
            ClaimValuesMode::Only,
            [b"first".as_slice(), b"\0\xff", b""].as_slice()
        )
    );
    Ok(())
}

#[test]
fn payload_defaults_follow_reply_full_and_override() -> TestResult {
    for (reply_mode, default_payload) in [
        (ClaimReplyMode::Records, true),
        (ClaimReplyMode::Jobs, false),
        (ClaimReplyMode::Compact, false),
        (ClaimReplyMode::CompactAttrs, false),
        (ClaimReplyMode::CompactState, false),
        (ClaimReplyMode::CompactStateAttrs, false),
    ] {
        for (full, payload, expected) in [
            (None, None, default_payload),
            (Some(false), None, false),
            (Some(true), None, true),
            (None, Some(false), false),
            (None, Some(true), true),
            (Some(false), Some(true), true),
            (Some(true), Some(false), false),
            (Some(false), Some(false), false),
            (Some(true), Some(true), true),
        ] {
            let value = normalize_claim_due(
                ClaimDueInput {
                    reply_mode: Some(reply_mode),
                    full,
                    payload,
                    ..ClaimDueInput::new(b"job", b"worker")
                },
                ClaimCallerLimits::default(),
            )?;
            assert_eq!(value.reply_mode(), reply_mode);
            assert_eq!(value.payload_enabled(), expected);
            assert_eq!(value.payload_max_bytes(), None);
        }
    }
    for cap in [0, 65_536, u64::MAX] {
        let value = normalize_claim_due(
            ClaimDueInput {
                payload_max_bytes: Some(cap),
                ..ClaimDueInput::new(b"job", b"worker")
            },
            ClaimCallerLimits::default(),
        )?;
        assert_eq!(value.payload_max_bytes(), Some(cap));
    }
    Ok(())
}
