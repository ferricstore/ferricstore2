//! Original schedule discovery normalization with an explicitly captured grant result.
use super::{I, OriginalCreateScheduleMetadata as S};
use super::{
    budget::{Failure, source},
    options::{atom, get},
};

pub(super) fn run<'a>(
    value: I<'_, 'a>,
    workflow_type: &[u8],
    allowed: bool,
) -> Result<S<'a>, Failure> {
    let reject = || source("ERR flow schedule metadata is reserved for internal use");
    if !allowed || workflow_type != b"__ferricstore_schedule" {
        return Err(reject());
    }
    let I::Map(rows) = value else {
        return Err(reject());
    };
    let mut kind = None;
    let mut target = None;
    let mut zone = None;
    for &(key, value) in rows {
        match atom(key) {
            Some("kind") => kind = Some(value),
            Some("target_type") => target = Some(value),
            Some("timezone") => zone = Some(value),
            _ => return Err(reject()),
        }
    }
    let name = match kind {
        Some(I::Binary(v)) => std::str::from_utf8(v).ok(),
        Some(v) => atom(v),
        None => None,
    };
    let kind = match name {
        Some("one_shot") => "one_shot",
        Some("delay") => "delay",
        Some("interval") => "interval",
        Some("cron") => "cron",
        _ => return Err(reject()),
    };
    let Some(I::Binary(target_type)) = target else {
        return Err(reject());
    };
    if target_type.is_empty() {
        return Err(reject());
    }
    let timezone = match (kind, rows.len(), zone) {
        ("cron", 3, Some(I::Binary(v))) if !v.is_empty() => Some(v),
        ("one_shot" | "delay" | "interval", 2, None) => None,
        _ => return Err(reject()),
    };
    Ok(S {
        kind,
        target_type,
        timezone,
    })
}

pub(super) fn optional<'a>(
    rows: &[I<'_, 'a>],
    workflow_type: &[u8],
    allowed: bool,
) -> Result<Option<S<'a>>, Failure> {
    get(rows, "schedule_metadata")
        .map(|v| run(v, workflow_type, allowed))
        .transpose()
}
