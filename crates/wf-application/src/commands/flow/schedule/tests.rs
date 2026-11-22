//! Business cases of Elixir's `schedule_test.exs` that need no node:
//! creation rules and the queued-overlap backoff.

use super::super::opts::MAX_EXACT;
use super::super::term::Term;
use super::create::definition;
use super::definition::Definition;
use super::plan::{Apply, Plan, plan};
use super::{Command, parse};

fn object(entries: Vec<(&str, Term)>) -> Term {
    Term::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value))
            .collect(),
    )
}

fn target(entries: Vec<(&str, Term)>) -> Term {
    let mut all = vec![("type", Term::text("scheduled"))];
    all.extend(entries);
    object(all)
}

fn create(entries: Vec<(&str, Term)>) -> Result<Definition, String> {
    let mut all = vec![("id", Term::text("s"))];
    all.extend(entries);
    let Ok(Command::Create(request)) = parse("FLOW.SCHEDULE.CREATE", &object(all)) else {
        return Err("not a create".to_owned());
    };
    definition(&request, 1_000).map_err(|error| error.to_string())
}

fn refused(entries: Vec<(&str, Term)>) -> String {
    create(entries).err().unwrap_or_default()
}

#[test]
fn timing_fields_belong_to_their_kind() {
    for (kind, extra, field, supported) in [
        (
            "one_shot",
            vec![("every_ms", Term::Int(1_000))],
            "every_ms",
            "interval",
        ),
        (
            "delay",
            vec![
                ("delay_ms", Term::Int(1_000)),
                ("cron", Term::text("* * * * *")),
            ],
            "cron",
            "cron",
        ),
        (
            "interval",
            vec![
                ("every_ms", Term::Int(1_000)),
                ("delay_ms", Term::Int(1_000)),
            ],
            "delay_ms",
            "delay",
        ),
        (
            "cron",
            vec![
                ("cron", Term::text("* * * * *")),
                ("every_ms", Term::Int(1_000)),
            ],
            "every_ms",
            "interval",
        ),
    ] {
        let mut entries = vec![("kind", Term::text(kind)), ("target", target(Vec::new()))];
        entries.extend(extra);
        assert_eq!(
            refused(entries),
            format!("ERR flow schedule {field} is only supported for {supported} schedules")
        );
    }
    assert_eq!(
        refused(vec![
            ("kind", Term::text("interval")),
            ("at_ms", Term::Int(1_000)),
            ("start_at_ms", Term::Int(2_000)),
            ("every_ms", Term::Int(100)),
            ("target", target(Vec::new())),
        ]),
        "ERR flow schedule cannot set both at_ms and start_at_ms"
    );
}

#[test]
fn targets_and_end_conditions_follow_the_kind() {
    let fixed = target(vec![("id", Term::text("x"))]);
    assert_eq!(
        refused(vec![
            ("every_ms", Term::Int(100)),
            ("target", fixed.clone())
        ]),
        "ERR recurring schedule target must use id_prefix, not id"
    );
    assert_eq!(
        refused(vec![(
            "target",
            target(vec![
                ("id", Term::text("x")),
                ("id_prefix", Term::text("y"))
            ])
        )]),
        "ERR flow schedule target cannot set both id and id_prefix"
    );
    assert_eq!(
        refused(vec![
            ("kind", Term::text("one_shot")),
            ("max_fires", Term::Int(1)),
            ("target", fixed.clone())
        ]),
        "ERR flow schedule max_fires is only supported for recurring schedules"
    );
    assert_eq!(
        refused(vec![
            ("delay_ms", Term::Int(100)),
            ("end_at_ms", Term::Int(1_000)),
            ("target", fixed)
        ]),
        "ERR flow schedule end_at_ms is only supported for recurring schedules"
    );
    assert_eq!(
        refused(vec![
            ("every_ms", Term::Int(100)),
            ("overlap_policy", Term::text("allow")),
            ("overlap_retry_ms", Term::Int(10)),
            ("target", target(Vec::new())),
        ]),
        "ERR flow schedule overlap_retry_ms requires overlap_policy :queue_after_previous"
    );
    assert_eq!(
        refused(vec![("at_ms", Term::Int(5))]),
        "ERR flow schedule target is required"
    );
    assert_eq!(
        refused(vec![("target", target(vec![("bogus", Term::Int(1))]))]),
        "ERR unsupported flow schedule target field bogus"
    );
    assert_eq!(
        refused(vec![(
            "target",
            target(vec![("id", Term::text("__ferricstore_schedule__:x"))])
        )]),
        "ERR scheduled target id is reserved for internal use"
    );
}

#[test]
fn delay_and_cron_schedules_start_from_their_rules() -> Result<(), String> {
    let delayed = create(vec![
        ("delay_ms", Term::Int(300)),
        ("target", target(Vec::new())),
    ])?;
    assert_eq!(
        (delayed.next_run_at_ms, delayed.delay_ms),
        (Some(1_300), Some(300))
    );
    let utc = create(vec![
        ("cron", Term::text("0 12 * * *")),
        ("timezone", Term::text("UTC")),
        ("target", target(Vec::new())),
    ])?;
    assert_eq!(utc.timezone.as_deref(), Some(b"Etc/UTC".as_slice()));
    assert_eq!(utc.next_run_at_ms, Some(43_200_000));
    assert_eq!(
        refused(vec![
            ("cron", Term::text("* * * * *")),
            ("timezone", Term::text("Mars/Base")),
            ("target", target(Vec::new()))
        ]),
        "ERR flow schedule timezone is invalid or unavailable"
    );
    assert_eq!(
        refused(vec![
            ("cron", Term::text("* * *")),
            ("target", target(Vec::new()))
        ]),
        "ERR flow schedule cron must have 5 fields"
    );
    Ok(())
}

