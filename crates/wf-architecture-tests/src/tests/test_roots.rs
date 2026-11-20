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
