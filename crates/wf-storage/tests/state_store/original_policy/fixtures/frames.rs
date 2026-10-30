//! Public frame fixture construction and exact intent/Complete adapters.

use super::{TestResult, clock, limits, log};
use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    OriginalPolicySetFrameInput, OriginalPolicySetMapOrders, OriginalPolicySetPatchView,
    PolicySchemaCharge, RawFlowPolicyValue as Raw, prepare_original_policy_set_frame_bytes,
    prepare_original_policy_set_request,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_ports::log::{
    prepare_original_policy_set_frame, prepare_original_policy_set_point_frame_bounded,
};
use wf_ports::state::StoredWorkflowRecord;
use wf_types::identity::RequestId;

pub(crate) fn public_frame(
    namespace: &RequestScope,
    request: &RequestId,
    stamp: u64,
    kind: &[u8],
    attrs: Raw<'_>,
    opts: Raw<'_>,
    charge: &mut PolicySchemaCharge,
) -> TestResult<Vec<u8>> {
    let Raw::Map(attrs) = attrs else {
        return Err("fixture attributes must be a map".into());
    };
    let Raw::List(controls) = opts else {
        return Err("fixture controls must be keyword list".into());
    };
    let pairs = attrs
        .iter()
        .map(|(key, value)| [*key, *value])
        .collect::<Vec<_>>();
    let mut rows = pairs
        .iter()
        .map(|pair| Raw::Tuple(pair))
        .collect::<Vec<_>>();
    rows.extend_from_slice(controls);
    let profile = charge.profile();
    let prepared = prepare_original_policy_set_request(
        &Raw::Binary(kind),
        &Raw::List(&rows),
        profile,
        charge,
    )?
    .map_err(|reason| format!("front fixture rejected: {reason:?}"))?;
    let mut records = Vec::new();
    patch_records(prepared.patch(), &mut records)?;
    let borrowed = records
        .iter()
        .map(|(n, p, q)| (*n, p.as_slice(), q.as_slice()))
        .collect::<Vec<_>>();
    let initial = OriginalPolicySetMapOrders::new([0; 32], None, &borrowed, &[], &[]);
    let input = OriginalPolicySetFrameInput::new(
        namespace.clone(),
        request.clone(),
        clock(stamp)?,
        &prepared,
        &initial,
    );
    let (bytes, _, _, range) = prepare_original_policy_set_frame_bytes(
        input,
        profile,
        AdmissionLimits::default(),
        charge,
    )?;
    let digest: [u8; 32] = Sha256::digest(&bytes[range]).into();
    let orders = OriginalPolicySetMapOrders::new(digest, None, &borrowed, &[], &[]);
    let input = OriginalPolicySetFrameInput::new(
        namespace.clone(),
        request.clone(),
        clock(stamp)?,
        &prepared,
        &orders,
    );
    Ok(
        prepare_original_policy_set_frame(input, profile, AdmissionLimits::default(), charge)?
            .as_bytes()
            .to_vec(),
    )
}

fn patch_records(
    value: OriginalPolicySetPatchView<'_>,
    out: &mut Vec<(u32, Vec<u32>, Vec<u32>)>,
) -> TestResult {
    if let Some(len) = value.map_len() {
        let rows = (0..len)
            .map(|i| value.map_entry(i).ok_or("fixture map row"))
            .collect::<Result<Vec<_>, _>>()?;
        let mut sorted = (0..len).collect::<Vec<_>>();
        let keys = rows
            .iter()
            .map(|(key, _)| match key.raw().ok_or("raw fixture key")? {
                Raw::Atom(name) => Ok((0, name.as_bytes())),
                Raw::Binary(name) => Ok((1, name)),
                _ => Err("fixture key class".into()),
            })
            .collect::<TestResult<Vec<_>>>()?;
        sorted.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
        let q = sorted
            .iter()
            .copied()
            .map(u32::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let mut p = vec![0; len];
        for (i, &j) in q.iter().enumerate() {
            p[usize::try_from(j)?] = u32::try_from(i)?;
        }
        out.push((u32::try_from(out.len())?, p, q));
        for i in sorted {
            patch_records(rows[i].0, out)?;
            patch_records(rows[i].1, out)?;
        }
    } else if let Some(len) = value.list_len() {
        for i in 0..len {
            patch_records(value.list_item(i).ok_or("fixture list row")?, out)?;
        }
    } else if let Some(Raw::Map(rows)) = value.raw() {
        if !rows.is_empty() {
            return Err("nonempty raw-map fixture needs explicit capture".into());
        }
        out.push((u32::try_from(out.len())?, Vec::new(), Vec::new()));
    }
    Ok(())
}

pub(in super::super) fn frame_intent(frame: &[u8], at: u64) -> TestResult<ClientIntent> {
    let profile = limits();
    let mut charge = PolicySchemaCharge::new(profile.schema);
    Ok(prepare_original_policy_set_point_frame_bounded(
        frame,
        profile.schema,
        profile.codec.admission(),
        &mut charge,
        profile.workspace_bytes,
    )?
    .header(log(at))
    .intent)
}

pub(in super::super) fn complete_frame(
    namespace: &RequestScope,
    request: &RequestId,
    owner: &StoredWorkflowRecord,
    now: u64,
) -> TestResult<Vec<u8>> {
    use wf_domain::workflow::caller::{
        MutationFieldValue as V, MutationInput, normalize_core_mutation,
    };
    use wf_ports::log::encode_public_mutation;
    let lease = owner.record.lease().ok_or("real lease required")?;
    let fields = [
        ("lease_token", V::Bytes(lease.token.as_bytes())),
        (
            "fencing_token",
            V::Integer(i128::from(owner.record.fence().get())),
        ),
        ("now_ms", V::Integer(i128::from(now))),
    ];
    let caller = normalize_core_mutation(
        MutationInput {
            operation: "complete",
            id: owner.record.id().as_bytes(),
            fields: &fields,
        },
        &[],
        Some(owner.partition.as_bytes()),
        AdmissionLimits::default(),
        usize::MAX,
    )?;
    let (frame, _) = encode_public_mutation(
        namespace,
        request,
        &caller,
        clock(now)?,
        limits().codec,
        limits().workspace_bytes,
    )?;
    Ok(frame.as_bytes().to_vec())
}
