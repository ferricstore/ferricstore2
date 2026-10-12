//! Current trusted pure tooling, safe argv and bounded child outputs/deadline.

use std::{
    env,
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use super::{
    METADATA,
    model::{Capture, PureVerification},
    refs::{TOOLS, ensure},
};
use crate::{CheckError, artifact};

static NEXT_OUTPUT: AtomicU64 = AtomicU64::new(0);

pub(super) fn check(root: &Path, capture: &Capture, digest: &str) -> Result<(), CheckError> {
    let executable = current_launcher()?;
    let recorded = fs::canonicalize(&capture.elixir_launcher_path)
        .map_err(|error| CheckError::new(format!("compiler launcher prerequisite: {error}")))?;
    ensure(
        executable == recorded
            && artifact::digest(&read_file(&executable, capture.limits.content_bytes)?)
                == capture.elixir_launcher_sha256,
        "compiler current launcher path/hash drift",
    )?;
    let script = artifact::checked_path(root, TOOLS[6])?;
    let directory = env::temp_dir().join(format!(
        "wf-compiler-verify-{}-{}",
        std::process::id(),
        NEXT_OUTPUT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory)
        .map_err(|error| CheckError::new(format!("compiler verifier output ownership: {error}")))?;
    let result = run(&executable, &script, root, &directory, capture, digest);
    let cleanup = fs::remove_dir_all(&directory)
        .map_err(|error| CheckError::new(format!("compiler verifier cleanup: {error}")));
    result.and(cleanup)
}

fn run(
    executable: &Path,
    script: &Path,
    root: &Path,
    directory: &Path,
    capture: &Capture,
    digest: &str,
) -> Result<(), CheckError> {
    let stdout = directory.join("stdout");
    let stderr = directory.join("stderr");
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stdout)
        .map_err(|error| CheckError::new(error.to_string()))?;
    let errors = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stderr)
        .map_err(|error| CheckError::new(error.to_string()))?;
    let mut command = trusted_command(executable, script, root);
    command.stdout(output).stderr(errors);
    let mut child = command
        .spawn()
        .map_err(|error| CheckError::new(format!("compiler pure verifier unavailable: {error}")))?;
    let started = Instant::now();
    let status = loop {
        let sizes = fs::metadata(&stdout)
            .and_then(|output| fs::metadata(&stderr).map(|errors| (output.len(), errors.len())));
        let excess = sizes.as_ref().map_or(true, |(output, errors)| {
            *output > capture.limits.stdout_bytes || *errors > capture.limits.stderr_bytes
        });
        if excess || started.elapsed() > Duration::from_millis(capture.limits.verifier_deadline_ms)
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CheckError::new(
                "compiler pure verifier output/deadline limit exceeded",
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CheckError::new(format!(
                    "compiler pure verifier polling failed: {error}"
                )));
            }
        }
    };
    let output = read_file(&stdout, capture.limits.stdout_bytes)?;
    let errors = read_file(&stderr, capture.limits.stderr_bytes)?;
    ensure(
        status.success(),
        &format!(
            "compiler pure verifier failed: {}",
            String::from_utf8_lossy(&errors)
        ),
    )?;
    let report: PureVerification = serde_json::from_slice(&output).map_err(|error| {
        CheckError::new(format!(
            "compiler pure verifier malformed/multiple report: {error}"
        ))
    })?;
    report_check(capture, digest, &report)
}

fn report_check(
    capture: &Capture,
    digest: &str,
    report: &PureVerification,
) -> Result<(), CheckError> {
    ensure(
        report.schema_version == 1
            && report.capture_sha256 == digest
            && report.source_commit == capture.source_commit
            && report.elixir_version == capture.elixir_version
            && report.otp_release == capture.otp_release
            && report.normalization == capture.normalization,
        "compiler pure report current-input/version mismatch",
    )?;
    ensure(
        report.registered_cases == capture.registered.len()
            && report.candidate_origins == capture.candidate_origins.len()
            && report.unresolved_runtime_cases == capture.unresolved_runtime_case_ids.len()
            && report.unresolved_declarations == capture.unresolved_declaration_ids.len(),
        "compiler pure report complements disagree",
    )?;
    let statistics = &report.statistics;
    let sources: std::collections::BTreeSet<_> = capture
        .modules
        .iter()
        .filter_map(|row| row.source_path.as_deref())
        .collect();
    let beams: std::collections::BTreeSet<_> = capture
        .modules
        .iter()
        .map(|row| row.beam.sha256.as_str())
        .collect();
    ensure(
        statistics.registered_cases == capture.registered.len()
            && statistics.module_versions == capture.modules.len()
            && statistics.macro_events == capture.calls.len()
            && statistics.source_reads == sources.len()
            && statistics.source_parses <= statistics.source_reads
            && statistics.beam_decodes == beams.len(),
        "compiler pure report analysis counters disagree",
    )
}

fn current_launcher() -> Result<PathBuf, CheckError> {
    let paths = env::var_os("PATH")
        .ok_or_else(|| CheckError::new("compiler Elixir prerequisite: PATH absent"))?;
    for directory in env::split_paths(&paths) {
        let candidate = directory.join("elixir");
        if candidate.is_file() {
            return candidate
                .canonicalize()
                .map_err(|error| CheckError::new(error.to_string()));
        }
    }
    Err(CheckError::new(
        "compiler Elixir prerequisite: launcher missing",
    ))
}

