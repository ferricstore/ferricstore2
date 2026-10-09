//! Real archunit release evaluation; forbidden sources are parser data only.
#![forbid(unsafe_code)]

use std::{error::Error, fs, path::Path};

use archunit::{Checkable, ViolationKind, project_files_in, project_layers_in};

fn fixture(adapter: &str) -> Result<tempfile::TempDir, Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    fs::create_dir(directory.path().join("src"))?;
    fs::write(
        directory.path().join("Cargo.toml"),
        include_str!("fixtures/archunit/manifest.toml"),
    )?;
    fs::write(
        directory.path().join("src/lib.rs"),
        include_str!("fixtures/archunit/lib.rs"),
    )?;
    fs::write(
        directory.path().join("src/domain.rs"),
        include_str!("fixtures/archunit/domain.rs"),
    )?;
    fs::write(directory.path().join("src/adapter.rs"), adapter)?;
    Ok(directory)
}

#[test]
fn release_detects_forbidden_edges_reexports_aliases_and_qualified_references()
-> Result<(), Box<dyn Error>> {
    for text in [
        include_str!("fixtures/archunit/edge.rs"),
        include_str!("fixtures/archunit/reexport.rs"),
        include_str!("fixtures/archunit/alias.rs"),
        include_str!("fixtures/archunit/qualified.rs"),
    ] {
        let directory = fixture(text)?;
        let failures = project_files_in(directory.path())
            .in_path("src/adapter.rs")
            .should_not()
            .depend_on_files()
            .in_path("src/domain.rs")
            .check()?;
        assert!(
            failures
                .iter()
                .any(|failure| failure.kind() == ViolationKind::FileDependency),
            "{failures:?}"
        );
        let failures = project_layers_in(directory.path())
            .layer("adapter")
            .defined_by("src/adapter.rs")
            .layer("domain")
            .defined_by("src/domain.rs")
            .where_layer("adapter")
            .may_only_depend_on_layers(&[])
            .check()?;
        assert!(
            failures
                .iter()
                .any(|failure| failure.kind() == ViolationKind::LayerDependency),
            "{failures:?}"
        );
    }
    Ok(())
}

#[test]
fn empty_file_and_layer_selections_fail_and_positive_control_passes() -> Result<(), Box<dyn Error>>
{
    let directory = fixture("pub struct Pure;\n")?;
    let failures = project_files_in(directory.path())
        .in_path("src/missing/**")
        .should()
        .have_no_cycles()
        .check()?;
    assert!(
        failures
            .iter()
            .any(|failure| failure.kind() == ViolationKind::EmptyTest)
    );
    let failures = project_layers_in(directory.path())
        .layer("missing")
        .defined_by("src/missing/**")
        .where_layer("missing")
        .may_only_depend_on_layers(&[])
        .check()?;
    assert!(
        failures
            .iter()
            .any(|failure| failure.kind() == ViolationKind::EmptyTest)
    );
    assert!(
        project_files_in(directory.path())
            .in_path("src/adapter.rs")
            .should_not()
            .depend_on_files()
            .in_path("src/domain.rs")
            .check()?
            .is_empty()
    );
    Ok(())
}

#[test]
fn ignore_directive_is_a_demonstrated_framework_blind_spot() -> Result<(), Box<dyn Error>> {
    let directory = fixture(include_str!("fixtures/archunit/ignored.rs"))?;
    let failures = project_files_in(directory.path())
        .in_path("src/adapter.rs")
        .should_not()
        .depend_on_files()
        .in_path("src/domain.rs")
        .check()?;
    assert!(
        failures.is_empty(),
        "upstream behavior changed; re-evaluate policy supplement: {failures:?}"
    );
    // The source policy has an independent negative fixture for this directive.
    Ok(())
}

#[test]
fn actual_workspace_files_follow_all_fifteen_classified_layers() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let policy: toml::Value = fs::read_to_string(root.join("architecture/policy.toml"))?.parse()?;
    let packages = policy["packages"]
        .as_table()
        .ok_or("missing policy packages")?;
    let mut rule = project_layers_in(&root);
    for name in packages.keys() {
        rule = rule.layer(name).defined_by(format!("crates/{name}/src/**"));
    }
    for (name, package) in packages {
        let allowed: Vec<_> = package["normal"]
            .as_array()
            .ok_or("missing first-party normal edges")?
            .iter()
            .filter_map(toml::Value::as_str)
            .collect();
        rule = rule.where_layer(name).may_only_depend_on_layers(&allowed);
    }
    let failures = rule.check()?;
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}