#[test]
fn a_repeated_overlap_backs_off_on_the_same_occurrence() -> Result<(), String> {
    let mut schedule = create(vec![
        ("every_ms", Term::Int(100)),
        ("start_at_ms", Term::Int(4_500)),
        ("now_ms", Term::Int(4_500)),
        ("overlap_policy", Term::text("queue_after_previous")),
        ("overlap_retry_ms", Term::Int(50)),
        ("target", target(vec![("id_prefix", Term::text("t"))])),
    ])?;
    let Ok(Plan::Fire {
        then: Apply::Reschedule { definition, .. },
        ..
    }) = plan(schedule, None, 4_500, None)
    else {
        return Err("first fire did not reschedule".to_owned());
    };
    schedule = definition;
    let mut now = 4_600;
    for expected in [4_650, 4_750, 4_950] {
        let Ok(Plan::Hold {
            then: Apply::Reschedule { definition, run_at },
            ..
        }) = plan(schedule, None, now, Some(b"t:4500:1"))
        else {
            return Err("overlap did not queue".to_owned());
        };
        assert_eq!(run_at, expected);
        assert_eq!(definition.overlap_queued_due_at_ms, Some(4_600));
        assert_eq!(definition.fire_count, 1);
        schedule = definition;
        now = run_at;
    }
    for _ in 0..10 {
        let Ok(Plan::Hold {
            then: Apply::Reschedule { definition, run_at },
            ..
        }) = plan(schedule, None, now, Some(b"t:4500:1"))
        else {
            return Err("overlap did not queue".to_owned());
        };
        schedule = definition;
        now = run_at;
    }
    assert_eq!(
        schedule
            .next_run_at_ms
            .zip(schedule.last_overlap_at_ms)
            .map(|(next, at)| next - at),
        Some(30_000)
    );
    Ok(())
}

#[test]
fn a_later_claim_failure_is_reported_beside_the_outcomes() {
    // Elixir's `schedule_summary_test.exs`: "reports a later claim failure
    // separately from claimed schedule outcomes" and "omits claim_error from
    // the normal response".
    use super::list::{Fired, summary};
    let outcomes = [
        Fired::Fired {
            target_id: b"daily:1000:1".to_vec(),
            coalesced: 2,
        },
        Fired::Failed {
            id: b"broken".to_vec(),
            reason: "ERR target creation failed".to_owned(),
        },
    ];
    let reported = summary(&outcomes, Some("ERR claim unavailable"));
    let field = |term: &Term, key: &str| term.get(key.as_bytes()).cloned();
    assert_eq!(field(&reported, "claimed"), Some(Term::Int(2)));
    assert_eq!(field(&reported, "fired"), Some(Term::Int(1)));
    assert_eq!(field(&reported, "skipped"), Some(Term::Int(0)));
    assert_eq!(field(&reported, "coalesced"), Some(Term::Int(2)));
    assert_eq!(
        field(&reported, "errors"),
        Some(Term::List(vec![Term::List(vec![
            Term::text("broken"),
            Term::text("ERR target creation failed")
        ])]))
    );
    assert_eq!(
        field(&reported, "claim_error"),
        Some(Term::text("ERR claim unavailable"))
    );
    assert_eq!(
        field(&reported, "last_target_id"),
        Some(Term::text("daily:1000:1"))
    );
    assert_eq!(field(&summary(&[], None), "claim_error"), None);
    // `schedule_catchup_test.exs`: "a batch catch-up total stays within the
    // public exact-integer contract".
    let late = |id: &str| Fired::Fired {
        target_id: id.as_bytes().to_vec(),
        coalesced: MAX_EXACT - 1,
    };
    assert_eq!(
        field(&summary(&[late("a"), late("b")], None), "coalesced"),
        Some(Term::int(MAX_EXACT))
    );
    let held = Fired::Skipped {
        reason: "previous target still active: a".to_owned(),
        coalesced: MAX_EXACT - 1,
    };
    assert_eq!(
        field(&summary(&[late("a"), held], None), "coalesced"),
        Some(Term::int(MAX_EXACT))
    );
}

#[test]
fn catch_up_is_constant_work_and_an_elapsed_end_wins() -> Result<(), String> {
    // Elixir's `schedule_catchup_test.exs`: "catch-up planning remains
    // constant-work near the exact-integer limit" and "an elapsed end bound
    // takes precedence over recovery timestamp overflow".
    use super::plan::{Next, next};
    let mut every_one = create(vec![
        ("every_ms", Term::Int(1)),
        ("start_at_ms", Term::Int(0)),
        ("now_ms", Term::Int(0)),
        ("target", target(vec![("id_prefix", Term::text("t"))])),
    ])?;
    let recovery = MAX_EXACT - 1;
    assert!(matches!(
        next(&every_one, 0, Some(recovery))?,
        Next::At(MAX_EXACT, coalesced) if coalesced == recovery
    ));
    every_one.every_ms = Some(5);
    let due = recovery - 20;
    every_one.end_at_ms = Some(due + 10);
    assert!(matches!(
        next(&every_one, due, Some(recovery))?,
        Next::EndAt(2)
    ));
    Ok(())
}
