//! ADR-012: a crate's combined integration-test root may load each earlier
//! test target, unchanged, from its own file and nothing else.

use super::{CheckResult, source_check_kind};

const REDIRECTION: &str = "module path redirection requires an audit gate";

fn redirected(package: &str, module: &str, text: &str, test_file: bool) -> CheckResult<bool> {
    Ok(source_check_kind(package, module, text, test_file)?
        .iter()
        .any(|finding| finding == REDIRECTION))
}

#[test]
fn a_combined_test_root_loads_each_target_from_its_own_file() -> CheckResult<()> {
    for package in [
        "wf-domain",
        "wf-ports",
        "wf-storage",
        "wf-application",
        "wf-types",
    ] {
        let root = "#[path = \"fencing_retry.rs\"] mod fencing_retry;\n\
                    #[path = \"claim_apply/main.rs\"] mod claim_apply;";
        assert!(
            !redirected(package, "tests::main", root, true)?,
            "{package}"
        );
    }
    Ok(())
}

#[test]
fn any_other_redirection_from_a_combined_root_is_refused() -> CheckResult<()> {
    for (module, text, test_file) in [
        (
            "tests::main",
            "#[path = \"other.rs\"] mod fencing_retry;",
            true,
        ),
        (
            "tests::main",
            "#[path = \"claim_apply/other.rs\"] mod claim_apply;",
            true,
        ),
        ("tests::main", "#[path = \"../x.rs\"] mod x;", true),
        ("tests::main", "#[path = \"a/b/main.rs\"] mod a;", true),
        ("tests::main", "#[path = \"/tmp/x.rs\"] mod x;", true),
        (
            "tests::main",
            "mod outer { #[path = \"x.rs\"] mod x; }",
            true,
        ),
        ("tests::main", "#[path = \"x.rs\"] mod x;", false),
        ("tests::other", "#[path = \"x.rs\"] mod x;", true),
        (
            "tests::claim_apply::main",
            "#[path = \"x.rs\"] mod x;",
            true,
        ),
    ] {
        assert!(
            redirected("wf-domain", module, text, test_file)?,
            "escaped the combined-root rule: {module}: {text}"
        );
    }
    Ok(())
}

fn root_check(
    policy: &crate::policy::Policy,
    package: &str,
    module: &str,
    root_file: bool,
) -> CheckResult<Vec<String>> {
    let names = std::collections::BTreeMap::new();
    let classification = policy
        .packages
        .get(package)
        .ok_or("missing classification")?;
    crate::source::check(
        "#![forbid(unsafe_code)]\nfn main() {}",
        &crate::source::Context {
            package,
            module,
            root_file,
            test_file: false,
            classification,
            policy,
            dependency_names: &names,
        },
    )
}

#[test]
fn only_a_declared_binary_root_is_a_classified_production_module() -> CheckResult<()> {
    let text = std::fs::read_to_string(super::root()?.join("architecture/policy.toml"))?;
    let policy = crate::policy::Policy::parse(&text)?;
    let unclassified = "unclassified production module";
    assert_eq!(
        policy
            .packages
            .get("wf-node")
            .map(|package| package.binaries.clone()),
        Some(vec!["main".to_owned()])
    );
    assert!(
        !root_check(&policy, "wf-node", "main", true)?
            .iter()
            .any(|f| f == unclassified)
    );
    for (package, root_file) in [
        ("wf-node", false),
        ("wf-domain", true),
        ("wf-api-tcp", true),
    ] {
        assert!(
            root_check(&policy, package, "main", root_file)?
                .iter()
                .any(|f| f == unclassified),
            "{package} main (root {root_file}) escaped classification"
        );
    }
    Ok(())
}
