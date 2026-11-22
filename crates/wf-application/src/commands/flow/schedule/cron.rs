//! Cron schedules, as `Ferricstore.Flow.Schedule.Cron` reads and steps
//! them: five fields (minute, hour, day, month, weekday) with `*`/`?`,
//! lists, ranges, steps and month and weekday names; a day and a weekday
//! both restricted match when either does. Wall-clock minutes resolve in an
//! IANA zone: a repeated minute (DST fold) gives both instants, a skipped
//! one (DST gap) none.

use jiff::civil::{Date, DateTime};
use jiff::tz::{AmbiguousOffset, Offset, TimeZone};
use jiff::{SignedDuration, Timestamp, ToSpan};

const MINUTE_MS: i64 = 60_000;
const NO_MATCH: &str = "ERR flow schedule cron has no matching time in search window";
/// Elixir's calendar-range refusal.
pub const CALENDAR_RANGE: &str = "ERR flow schedule timestamp is outside supported calendar range";
const INVALID_ZONE: &str = "ERR flow schedule timezone is invalid or unavailable";
/// The calendar's last millisecond, 9999-12-31T23:59:59.999Z, Elixir's
/// bound; later than `Timestamp::MAX`, so instants are counted on the civil
/// calendar.
const CALENDAR_MAX_MS: i64 = 253_402_300_799_999;
const EPOCH: DateTime = DateTime::constant(1970, 1, 1, 0, 0, 0, 0);
/// Eight years of minutes, Elixir's search window.
pub const SEARCH_MINUTES: i64 = 8 * 366 * 24 * 60;

/// A parsed expression: the allowed values of each field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cron {
    minute: Vec<u8>,
    hour: Vec<u8>,
    day: Vec<u8>,
    day_any: bool,
    month: Vec<u8>,
    weekday: Vec<u8>,
    weekday_any: bool,
}

const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];
const WEEKDAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

fn invalid() -> String {
    "ERR flow schedule cron field is invalid".to_owned()
}

fn value(text: &str, names: &[&str], first: u8) -> Result<u8, String> {
    let upper = text.to_ascii_uppercase();
    if let Some(index) = names.iter().position(|name| *name == upper) {
        return u8::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(first))
            .ok_or_else(invalid);
    }
    if text.is_empty()
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-' || byte == b'+')
    {
        return Err(invalid());
    }
    text.parse::<i64>()
        .ok()
        .and_then(|number| u8::try_from(number).ok().or(Some(u8::MAX)))
        .ok_or_else(invalid)
}

/// One field's allowed values and whether it was `*`/`?`.
fn field(
    text: &str,
    bounds: (u8, u8),
    names: &[&str],
    first: u8,
) -> Result<(Vec<u8>, bool), String> {
    let (min, max) = bounds;
    if text == "*" || text == "?" {
        return Ok(((min..=max).collect(), true));
    }
    let mut values = Vec::new();
    for part in text.split(',') {
        if part.is_empty() {
            return Err(invalid());
        }
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => match step.parse::<u64>() {
                Ok(step) if step > 0 => (range, step),
                _ => return Err("ERR flow schedule cron step must be positive".to_owned()),
            },
            None => (part, 1),
        };
        let (start, end) = if range == "*" || range == "?" {
            (min, max)
        } else {
            let (low, high) = if let Some((low, high)) = range.split_once('-') {
                (value(low, names, first)?, value(high, names, first)?)
            } else {
                let single = value(range, names, first)?;
                (single, single)
            };
            if low < min || low > max || high < min || high > max {
                return Err("ERR flow schedule cron value out of range".to_owned());
            }
            if low > high {
                return Err("ERR flow schedule cron range is invalid".to_owned());
            }
            (low, high)
        };
        let step = usize::try_from(step).unwrap_or(usize::MAX);
        values.extend((start..=end).step_by(step));
    }
    values.sort_unstable();
    values.dedup();
    Ok((values, false))
}