fn trusted_command(executable: &Path, script: &Path, root: &Path) -> Command {
    let mut command = Command::new(executable);
    command
        .arg(script)
        .arg(root)
        .arg(METADATA)
        .current_dir(root)
        .stdin(Stdio::null());
    for variable in [
        "ERL_FLAGS",
        "ERL_AFLAGS",
        "ERL_ZFLAGS",
        "ERL_LIBS",
        "ELIXIR_ERL_OPTIONS",
        "ERL_INETRC",
        "ELIXIR_CLI_DRY_RUN",
        "BASH_ENV",
        "ENV",
        "ZDOTDIR",
    ] {
        command.env_remove(variable);
    }
    command
}

fn read_file(path: &Path, limit: u64) -> Result<Vec<u8>, CheckError> {
    let file = File::open(path).map_err(|error| CheckError::new(error.to_string()))?;
    ensure(
        file.metadata()
            .map_err(|error| CheckError::new(error.to_string()))?
            .is_file(),
        "compiler process path is not a regular file",
    )?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| CheckError::new(error.to_string()))?;
    ensure(
        u64::try_from(bytes.len()).map_err(|error| CheckError::new(error.to_string()))? <= limit,
        "compiler bounded process read exceeded",
    )?;
    Ok(bytes)
}

#[cfg(test)]
#[test]
fn safe_argv_preserves_literal_paths_and_removes_startup_injection() {
    let root = Path::new("/literal root; $(printf injected)");
    let script = Path::new("/trusted/script with space.exs");
    let command = trusted_command(Path::new("/current/elixir"), script, root);
    let arguments: Vec<_> = command.get_args().collect();
    assert_eq!(
        arguments,
        [
            script.as_os_str(),
            root.as_os_str(),
            std::ffi::OsStr::new(METADATA)
        ]
    );
    assert_eq!(command.get_program(), "/current/elixir");
    for variable in [
        "ERL_FLAGS",
        "ERL_AFLAGS",
        "ERL_ZFLAGS",
        "ERL_LIBS",
        "ELIXIR_ERL_OPTIONS",
        "ERL_INETRC",
    ] {
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == variable && value.is_none())
        );
    }
}

#[cfg(test)]
#[test]
fn malformed_multiple_reports_and_version_drift_stay_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let capture = super::model::synthetic_capture();
    let value = serde_json::json!({"schema_version":1,"capture_sha256":"0".repeat(64),"source_commit":crate::SOURCE_COMMIT,
        "elixir_version":"1.19.5","otp_release":"28","normalization":super::NORMALIZATION,
        "registered_cases":1,"candidate_origins":0,"unresolved_runtime_cases":1,"unresolved_declarations":0,
        "statistics":{"source_reads":0,"source_parses":0,"beam_decodes":0,"registered_cases":1,"module_versions":0,"macro_events":0}});
    let mut report: PureVerification = serde_json::from_value(value.clone())?;
    assert!(report_check(&capture, &"0".repeat(64), &report).is_ok());
    report.elixir_version = "drifted".to_owned();
    assert!(report_check(&capture, &"0".repeat(64), &report).is_err());
    assert!(
        serde_json::from_slice::<PureVerification>(format!("{value}{value}").as_bytes()).is_err()
    );
    assert!(serde_json::from_slice::<PureVerification>(b"{}").is_err());
    Ok(())
}

#[cfg(test)]
#[test]
fn actual_trusted_verifier_nonzero_exit_is_an_operational_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let root = crate::workspace_root().canonicalize()?;
    let script = artifact::checked_path(&root, TOOLS[6])?;
    let directory = env::temp_dir().join(format!(
        "wf-compiler-verify-{}-{}",
        std::process::id(),
        NEXT_OUTPUT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory)?;
    let result = run(
        &current_launcher()?,
        &script,
        &root,
        &directory,
        &super::model::synthetic_capture(),
        &"0".repeat(64),
    );
    fs::remove_dir_all(directory)?;
    assert!(result.is_err_and(|error| error.to_string().contains("pure verifier failed")));
    for deadline in [false, true] {
        let directory = env::temp_dir().join(format!(
            "wf-compiler-verify-{}-{}",
            std::process::id(),
            NEXT_OUTPUT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory)?;
        let mut capture = super::model::synthetic_capture();
        // Private process controls lower limits; the public header rejects profile drift.
        if deadline {
            capture.limits.verifier_deadline_ms = 0;
        } else {
            capture.limits.stderr_bytes = 1;
        }
        let result = run(
            &current_launcher()?,
            &script,
            &root,
            &directory,
            &capture,
            &"0".repeat(64),
        );
        fs::remove_dir_all(directory)?;
        assert!(result.is_err_and(|error| {
            error.to_string().contains("output/deadline limit exceeded")
                || (!deadline && error.to_string().contains("bounded process read exceeded"))
        }));
    }
    Ok(())
}
