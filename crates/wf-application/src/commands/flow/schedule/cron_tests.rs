//! Elixir's `schedule_test.exs`, its cron cases: what a cron expression
//! refuses, and the next run it plans, through aliases, ranges, steps,
//! leap days, time zones and daylight-saving folds. A schedule starts at
//! its start time inclusive, as `create` asks for the first run after the
//! millisecond before it.

use jiff::Timestamp;

use super::cron::next_run_at_ms;

fn at(text: &str) -> Result<i64, String> {
    text.parse::<Timestamp>()
        .map(Timestamp::as_millisecond)
        .map_err(|error| error.to_string())
}

/// The first run at or after `start`, as a schedule's creation plans it.
fn first(expression: &str, start: i64, zone: &str) -> Result<u64, String> {
    next_run_at_ms(expression, start.saturating_sub(1), zone)
}

#[test]
fn malformed_expressions_are_refused_with_their_reason() {
    // Elixir: "invalid cron is rejected during schedule creation", "invalid
    // cron range is rejected during schedule creation", "cron fields reject
    // empty comma elements before searching the time window", "cron rejects
    // impossible calendar combinations before searching minutes",
    // "recurrence planning failures do not create orphan targets" (its
    // reason) and "cron schedule rejects invalid timezone".
    let refused = |expression: &str, zone: &str| first(expression, 0, zone).err();
    for (expression, reason) in [
        ("99 * * * *", "ERR flow schedule cron value out of range"),
        ("10-5 * * * *", "ERR flow schedule cron range is invalid"),
        (", * * * *", "ERR flow schedule cron field is invalid"),
        ("1,,2 * * * *", "ERR flow schedule cron field is invalid"),
        ("1, * * * *", "ERR flow schedule cron field is invalid"),
        (
            "0 0 31 FEB *",
            "ERR flow schedule cron day cannot occur in the selected months",
        ),
        (
            "not a cron expression",
            "ERR flow schedule cron must have 5 fields",
        ),
    ] {
        assert_eq!(
            refused(expression, "Etc/UTC"),
            Some(reason.to_owned()),
            "{expression}"
        );
    }
    assert_eq!(
        refused("0 9 * * *", "Mars/Olympus"),
        Some("ERR flow schedule timezone is invalid or unavailable".to_owned())
    );
}

#[test]
fn aliases_ranges_steps_and_leap_days_plan_their_runs() -> Result<(), String> {
    // Elixir: "cron schedule supports aliases, ranges, and steps", "cron
    // schedule supports weekday aliases" and "cron accepts leap-day
    // schedules independently of mutable application limits".
    let quarter = 15 * 60_000;
    assert_eq!(first("0-30/15 0 1 JAN ?", 0, "Etc/UTC")?, 0);
    assert_eq!(next_run_at_ms("0-30/15 0 1 JAN ?", 0, "Etc/UTC")?, quarter);
    assert_eq!(
        next_run_at_ms(
            "0-30/15 0 1 JAN ?",
            i64::try_from(quarter).map_err(|e| e.to_string())?,
            "Etc/UTC"
        )?,
        2 * quarter
    );
    // 1 January 1970 was a Thursday.
    assert_eq!(first("0 0 ? JAN THU", 0, "Etc/UTC")?, 0);
    let leap = first("0 0 29 FEB *", at("2097-03-01T00:00:00Z")?, "Etc/UTC")?;
    assert_eq!(
        i64::try_from(leap).map_err(|e| e.to_string())?,
        at("2104-02-29T00:00:00Z")?
    );
    Ok(())
}

#[test]
fn zones_and_daylight_saving_folds_plan_wall_clock_times() -> Result<(), String> {
    // Elixir: "cron schedule matches wall-clock time in configured
    // timezone", "cron schedule handles repeated DST wall-clock minute as
    // distinct due times" and "cron chooses the earliest instant across
    // multiple DST fallback wall minutes".
    let int = |value: u64| i64::try_from(value).map_err(|error| error.to_string());
    let jerusalem = first("0 9 * * *", at("2026-01-01T00:00:00Z")?, "Asia/Jerusalem")?;
    assert_eq!(int(jerusalem)?, at("2026-01-01T07:00:00Z")?);
    let repeated = first(
        "30 1 1 NOV *",
        at("2026-11-01T05:00:00Z")?,
        "America/New_York",
    )?;
    assert_eq!(int(repeated)?, at("2026-11-01T05:30:00Z")?);
    let again = next_run_at_ms("30 1 1 NOV *", int(repeated)?, "America/New_York")?;
    assert_eq!(int(again)?, at("2026-11-01T06:30:00Z")?);
    let fallback = next_run_at_ms("* 1 * * *", at("2024-11-03T05:30:00Z")?, "America/New_York")?;
    assert_eq!(int(fallback)?, at("2024-11-03T05:31:00Z")?);
    Ok(())
}

#[test]
fn the_calendar_s_last_minute_still_plans() -> Result<(), String> {
    // Elixir: "cron fire at the calendar ceiling completes after emitting
    // exactly one target": 9999-12-31T23:59Z plans, and nothing after it.
    let ceiling: i64 = 253_402_300_740_000;
    let first = first("59 23 31 DEC *", ceiling - 60_000, "Etc/UTC")?;
    assert_eq!(i64::try_from(first).map_err(|e| e.to_string())?, ceiling);
    assert_eq!(
        next_run_at_ms("59 23 31 DEC *", ceiling, "Etc/UTC").err(),
        Some(super::cron::CALENDAR_RANGE.to_owned())
    );
    assert_eq!(
        next_run_at_ms("* * * * *", ceiling + 59_999, "Etc/UTC").err(),
        Some(super::cron::CALENDAR_RANGE.to_owned())
    );
    // New York's last 23:59 is past the calendar's end in UTC.
    assert_eq!(
        next_run_at_ms("59 23 31 DEC *", ceiling - 86_400_000, "America/New_York").err(),
        Some(super::cron::CALENDAR_RANGE.to_owned())
    );
    Ok(())
}