/// Parses a cron expression.
///
/// # Errors
/// Elixir's text for a malformed expression.
pub fn parse(expression: &str) -> Result<Cron, String> {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    let [minute, hour, day, month, weekday] = fields.as_slice() else {
        return Err("ERR flow schedule cron must have 5 fields".to_owned());
    };
    let (minute, _) = field(minute, (0, 59), &[], 0)?;
    let (hour, _) = field(hour, (0, 23), &[], 0)?;
    let (day, day_any) = field(day, (1, 31), &[], 0)?;
    let (month, _) = field(month, (1, 12), &MONTHS, 1)?;
    let (weekday, weekday_any) = field(weekday, (0, 7), &WEEKDAYS, 0)?;
    let mut weekday: Vec<u8> = weekday
        .into_iter()
        .map(|day| if day == 7 { 0 } else { day })
        .collect();
    weekday.sort_unstable();
    weekday.dedup();
    Ok(Cron {
        minute,
        hour,
        day,
        day_any,
        month,
        weekday,
        weekday_any,
    })
}

/// The zone named `name`.
///
/// # Errors
/// Elixir's text for an unknown zone.
pub fn zone(name: &str) -> Result<TimeZone, String> {
    TimeZone::get(name).map_err(|_| INVALID_ZONE.to_owned())
}

