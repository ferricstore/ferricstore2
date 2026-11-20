//! ADR-010 panic lints and runtime effect ownership rules.

use std::collections::BTreeMap;

use super::{CheckResult, Context, Policy, manifest, root, source};

const PANIC_LINTS: [&str; 8] = [
    "unreachable",
    "string_slice",
    "get_unwrap",
    "unwrap_in_result",
    "let_underscore_must_use",
    "mem_forget",
    "exit",
    "fallible_impl_from",
];

fn real_policy_text() -> CheckResult<String> {
    Ok(std::fs::read_to_string(
        root()?.join("architecture/policy.toml"),
    )?)
}

fn check_with(
    policy: &Policy,
    package: &str,
    module: &str,
    source_text: &str,
    test_file: bool,
) -> CheckResult<Vec<String>> {
    let dependency_names = BTreeMap::new();
    let classification = policy
        .packages
        .get(package)
        .ok_or("missing source classification")?;
    source::check(
        source_text,
        &Context {
            package,
            module,
            root_file: false,
            test_file,
            classification,
            policy,
            dependency_names: &dependency_names,
        },
    )
}

fn has(findings: &[String], diagnosis: &str) -> bool {
    findings.iter().any(|finding| finding.contains(diagnosis))
}

#[test]
fn panic_lints_are_required_workspace_lints() -> CheckResult<()> {
    let real = root()?;
    let fixture = tempfile::tempdir()?;
    for file in ["rust-toolchain.toml", "clippy.toml"] {
        std::fs::copy(real.join(file), fixture.path().join(file))?;
    }
    let baseline: toml::Value = std::fs::read_to_string(real.join("Cargo.toml"))?.parse()?;
    for lint in PANIC_LINTS {
        let mut weakened = baseline.clone();
        if let Some(lints) = weakened["workspace"]["lints"]["clippy"].as_table_mut() {
            lints.remove(lint);
        }
        std::fs::write(
            fixture.path().join("Cargo.toml"),
            toml::to_string(&weakened)?,
        )?;
        let findings = manifest::check_root(fixture.path())?;
        assert!(
            has(
                &findings,
                &format!("workspace lint clippy::{lint} must be deny")
            ),
            "missing {lint} escaped policy: {findings:?}"
        );
    }
    Ok(())
}

#[test]
fn filesystem_effects_stay_in_approved_storage_packages() -> CheckResult<()> {
    let policy = Policy::parse(&real_policy_text()?)?;
    let diagnosis = "filesystem effects belong to an approved storage package";
    for (package, module) in [
        ("wf-node", "composition"),
        ("wf-application", "commands"),
        ("wf-api-tcp", "session"),
        ("wf-consensus", "adapter"),
    ] {
        for text in [
            "use std::fs::File;",
            "fn open() -> std::io::Result<std::fs::File> { std::fs::File::open(\"x\") }",
            "use std::fs as disk; fn read() -> std::io::Result<Vec<u8>> { disk::read(\"x\") }",
        ] {
            let findings = check_with(&policy, package, module, text, false)?;
            assert!(
                has(&findings, diagnosis),
                "{package}::{module}: {findings:?}"
            );
            let test_findings = check_with(&policy, package, module, text, true)?;
            assert!(
                !has(&test_findings, diagnosis),
                "test source: {test_findings:?}"
            );
        }
    }
    let storage = check_with(&policy, "wf-storage", "log", "use std::fs::File;", false)?;
    assert!(!has(&storage, diagnosis), "storage: {storage:?}");
    Ok(())
}

#[test]
fn durable_sync_calls_stay_in_approved_storage_modules() -> CheckResult<()> {
    let policy = Policy::parse(&real_policy_text()?)?;
    let diagnosis = "durable sync calls belong to approved storage modules";
    for text in [
        "fn sync(file: &std::fs::File) -> std::io::Result<()> { file.sync_data() }",
        "fn sync(file: &std::fs::File) -> std::io::Result<()> { file.sync_all() }",
        "fn sync(file: &std::fs::File) -> std::io::Result<()> { std::fs::File::sync_all(file) }",
    ] {
        let unapproved = check_with(&policy, "wf-storage", "state", text, false)?;
        assert!(has(&unapproved, diagnosis), "{unapproved:?}");
        let node = check_with(&policy, "wf-node", "recovery", text, false)?;
        assert!(has(&node, diagnosis), "{node:?}");
        let approved = check_with(&policy, "wf-storage", "log::store", text, false)?;
        assert!(!has(&approved, diagnosis), "{approved:?}");
        let test_source = check_with(&policy, "wf-storage", "state", text, true)?;
        assert!(!has(&test_source, diagnosis), "{test_source:?}");
    }
    Ok(())
}

