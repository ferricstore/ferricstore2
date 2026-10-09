//! Infrastructure behavior and failure checks, not workflow parity cases.

#![forbid(unsafe_code)]

use std::{
    error::Error,
    fs, io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use wf_test_support::{
    clock::{ClockError, ManualClock},
    fault::{FaultError, MAX_WAIT, fault_barrier},
    resources::TestResources,
};

type TestResult = Result<(), Box<dyn Error>>;
const BOUND: Duration = Duration::from_secs(5);

#[test]
fn resources_are_isolated_and_same_path_can_be_reopened() -> TestResult {
    let first = TestResources::new()?;
    let second = TestResources::new()?;
    assert_ne!(first.path(), second.path());
    let persisted = first.path().join("fixture");
    fs::write(&persisted, b"exact fixture bytes\0")?;
    assert_eq!(fs::read(&persisted)?, b"exact fixture bytes\0");
    assert!(!second.path().join("fixture").exists());
    let first_path = first.path().to_path_buf();
    first.close()?;
    assert!(!first_path.exists());
    assert!(second.path().is_dir());
    Ok(())
}

#[test]
fn resources_clean_up_nested_files_on_drop() -> TestResult {
    let resources = TestResources::new()?;
    let path = resources.path().to_path_buf();
    fs::create_dir(path.join("nested"))?;
    fs::write(path.join("nested/bytes"), b"retained by owner")?;
    drop(resources);
    assert!(!path.exists());
    Ok(())
}

#[test]
fn unavailable_resource_parent_is_an_error() -> TestResult {
    let parent = TestResources::new()?;
    assert!(TestResources::in_parent(&parent.path().join("missing")).is_err());
    let file = parent.path().join("file");
    fs::write(&file, b"not a directory")?;
    assert!(TestResources::in_parent(&file).is_err());
    Ok(())
}

#[test]
fn explicit_clock_progress_is_shared_and_equality_is_valid() -> TestResult {
    let clock = ManualClock::new(0);
    let injected = clock.clone();
    assert_eq!(clock.now_millis(), 0);
    assert_eq!(injected.advance_by(9_999)?, 9_999);
    assert_eq!(clock.now_millis(), 9_999);
    clock.set_millis(10_000)?;
    injected.set_millis(10_000)?;
    assert_eq!(injected.advance_by(0)?, 10_000);
    assert_eq!(clock.now_millis(), 10_000);
    Ok(())
}

#[test]
fn rejected_clock_changes_preserve_the_previous_value() -> TestResult {
    let clock = ManualClock::new(u64::MAX - 1);
    assert_eq!(clock.set_millis(0), Err(ClockError::Backwards));
    assert_eq!(clock.advance_by(2), Err(ClockError::Overflow));
    assert_eq!(clock.now_millis(), u64::MAX - 1);
    assert_eq!(clock.advance_by(1)?, u64::MAX);
    assert_eq!(clock.advance_by(1), Err(ClockError::Overflow));
    assert_eq!(clock.now_millis(), u64::MAX);
    Ok(())
}

#[test]
fn concurrent_explicit_advances_do_not_lose_updates() -> TestResult {
    let clock = ManualClock::new(20);
    let mut joins = Vec::new();
    for _ in 0..4 {
        let injected = clock.clone();
        joins.push(thread::spawn(move || -> Result<(), ClockError> {
            for _ in 0..100 {
                injected.advance_by(1)?;
            }
            Ok(())
        }));
    }
    for join in joins {
        join.join()
            .map_err(|_| io::Error::other("clock thread failed"))??;
    }
    assert_eq!(clock.now_millis(), 420);
    Ok(())
}

#[test]
fn acknowledgement_precedes_release_and_target_completion() -> TestResult {
    let (controller, hook) = fault_barrier();
    let completed = Arc::new(AtomicBool::new(false));
    let target_completed = Arc::clone(&completed);
    let target = thread::spawn(move || -> Result<(), FaultError> {
        hook.reach(BOUND)?;
        target_completed.store(true, Ordering::SeqCst);
        Ok(())
    });
    let reached = controller.wait_reached(BOUND)?;
    assert!(!completed.load(Ordering::SeqCst));
    reached.release()?;
    target
        .join()
        .map_err(|_| io::Error::other("fault target failed"))??;
    assert!(completed.load(Ordering::SeqCst));
    Ok(())
}

#[test]
fn an_operation_ending_without_the_hook_cannot_count_as_injection() {
    let (controller, hook) = fault_barrier();
    drop(hook);
    assert!(matches!(
        controller.wait_reached(BOUND),
        Err(FaultError::HookMissing)
    ));
}

#[test]
fn an_unreached_hook_times_out_as_failure() {
    let (controller, _hook) = fault_barrier();
    assert!(matches!(
        controller.wait_reached(Duration::from_millis(10)),
        Err(FaultError::HookTimeout)
    ));
}

#[test]
fn dropping_acknowledgement_cancels_the_paused_target() -> TestResult {
    let (controller, hook) = fault_barrier();
    let target = thread::spawn(move || hook.reach(BOUND));
    drop(controller.wait_reached(BOUND)?);
    assert_eq!(
        target
            .join()
            .map_err(|_| io::Error::other("fault target failed"))?,
        Err(FaultError::Disconnected)
    );
    Ok(())
}

#[test]
fn an_unreleased_hook_times_out_and_late_release_fails() -> TestResult {
    let (controller, hook) = fault_barrier();
    let target = thread::spawn(move || hook.reach(Duration::from_millis(20)));
    let reached = controller.wait_reached(BOUND)?;
    assert_eq!(
        target
            .join()
            .map_err(|_| io::Error::other("fault target failed"))?,
        Err(FaultError::ReleaseTimeout)
    );
    assert_eq!(reached.release(), Err(FaultError::Disconnected));
    Ok(())
}

#[test]
fn a_missing_controller_cannot_acknowledge_a_hook() {
    let (controller, hook) = fault_barrier();
    drop(controller);
    assert_eq!(hook.reach(BOUND), Err(FaultError::Disconnected));
}

#[test]
fn zero_and_excessive_watchdog_bounds_are_rejected() {
    for bound in [Duration::ZERO, MAX_WAIT + Duration::from_secs(1)] {
        let (controller, _hook) = fault_barrier();
        assert!(matches!(
            controller.wait_reached(bound),
            Err(FaultError::InvalidBound)
        ));
        let (_controller, hook) = fault_barrier();
        assert_eq!(hook.reach(bound), Err(FaultError::InvalidBound));
    }
}