fn days_in_month(month: u8) -> u8 {
    match month {
        2 => 29,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl Cron {
    /// Whether some selected day exists in some selected month.
    fn feasible(&self) -> bool {
        self.day_any
            || !self.weekday_any
            || self
                .month
                .iter()
                .any(|month| self.day.iter().any(|day| *day <= days_in_month(*month)))
    }

    fn day_matches(&self, date: Date) -> bool {
        let day = u8::try_from(date.day()).unwrap_or(0);
        let weekday = u8::try_from(date.weekday().to_sunday_zero_offset()).unwrap_or(0);
        match (self.day_any, self.weekday_any) {
            (true, true) => true,
            (true, false) => self.weekday.contains(&weekday),
            (false, true) => self.day.contains(&day),
            (false, false) => self.day.contains(&day) || self.weekday.contains(&weekday),
        }
    }

    /// The earliest instant of `date` at an allowed time after `after` and
    /// up to `upper`; a repeated minute offers both of its instants, a
    /// skipped one none, as Elixir's `candidate_for_date/6` picks.
    fn candidate(&self, date: Date, zone: &TimeZone, (after, upper): (i64, i64)) -> Option<i64> {
        let month = u8::try_from(date.month()).unwrap_or(0);
        if !self.month.contains(&month) || !self.day_matches(date) {
            return None;
        }
        self.hour
            .iter()
            .flat_map(|hour| self.minute.iter().map(move |minute| (*hour, *minute)))
            .filter_map(|(hour, minute)| {
                let hour = i8::try_from(hour).ok()?;
                let minute = i8::try_from(minute).ok()?;
                DateTime::new(date.year(), date.month(), date.day(), hour, minute, 0, 0).ok()
            })
            .flat_map(|civil| instants(zone, civil))
            .filter(|ms| *ms > after && *ms <= upper)
            .min()
    }
}

/// Milliseconds since the epoch of `civil` at `offset`.
fn epoch_ms(civil: DateTime, offset: Offset) -> Option<i64> {
    let local = i64::try_from(civil.duration_since(EPOCH).as_millis()).ok()?;
    local.checked_sub(i64::from(offset.seconds()).checked_mul(1_000)?)
}

/// The instants a wall-clock time names in `zone`, up to the calendar's
/// end.
fn instants(zone: &TimeZone, civil: DateTime) -> Vec<i64> {
    let offsets = match zone.to_ambiguous_timestamp(civil).offset() {
        AmbiguousOffset::Unambiguous { offset } => vec![offset],
        AmbiguousOffset::Fold { before, after } => vec![before, after],
        AmbiguousOffset::Gap { .. } => Vec::new(),
    };
    offsets
        .into_iter()
        .filter_map(|offset| epoch_ms(civil, offset))
        .filter(|ms| *ms <= CALENDAR_MAX_MS)
        .collect()
}

/// The date in `zone` of the instant `at_ms`, which may lie past
/// `Timestamp::MAX` up to the calendar's end.
fn local_date(at_ms: i64, zone: &TimeZone) -> Option<Date> {
    if at_ms > CALENDAR_MAX_MS {
        return None;
    }
    // Past `Timestamp::MAX`, the zone's offset there.
    let nearest = Timestamp::from_millisecond(at_ms).unwrap_or(Timestamp::MAX);
    let offset = i64::from(zone.to_offset(nearest).seconds()).checked_mul(1_000)?;
    let local = SignedDuration::from_millis(at_ms.checked_add(offset)?);
    EPOCH.checked_add(local).ok().map(DateTime::date)
}

/// The first instant after `after_ms` the expression matches in `zone`,
/// within the search window.
///
/// # Errors
/// Elixir's text for an expression no date can match, a zone that does
/// not exist, or a time outside the calendar.
pub fn next_run_at_ms(expression: &str, after_ms: i64, zone_name: &str) -> Result<u64, String> {
    let cron = parse(expression)?;
    if !cron.feasible() {
        return Err("ERR flow schedule cron day cannot occur in the selected months".to_owned());
    }
    let zone = zone(zone_name)?;
    let upper = after_ms.saturating_add(SEARCH_MINUTES.saturating_add(1).saturating_mul(MINUTE_MS));
    let first = local_date(after_ms, &zone).ok_or_else(|| CALENDAR_RANGE.to_owned())?;
    let days = SEARCH_MINUTES / (24 * 60) + 2;
    for offset in 0..=days {
        let Ok(date) = first.checked_add(offset.days()) else {
            return Err(CALENDAR_RANGE.to_owned());
        };
        if date.year() > 9_999 {
            return Err(CALENDAR_RANGE.to_owned());
        }
        if let Some(found) = cron.candidate(date, &zone, (after_ms, upper)) {
            return u64::try_from(found).map_err(|_| CALENDAR_RANGE.to_owned());
        }
    }
    Err(NO_MATCH.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{next_run_at_ms, parse};

    #[test]
    fn fields_take_names_ranges_steps_and_lists() {
        assert!(parse("*/15 9-17 * * MON-FRI").is_ok());
        assert_eq!(
            parse("* * *").err().as_deref(),
            Some("ERR flow schedule cron must have 5 fields")
        );
        assert_eq!(
            parse("61 * * * *").err().as_deref(),
            Some("ERR flow schedule cron value out of range")
        );
        assert_eq!(
            parse("5-1 * * * *").err().as_deref(),
            Some("ERR flow schedule cron range is invalid")
        );
        assert_eq!(
            parse("1,,2 * * * *").err().as_deref(),
            Some("ERR flow schedule cron field is invalid")
        );
        assert_eq!(
            parse("*/0 * * * *").err().as_deref(),
            Some("ERR flow schedule cron step must be positive")
        );
    }

    #[test]
    fn the_next_run_is_the_next_matching_wall_clock_minute() {
        // 2026-01-01T00:00Z is a Thursday; 09:00 New York is 14:00Z.
        assert_eq!(
            next_run_at_ms(
                "*/15 9-17 * * MON-FRI",
                1_767_225_600_000,
                "America/New_York"
            ),
            Ok(1_767_276_000_000)
        );
        assert_eq!(
            next_run_at_ms("0 0 30 2 *", 0, "Etc/UTC").err().as_deref(),
            Some("ERR flow schedule cron day cannot occur in the selected months")
        );
        assert_eq!(
            next_run_at_ms("* * * * *", 0, "Mars/Base").err().as_deref(),
            Some("ERR flow schedule timezone is invalid or unavailable")
        );
    }

    #[test]
    fn a_repeated_dst_minute_is_two_due_times() {
        // 2026-11-01 01:30 in New York happens at 05:30Z and again at 06:30Z.
        let first = next_run_at_ms("30 1 1 11 *", 1_793_404_800_000, "America/New_York");
        assert_eq!(first, Ok(1_793_511_000_000));
        let second = next_run_at_ms("30 1 1 11 *", 1_793_511_000_000, "America/New_York");
        assert_eq!(second, Ok(1_793_514_600_000));
    }
}
