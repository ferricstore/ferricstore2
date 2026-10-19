use std::fmt::Write as _;

use super::{CheckResult, Policy, graph, json, metadata, package_mut, root};

const SOURCE: &str =
    "git+https://github.com/cberner/redb?rev=7061f7e60d4573311c0636e161d417b93e9fd2b1";

fn configured(source: &str) -> CheckResult<Policy> {
    let mut document: toml::Value =
        std::fs::read_to_string(root()?.join("architecture/policy.toml"))?.parse()?;
    let external: toml::Value = format!(
        "external_normal = [{{ name = 'redb', version = '=4.4.0', features = ['std'], default_features = false, source = '{source}' }}]"
    )
    .parse()?;
    let dependencies = document["packages"]["wf-storage"]
        .as_table_mut()
        .ok_or("missing storage policy")?
        .entry("external_normal")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or("external policy must be an array")?;
    dependencies
        .retain(|dependency| dependency.get("name").and_then(toml::Value::as_str) != Some("redb"));
    dependencies.push(external["external_normal"][0].clone());
    let mut text = String::new();
    for (name, value) in document.as_table().ok_or("missing policy root")? {
        writeln!(&mut text, "{name} = {value}")?;
    }
    Policy::parse(&text)
}

#[test]
fn exact_git_source_is_scoped_and_pinned() -> CheckResult<()> {
    let policy = configured(SOURCE)?;
    let mut accepted = metadata()?;
    let edge = json!({
        "name": "redb", "kind": null, "optional": false, "target": null,
        "source": SOURCE, "path": null, "features": ["std"],
        "req": "=4.4.0", "uses_default_features": false,
    });
    let dependencies = package_mut(&mut accepted, "wf-storage")?["dependencies"]
        .as_array_mut()
        .ok_or("missing dependencies")?;
    dependencies.retain(|dependency| dependency["name"] != "redb");
    dependencies.push(edge.clone());
    assert_eq!(
        graph::check(&accepted, &policy, &root()?)?,
        Vec::<String>::new()
    );
    for (field, value) in [
        ("req", json!("^4.4.0")),
        ("features", json!(["std", "logging"])),
        ("uses_default_features", json!(true)),
        ("path", json!(root()?.join("vendor/redb"))),
        (
            "source",
            json!("registry+https://github.com/rust-lang/crates.io-index"),
        ),
        (
            "source",
            json!("git+https://example.invalid/redb?rev=7061f7e60d4573311c0636e161d417b93e9fd2b1"),
        ),
        (
            "source",
            json!(
                "git+https://github.com/cberner/redb?rev=7061f7e60d4573311c0636e161d417b93e9fd2b0"
            ),
        ),
        (
            "source",
            json!("git+https://github.com/cberner/redb?branch=main"),
        ),
    ] {
        let mut changed = accepted.clone();
        let dependency = package_mut(&mut changed, "wf-storage")?["dependencies"]
            .as_array_mut()
            .ok_or("missing dependencies")?
            .iter_mut()
            .find(|dependency| dependency["name"] == "redb")
            .ok_or("missing redb edge")?;
        dependency[field] = value;
        assert!(
            graph::check(&changed, &policy, &root()?)?
                .iter()
                .any(|failure| failure.contains("wf-storage -> redb: unreviewed")),
            "source policy admitted drift in {field}"
        );
    }
    package_mut(&mut accepted, "wf-domain")?["dependencies"]
        .as_array_mut()
        .ok_or("missing dependencies")?
        .push(edge);
    assert!(
        graph::check(&accepted, &policy, &root()?)?
            .iter()
            .any(|failure| failure.contains("unclassified external edge wf-domain -> redb"))
    );
    Ok(())
}

#[test]
fn git_source_policy_rejects_unpinned_or_unsupported_sources() -> CheckResult<()> {
    configured(SOURCE)?;
    configured("registry+https://github.com/rust-lang/crates.io-index")?;
    for source in [
        "git+https://github.com/cberner/redb",
        "git+https://github.com/cberner/redb?branch=main",
        "git+https://github.com/cberner/redb?tag=v4.4.0",
        "git+https://github.com/cberner/redb?rev=7061f7e",
        "git+https://github.com/cberner/redb?rev=7061f7e60d4573311c0636e161d417b93e9fd2bz",
        "git+http://github.com/cberner/redb?rev=7061f7e60d4573311c0636e161d417b93e9fd2b1",
        "git+https://github.com/cberner/redb?branch=main&rev=7061f7e60d4573311c0636e161d417b93e9fd2b1",
        "git+https://github.com/cberner/redb?rev=7061f7e60d4573311c0636e161d417b93e9fd2b1#another",
        "registry+https://example.invalid/index",
        "path+file:///private/tmp/redb",
    ] {
        assert!(
            configured(source).is_err(),
            "accepted unpinned source {source}"
        );
    }
    Ok(())
}

#[test]
fn committed_recovery_dependency_profiles_are_exact_and_layered() -> CheckResult<()> {
    let mut accepted = metadata()?;
    let dependencies = package_mut(&mut accepted, "wf-node")?["dependencies"]
        .as_array_mut()
        .ok_or("missing node dependencies")?;
    dependencies.retain(|edge| edge["name"] != "sha2");
    dependencies.push(json!({
        "name": "sha2", "kind": null, "optional": false, "target": null,
        "source": "registry+https://github.com/rust-lang/crates.io-index",
        "path": null, "features": [], "req": "=0.11.0", "uses_default_features": false,
    }));
    assert_eq!(
        graph::check(&accepted, &super::policy()?, &root()?)?,
        Vec::<String>::new()
    );
    for (name, field, value) in [
        ("sha2", "req", json!("^0.11.0")),
        ("sha2", "features", json!(["std"])),
        ("sha2", "uses_default_features", json!(true)),
        (
            "sha2",
            "source",
            json!("registry+https://example.invalid/index"),
        ),
        ("redb", "req", json!("^4.4.0")),
        ("redb", "features", json!(["std", "logging"])),
        ("redb", "uses_default_features", json!(true)),
        ("redb", "kind", json!(null)),
        (
            "redb",
            "source",
            json!("registry+https://github.com/rust-lang/crates.io-index"),
        ),
        ("redb", "path", json!(root()?.join("vendor/redb"))),
    ] {
        let mut changed = accepted.clone();
        let edge = package_mut(&mut changed, "wf-node")?["dependencies"]
            .as_array_mut()
            .ok_or("missing node dependencies")?
            .iter_mut()
            .find(|edge| edge["name"] == name)
            .ok_or("missing selected edge")?;
        edge[field] = value;
        let findings = graph::check(&changed, &super::policy()?, &root()?)?;
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains(&format!("wf-node -> {name}"))),
            "missed {name} drift in {field}: {findings:?}"
        );
    }
    Ok(())
}
