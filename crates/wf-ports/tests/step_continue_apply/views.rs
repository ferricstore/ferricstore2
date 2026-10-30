use super::{fixtures as f, *};
use std::collections::BTreeMap;
use wf_domain::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder, ScopedRecordId};
use wf_domain::policy::definition::{FieldPatch, PolicyPatch, StateChange, StatePolicyPatch};
use wf_domain::policy::installation::{InstallCommand, PolicyLimits, prepare_install};
use wf_domain::workflow::indexed::TouchedIndexedGroupView;

pub(super) fn with_view<'current, T>(
    record: &'current WorkflowRecord,
    mode: StateMode,
    check: impl FnOnce(TouchedIndexedGroupView<'_, 'current>) -> TestResult<T>,
) -> TestResult<T> {
    let scope = f::scope()?;
    let placements = BTreeMap::from([(PartitionId::new(b"p".to_vec())?, scope.group().clone())]);
    let entry = ClaimRecord {
        record,
        partition_key: b"p",
        priority: 1,
        state_enter_seq: 5,
    };
    let id = ScopedRecordId::for_record(&entry);
    let current = BTreeMap::from([(id, Some(entry))]);
    let due = DueOrder::for_record(&entry).ok_or("fixture due")?;
    let parallel = BTreeMap::from([(due, true)]);
    let lane = LaneKey::for_record(&entry).ok_or("fixture lane")?;
    let member = LaneOrder::for_record(&entry);
    let fifo_members = BTreeMap::from([((lane, member), None)]);
    let fifo_heads = BTreeMap::from([(lane, None)]);
    let mut patch = PolicyPatch::default();
    patch.states.insert(
        b"charge".to_vec(),
        StateChange::Patch(StatePolicyPatch {
            mode: FieldPatch::Set(mode),
            ..StatePolicyPatch::default()
        }),
    );
    let policy = prepare_install(
        None,
        0,
        &InstallCommand::low_level(b"email".to_vec(), patch),
        PolicyLimits::default(),
    )?;
    let policies = BTreeMap::from([(b"email".as_slice(), Some(policy.policy()))]);
    let retry_policies = BTreeMap::new();
    check(TouchedIndexedGroupView {
        group: scope.group(),
        placements: &placements,
        current: &current,
        parallel: &parallel,
        fifo_members: &fifo_members,
        fifo_heads: &fifo_heads,
        policies: &policies,
        retry_policies: &retry_policies,
    })
}
