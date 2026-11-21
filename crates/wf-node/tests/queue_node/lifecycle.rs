//! Create, claim and complete with the Elixir server's outcomes.

use wf_application::commands::queue::ServiceError;
use wf_domain::value::DomainError;

use super::{TestResult, claim, complete, completion, create, directory, open};

#[test]
fn a_flow_is_created_claimed_and_completed() -> TestResult {
    let directory = directory("lifecycle")?;
    let node = open(&directory, 4)?;
    assert_eq!(create(&node, &["f1"])?, vec![Ok(())]);
    let jobs = claim(&node, 10)?;
    assert_eq!(jobs.len(), 1);
    let job = jobs.first().ok_or("no job")?;
    assert_eq!(job.id, b"f1");
    assert!(job.partition.starts_with(b"__flow_auto__:"));
    assert_eq!(job.fence, 1);
    assert_eq!(job.run_state, b"queued");
    let token = String::from_utf8(job.lease_token.clone())?;
    let parts: Vec<&str> = token.split(':').collect();
    assert_eq!(
        (parts.first(), parts.last(), parts.len()),
        (Some(&"w1"), Some(&"1"), 3)
    );
    assert_eq!(
        claim(&node, 10)?,
        Vec::new(),
        "a running flow was claimed again"
    );
    assert_eq!(complete(&node, vec![completion(job)])?, vec![Ok(())]);
    assert_eq!(
        complete(&node, vec![completion(job)])?,
        vec![Ok(())],
        "repeat completion"
    );
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn refusals_carry_the_elixir_errors_per_item() -> TestResult {
    let directory = directory("refusals")?;
    let node = open(&directory, 4)?;
    assert_eq!(create(&node, &["a", "b"])?, vec![Ok(()), Ok(())]);
    assert_eq!(
        create(&node, &["c", "a"])?,
        vec![
            Ok(()),
            Err(ServiceError::Domain(DomainError::AlreadyExists))
        ]
    );
    let jobs = claim(&node, 10)?;
    assert_eq!(jobs.len(), 3);
    let first = jobs.first().ok_or("no job")?;
    let mut stale = completion(first);
    stale.lease_token = b"wrong".to_vec();
    let mut missing = completion(first);
    missing.id = b"missing".to_vec();
    assert_eq!(
        complete(&node, vec![stale, completion(first), missing])?,
        vec![
            Err(ServiceError::Domain(DomainError::StaleLease)),
            Ok(()),
            Err(ServiceError::Domain(DomainError::NotFound)),
        ]
    );
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_node_opens_in_a_data_directory_that_does_not_exist_yet() -> TestResult {
    let path = directory("lifecycle-fresh")?.join("data");
    let node = open(&path, 2)?;
    assert!(create(&node, &["f1"])?.iter().all(Result::is_ok));
    Ok(())
}
