use std::{collections::BTreeMap, fs, path::Path};

use crate::CheckResult;

pub(crate) struct Policy {
    pub(crate) packages: BTreeMap<String, Package>,
    pub(crate) wire_domain_module: String,
    pub(crate) worker_port_module: String,
    pub(crate) worker_wire_modules: Vec<String>,
}

pub(crate) struct Package {
    pub(crate) runtime: bool,
    pub(crate) normal: Vec<String>,
    pub(crate) dev: Vec<String>,
    pub(crate) modules: BTreeMap<String, bool>,
    pub(crate) external_normal: Vec<External>,
    pub(crate) external_dev: Vec<External>,
}

pub(crate) struct External {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) source: String,
    pub(crate) features: Vec<String>,
    pub(crate) default_features: bool,
}

impl Policy {
    pub(crate) fn load(path: &Path) -> CheckResult<Self> {
        Self::parse(&fs::read_to_string(path)?)
    }

    pub(crate) fn parse(text: &str) -> CheckResult<Self> {
        let document: toml::Value = text.parse()?;
        if document.get("schema").and_then(toml::Value::as_integer) != Some(1) {
            return Err("unsupported architecture policy schema".into());
        }
        let package_table = document
            .get("packages")
            .and_then(toml::Value::as_table)
            .ok_or("missing policy packages")?;
        let mut packages = BTreeMap::new();
        for (name, value) in package_table {
            let runtime = match string(value, "kind")? {
                "runtime" => true,
                "tooling" => false,
                _ => return Err("unclassified package kind".into()),
            };
            let mut modules = BTreeMap::new();
            if let Some(table) = value.get("modules").and_then(toml::Value::as_table) {
                for (name, public) in table {
                    modules.insert(
                        name.clone(),
                        public
                            .as_bool()
                            .ok_or("module visibility must be boolean")?,
                    );
                }
            }
            if runtime && modules.is_empty() {
                return Err(format!("{name}: no module classification").into());
            }
            packages.insert(
                name.clone(),
                Package {
                    runtime,
                    normal: strings(value, "normal")?,
                    dev: strings(value, "dev")?,
                    modules,
                    external_normal: externals(value, "external_normal")?,
                    external_dev: externals(value, "external_dev")?,
                },
            );
        }
        if packages.is_empty() {
            return Err("empty architecture package classification".into());
        }
        Ok(Self {
            packages,
            wire_domain_module: string(&document, "wire_domain_module")?.into(),
            worker_port_module: string(&document, "worker_port_module")?.into(),
            worker_wire_modules: strings(&document, "worker_wire_modules")?,
        })
    }
}

fn externals(value: &toml::Value, key: &str) -> CheckResult<Vec<External>> {
    let Some(values) = value.get(key) else {
        return Ok(Vec::new());
    };
    values
        .as_array()
        .ok_or("external policy must be an array")?
        .iter()
        .map(|value| {
            Ok(External {
                name: string(value, "name")?.into(),
                version: string(value, "version")?.into(),
                source: external_source(value)?,
                features: strings(value, "features")?,
                default_features: value
                    .get("default_features")
                    .and_then(toml::Value::as_bool)
                    .ok_or("missing external default-features policy")?,
            })
        })
        .collect()
}

fn external_source(value: &toml::Value) -> CheckResult<String> {
    const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";
    let source = match value.get("source") {
        None => REGISTRY,
        Some(value) => value.as_str().ok_or("external source must be a string")?,
    };
    let pinned_git = source
        .strip_prefix("git+https://")
        .and_then(|git| git.split_once("?rev="))
        .is_some_and(|(repository, revision)| {
            repository.split_once('/').is_some_and(|(host, path)| {
                !host.is_empty() && !path.is_empty() && !repository.contains(['?', '#', '@'])
            }) && !repository.chars().any(char::is_whitespace)
                && revision.len() == 40
                && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
    if source != REGISTRY && !pinned_git {
        return Err(
            "external source must be the approved registry or full-revision HTTPS Git".into(),
        );
    }
    Ok(source.into())
}

fn string<'a>(value: &'a toml::Value, key: &str) -> CheckResult<&'a str> {
    value
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("missing policy string {key}").into())
}

fn strings(value: &toml::Value, key: &str) -> CheckResult<Vec<String>> {
    value
        .get(key)
        .and_then(toml::Value::as_array)
        .ok_or_else(|| format!("missing policy array {key}"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "policy array entry must be a string".into())
        })
        .collect()
}
