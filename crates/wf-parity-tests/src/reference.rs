//! Exact preserved inventory validation; textual declarations are not runtime cases.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::Deserialize;

use crate::{CheckError, REFERENCE_MANIFEST_SHA256, SOURCE_COMMIT, artifact};

mod dependencies;

const MANIFEST_PATH: &str = "tests/reference/workflow-test-manifest.json";
const BASELINE: &str = "tests/reference/elixir-baseline";
const DIRTY: &str = "tests/reference/elixir-working-tree";
const JSON_LIMIT: u64 = 8 * 1024 * 1024;
const SOURCE_LIMIT: u64 = 16 * 1024 * 1024;
const FILE_COUNT: usize = 480;
const DECLARATION_COUNT: usize = 5_858;
const DIRTY_COUNT: usize = 9;

/// Verified artifact totals, with no runtime-case or passing-test count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReferenceSummary {
    /// Primary inventory files; shared dependency snapshots are checked separately.
    pub files: usize,
    /// Number of navigational textual declarations, not `ExUnit` cases.
    pub declarations: usize,
    /// Number of separate uncommitted review-input snapshots.
    pub working_tree_changes: usize,
    /// Literal `Code.require_file(path, __DIR__)` edges within the preserved closure.
    pub static_require_edges: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    schema_version: u32,
    source_repository: String,
    source_commit: String,
    selection: String,
    declaration_index_limit: String,
    source_file_count: usize,
    indexed_declaration_count: usize,
    category_counts: BTreeMap<String, usize>,
    files: Vec<SourceFile>,
    pub(crate) declarations: Vec<SourceDeclaration>,
    working_tree_changes: Vec<DirtyFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFile {
    source_path: String,
    sha256: String,
    bytes: u64,
    reference_path: String,
    category: String,
    suggested_target: String,
    milestone: String,
    status: String,
    declaration_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceDeclaration {
    pub(crate) id: String,
    pub(crate) source_path: String,
    pub(crate) line: usize,
    declaration: String,
    name_kind: String,
    port_status: String,
    category: String,
    target: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirtyFile {
    source_path: String,
    sha256: String,
    reference_path: String,
    status: String,
}

pub(crate) fn load(root: &Path) -> Result<(Manifest, ReferenceSummary), CheckError> {
    let bytes = artifact::read(root, MANIFEST_PATH, JSON_LIMIT)?;
    if artifact::digest(&bytes) != REFERENCE_MANIFEST_SHA256 {
        return Err(CheckError::new(
            "preserved manifest SHA-256 differs from the reviewed baseline",
        ));
    }
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|error| CheckError::new(format!("invalid reference manifest: {error}")))?;
    validate_header(&manifest)?;
    let sources = verify_files(root, &manifest)?;
    verify_declarations(&manifest, &sources)?;
    verify_dirty(root, &manifest)?;
    let static_require_edges = verify_closure(&sources)?;
    dependencies::check(root)?;
    let summary = ReferenceSummary {
        files: manifest.files.len(),
        declarations: manifest.declarations.len(),
        working_tree_changes: manifest.working_tree_changes.len(),
        static_require_edges,
    };
    Ok((manifest, summary))
}

fn validate_header(manifest: &Manifest) -> Result<(), CheckError> {
    if manifest.schema_version != 1
        || manifest.source_repository != "ferricstore/ferricstore"
        || manifest.source_commit != SOURCE_COMMIT
        || manifest.source_file_count != FILE_COUNT
        || manifest.files.len() != FILE_COUNT
        || manifest.indexed_declaration_count != DECLARATION_COUNT
        || manifest.declarations.len() != DECLARATION_COUNT
        || manifest.working_tree_changes.len() != DIRTY_COUNT
        || manifest.selection.is_empty()
        || manifest.declaration_index_limit.is_empty()
    {
        return Err(CheckError::new(
            "reference schema, provenance or frozen inventory counts differ",
        ));
    }
    Ok(())
}

fn verify_files(root: &Path, manifest: &Manifest) -> Result<BTreeMap<String, String>, CheckError> {
    let mut sources = BTreeMap::new();
    let mut paths = BTreeSet::new();
    let mut categories = BTreeMap::new();
    for file in &manifest.files {
        artifact::portable_path(&file.source_path)?;
        if file.reference_path != format!("{BASELINE}/{}", file.source_path)
            || !artifact::valid_hex(&file.sha256, 64)
            || file.status != "reference-only-not-ported"
            || file.category.is_empty()
            || file.suggested_target.is_empty()
            || file.milestone.is_empty()
            || !paths.insert(file.reference_path.clone())
        {
            return Err(CheckError::new(format!(
                "invalid or duplicate source record: {}",
                file.source_path
            )));
        }
        let bytes = artifact::read(root, &file.reference_path, SOURCE_LIMIT)?;
        if u64::try_from(bytes.len()).map_err(|error| CheckError::new(error.to_string()))?
            != file.bytes
            || artifact::digest(&bytes) != file.sha256
        {
            return Err(CheckError::new(format!(
                "source byte count or SHA-256 mismatch: {}",
                file.source_path
            )));
        }
        let source = String::from_utf8(bytes)
            .map_err(|error| CheckError::new(format!("invalid preserved UTF-8: {error}")))?;
        if sources.insert(file.source_path.clone(), source).is_some() {
            return Err(CheckError::new("duplicate baseline source identity"));
        }
        *categories.entry(file.category.clone()).or_insert(0) += 1;
    }
    if paths != artifact::snapshot_paths(root, BASELINE)? {
        return Err(CheckError::new(
            "baseline snapshot contains missing or unmanifested files",
        ));
    }
    if categories != manifest.category_counts {
        return Err(CheckError::new(
            "file category totals disagree with manifest",
        ));
    }
    Ok(sources)
}

fn verify_declarations(
    manifest: &Manifest,
    sources: &BTreeMap<String, String>,
) -> Result<(), CheckError> {
    let files: BTreeMap<_, _> = manifest
        .files
        .iter()
        .map(|file| (file.source_path.as_str(), file))
        .collect();
    let mut declarations = BTreeMap::new();
    let mut locations = BTreeSet::new();
    for declaration in &manifest.declarations {
        let file = files
            .get(declaration.source_path.as_str())
            .ok_or_else(|| CheckError::new("declaration refers to an absent source file"))?;
        let source = sources
            .get(&declaration.source_path)
            .ok_or_else(|| CheckError::new("declaration source bytes are absent"))?;
        let line = declaration
            .line
            .checked_sub(1)
            .and_then(|index| source.lines().nth(index))
            .ok_or_else(|| CheckError::new("declaration source line is zero or out of bounds"))?;
        if !artifact::valid_hex(&declaration.id, 16)
            || declaration.port_status != "not-ported"
            || declaration.name_kind != "literal-or-interpolated-template"
            || declaration.category != file.category
            || declaration.target != file.suggested_target
            || declaration.declaration.is_empty()
            || !line.contains("test ")
            || declarations
                .insert(declaration.id.as_str(), declaration)
                .is_some()
            || !locations.insert((&declaration.source_path, declaration.line))
        {
            return Err(CheckError::new(format!(
                "invalid or duplicate declaration: {}",
                declaration.id
            )));
        }
    }
    verify_file_ids(manifest, &declarations)
}

fn verify_file_ids(
    manifest: &Manifest,
    declarations: &BTreeMap<&str, &SourceDeclaration>,
) -> Result<(), CheckError> {
    let mut referenced = BTreeSet::new();
    for file in &manifest.files {
        for id in &file.declaration_ids {
            let declaration = declarations
                .get(id.as_str())
                .ok_or_else(|| CheckError::new("file declaration ID has no indexed declaration"))?;
            if declaration.source_path != file.source_path || !referenced.insert(id.as_str()) {
                return Err(CheckError::new(
                    "declaration ID is duplicated or owned by the wrong file",
                ));
            }
        }
    }
    if referenced.len() != declarations.len() {
        return Err(CheckError::new("declaration index contains orphan IDs"));
    }
    Ok(())
}

fn verify_dirty(root: &Path, manifest: &Manifest) -> Result<(), CheckError> {
    let mut paths = BTreeSet::new();
    let mut identities = BTreeSet::new();
    for file in &manifest.working_tree_changes {
        artifact::portable_path(&file.source_path)?;
        if file.reference_path != format!("{DIRTY}/{}", file.source_path)
            || !artifact::valid_hex(&file.sha256, 64)
            || file.status != "uncommitted-review-input-not-baseline"
            || !identities.insert(&file.source_path)
            || !paths.insert(file.reference_path.clone())
        {
            return Err(CheckError::new(
                "invalid or duplicate working-tree review input",
            ));
        }
        if artifact::digest(&artifact::read(root, &file.reference_path, SOURCE_LIMIT)?)
            != file.sha256
        {
            return Err(CheckError::new(format!(
                "working-tree snapshot SHA-256 mismatch: {}",
                file.source_path
            )));
        }
    }
    if paths != artifact::snapshot_paths(root, DIRTY)? {
        return Err(CheckError::new(
            "working-tree snapshot contains missing or unmanifested files",
        ));
    }
    Ok(())
}

fn verify_closure(sources: &BTreeMap<String, String>) -> Result<usize, CheckError> {
    let mut edges = 0;
    for (path, source) in sources {
        let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
        for required in literal_requires(source)? {
            artifact::portable_path(required)?;
            let target = format!("{parent}/{required}");
            if !sources.contains_key(&target) {
                return Err(CheckError::new(format!(
                    "missing static required section: {path} -> {target}"
                )));
            }
            edges += 1;
        }
    }
    Ok(edges)
}

// This recognizes only the documented literal section-file convention. It is
// intentionally not an Elixir parser or a proof of dynamic/fixture closure.
fn literal_requires(source: &str) -> Result<Vec<&str>, CheckError> {
    let mut required = Vec::new();
    for call in source.split("Code.require_file(").skip(1) {
        let Some(literal) = call.trim_start().strip_prefix('"') else {
            continue;
        };
        let Some((path, remaining)) = literal.split_once('"') else {
            return Err(CheckError::new("unterminated static require-file path"));
        };
        let Some(after_comma) = remaining.trim_start().strip_prefix(',') else {
            continue;
        };
        let Some(after_dir) = after_comma.trim_start().strip_prefix("__DIR__") else {
            continue;
        };
        if after_dir.trim_start().starts_with(')') {
            if path.contains(['\\', '#']) {
                return Err(CheckError::new(
                    "unsupported escaped or interpolated static require-file path",
                ));
            }
            required.push(path);
        }
    }
    Ok(required)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        error::Error,
        fs,
    };

    use super::{
        Manifest, literal_requires, validate_header, verify_closure, verify_declarations,
        verify_file_ids,
    };

    fn fixture() -> Result<Manifest, serde_json::Error> {
        serde_json::from_slice(include_bytes!(
            "../../../tests/reference/workflow-test-manifest.json"
        ))
    }

    fn fixture_sources(manifest: &Manifest) -> Result<BTreeMap<String, String>, std::io::Error> {
        manifest
            .files
            .iter()
            .map(|file| {
                let source =
                    fs::read_to_string(crate::workspace_root().join(&file.reference_path))?;
                Ok((file.source_path.clone(), source))
            })
            .collect()
    }

    #[test]
    fn frozen_header_counts_and_provenance_reject_changes() -> Result<(), Box<dyn Error>> {
        let mut manifest = fixture()?;
        validate_header(&manifest)?;
        manifest.schema_version = 2;
        assert!(validate_header(&manifest).is_err());
        manifest.schema_version = 1;
        manifest.source_file_count -= 1;
        assert!(validate_header(&manifest).is_err());
        manifest.source_file_count += 1;
        manifest.indexed_declaration_count -= 1;
        assert!(validate_header(&manifest).is_err());
        manifest.indexed_declaration_count += 1;
        manifest.source_commit = "other".into();
        assert!(validate_header(&manifest).is_err());
        Ok(())
    }

    #[test]
    fn declaration_integrity_rejects_duplicate_ids_wrong_ownership_and_lines()
    -> Result<(), Box<dyn Error>> {
        let mut manifest = fixture()?;
        let sources = fixture_sources(&manifest)?;
        verify_declarations(&manifest, &sources)?;
        let original = manifest.declarations[1].id.clone();
        manifest.declarations[1].id = manifest.declarations[0].id.clone();
        assert!(verify_declarations(&manifest, &sources).is_err());
        manifest.declarations[1].id = original;
        let original = manifest.declarations[0].source_path.clone();
        manifest.declarations[0].source_path = "missing.exs".into();
        assert!(verify_declarations(&manifest, &sources).is_err());
        manifest.declarations[0].source_path = original;
        manifest.declarations[0].line = 0;
        assert!(verify_declarations(&manifest, &sources).is_err());
        manifest.declarations[0].line = usize::MAX;
        assert!(verify_declarations(&manifest, &sources).is_err());
        Ok(())
    }

    #[test]
    fn file_index_is_bidirectional_and_preserved_statuses_cannot_be_passed()
    -> Result<(), Box<dyn Error>> {
        let mut manifest = fixture()?;
        let declarations = manifest
            .declarations
            .iter()
            .map(|declaration| (declaration.id.as_str(), declaration))
            .collect();
        verify_file_ids(&manifest, &declarations)?;
        let removed = manifest.files[0].declaration_ids.pop();
        assert!(removed.is_some());
        assert!(verify_file_ids(&manifest, &declarations).is_err());
        manifest.files[0]
            .declaration_ids
            .push("not-an-indexed-id".into());
        assert!(verify_file_ids(&manifest, &declarations).is_err());
        drop(declarations);
        let sources = fixture_sources(&manifest)?;
        manifest.declarations[0].port_status = "passing".into();
        assert!(verify_declarations(&manifest, &sources).is_err());
        Ok(())
    }

    #[test]
    fn literal_section_closure_supports_multiline_but_does_not_claim_dynamic_collection()
    -> Result<(), Box<dyn Error>> {
        let source = "Code.require_file(\n  \"sections/head.exs\",\n  __DIR__\n)\nCode.require_file(@support)";
        assert_eq!(literal_requires(source)?, vec!["sections/head.exs"]);
        let mut sources = BTreeMap::from([
            ("test/root.exs".into(), source.into()),
            (
                "test/sections/head.exs".into(),
                "test \"literal fixture\", do: :ok".into(),
            ),
        ]);
        assert_eq!(verify_closure(&sources)?, 1);
        sources.remove("test/sections/head.exs");
        assert!(verify_closure(&sources).is_err());
        sources.insert(
            "test/root.exs".into(),
            "Code.require_file(\"../escape.exs\", __DIR__)".into(),
        );
        assert!(verify_closure(&sources).is_err());
        Ok(())
    }

    #[test]
    fn reference_fixture_has_no_duplicate_locations_or_ids() -> Result<(), Box<dyn Error>> {
        let manifest = fixture()?;
        let ids: BTreeSet<_> = manifest
            .declarations
            .iter()
            .map(|declaration| &declaration.id)
            .collect();
        assert_eq!(ids.len(), 5_858);
        Ok(())
    }
}
