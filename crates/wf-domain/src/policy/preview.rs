//! No-copy final policy count/string preview before cloning a bounded candidate.

use super::RetryOverride;
use super::definition::{
    FieldPatch, PolicySnapshot, PolicyVersion, RetryPatch, StateChange, StatePolicy,
};
use super::installation::{InstallCommand, MAX_POLICY_SNAPSHOT_BYTES, PolicyError, PolicyLimits};

pub(super) fn validate_final(
    base: Option<&PolicySnapshot>,
    command: &InstallCommand,
    limits: PolicyLimits,
) -> Result<(), PolicyError> {
    let mut bytes = command.workflow_type.len();
    match &command.patch.version {
        FieldPatch::Set(PolicyVersion::Bytes(version)) => add(&mut bytes, version.len())?,
        FieldPatch::Unchanged => {
            if let Some(PolicyVersion::Bytes(version)) = base.and_then(|base| base.version.as_ref())
            {
                add(&mut bytes, version.len())?;
            }
        }
        FieldPatch::Set(PolicyVersion::Number(_)) | FieldPatch::Clear => {}
    }
    if let Some(target) = target(base.map(|base| &base.retry), &command.patch.retry) {
        add(&mut bytes, target.len())?;
    }
    let mut states = 0;
    if let Some(base) = base {
        for (name, policy) in &base.states {
            state(
                &mut bytes,
                &mut states,
                name,
                Some(policy),
                command.patch.states.get(name),
                limits,
            )?;
        }
    }
    for (name, change) in &command.patch.states {
        if base.is_none_or(|base| !base.states.contains_key(name)) {
            state(&mut bytes, &mut states, name, None, Some(change), limits)?;
        }
    }
    if bytes > MAX_POLICY_SNAPSHOT_BYTES {
        Err(PolicyError::SnapshotTooLarge)
    } else {
        Ok(())
    }
}

fn state(
    bytes: &mut usize,
    count: &mut usize,
    name: &[u8],
    old: Option<&StatePolicy>,
    change: Option<&StateChange>,
    limits: PolicyLimits,
) -> Result<(), PolicyError> {
    if matches!(change, Some(StateChange::Remove)) {
        return Ok(());
    }
    *count += 1;
    if *count > limits.max_states() {
        return Err(PolicyError::StateBudget);
    }
    add(bytes, name.len())?;
    let target = match change {
        Some(StateChange::Patch(patch)) => target(old.map(|old| &old.retry), &patch.retry),
        None => old.and_then(|old| old.retry.exhausted_to.as_deref()),
        Some(StateChange::Remove) => None,
    };
    if let Some(target) = target {
        add(bytes, target.len())?;
    }
    Ok(())
}

fn target<'a>(
    old: Option<&'a RetryOverride>,
    patch: &'a FieldPatch<RetryPatch>,
) -> Option<&'a [u8]> {
    match patch {
        FieldPatch::Clear => None,
        FieldPatch::Unchanged => old.and_then(|old| old.exhausted_to.as_deref()),
        FieldPatch::Set(patch) => patch
            .exhausted_to
            .as_deref()
            .or_else(|| old.and_then(|old| old.exhausted_to.as_deref())),
    }
}

fn add(bytes: &mut usize, value: usize) -> Result<(), PolicyError> {
    *bytes = bytes
        .checked_add(value)
        .ok_or(PolicyError::SnapshotTooLarge)?;
    if *bytes > MAX_POLICY_SNAPSHOT_BYTES {
        Err(PolicyError::SnapshotTooLarge)
    } else {
        Ok(())
    }
}