#[test]
fn threads_are_spawned_only_by_approved_owner_modules() -> CheckResult<()> {
    let text = real_policy_text()?;
    let policy = Policy::parse(&text)?;
    let approved_text = text.replace(
        "thread_owner_modules = []",
        "thread_owner_modules = [\"wf-node::supervision\"]",
    );
    assert_ne!(approved_text, text, "policy must declare thread owners");
    let approved = Policy::parse(&approved_text)?;
    let diagnosis = "threads are spawned only by approved runtime owner modules";
    for source_text in [
        "fn start() { let _handle = std::thread::spawn(|| ()); }",
        "use std::thread; fn start() { let _builder = thread::Builder::new(); }",
        "use std::thread::spawn; fn start() { let _handle = spawn(|| ()); }",
        "fn start() { std::thread::scope(|_scope| ()); }",
    ] {
        let findings = check_with(&policy, "wf-node", "supervision", source_text, false)?;
        assert!(has(&findings, diagnosis), "{source_text}: {findings:?}");
        let owner = check_with(&approved, "wf-node", "supervision", source_text, false)?;
        assert!(!has(&owner, diagnosis), "{owner:?}");
        let other = check_with(&approved, "wf-node", "composition", source_text, false)?;
        assert!(has(&other, diagnosis), "{other:?}");
    }
    Ok(())
}

#[test]
fn blocking_calls_are_rejected_inside_async_code() -> CheckResult<()> {
    let policy = Policy::parse(&real_policy_text()?)?;
    let diagnosis = "blocking call inside async code";
    for blocking in [
        "async fn run(rx: std::sync::mpsc::Receiver<u8>) -> Option<u8> { rx.recv().ok() }",
        "async fn run(d: std::time::Duration) { std::thread::sleep(d) }",
        "fn run(d: std::time::Duration) -> impl Future<Output = ()> { async move { std::thread::sleep(d) } }",
        "struct S; impl S { async fn run(&self, f: &std::fs::File) -> std::io::Result<()> { f.sync_data() } }",
        "async fn run() -> std::io::Result<Vec<u8>> { std::fs::read(\"x\") }",
        "async fn run() { let _l = std::net::TcpListener::bind(\"127.0.0.1:0\"); }",
    ] {
        let findings = check_with(&policy, "wf-storage", "log::store", blocking, false)?;
        assert!(has(&findings, diagnosis), "{blocking}: {findings:?}");
    }
    for allowed in [
        "async fn run(rx: Rx) -> Option<u8> { rx.recv().await }",
        "async fn run(d: std::time::Duration) { offload(move || std::thread::sleep(d)).await }",
        "fn run(rx: std::sync::mpsc::Receiver<u8>) -> Option<u8> { rx.recv().ok() }",
        "async fn run() { fn inner(d: std::time::Duration) { std::thread::sleep(d) } }",
    ] {
        let findings = check_with(&policy, "wf-storage", "log::store", allowed, false)?;
        assert!(!has(&findings, diagnosis), "{allowed}: {findings:?}");
    }
    let test_source = check_with(
        &policy,
        "wf-storage",
        "log::store",
        "async fn run(d: std::time::Duration) { std::thread::sleep(d) }",
        true,
    )?;
    assert!(!has(&test_source, diagnosis), "{test_source:?}");
    Ok(())
}

#[test]
fn policy_requires_explicit_effect_owner_lists() -> CheckResult<()> {
    let text = real_policy_text()?;
    for key in [
        "filesystem_packages",
        "durable_sync_modules",
        "thread_owner_modules",
    ] {
        let without = text
            .lines()
            .filter(|line| !line.starts_with(key))
            .collect::<Vec<_>>()
            .join("\n");
        assert_ne!(without, text, "policy must declare {key}");
        assert!(Policy::parse(&without).is_err(), "{key} became optional");
    }
    Ok(())
}
