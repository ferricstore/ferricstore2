//! Reviewed restricted AST lineage; hashes/anchors do not authenticate authorship.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    CheckError, REFERENCE_MANIFEST_SHA256, SOURCE_COMMIT, artifact,
    progress::ArtifactRef,
    registration::RegisteredTest,
    runtime_evidence::{CollectionDetails, RawCase, read_ref},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginArtifact {
    schema_version: u32,
    source_commit: String,
    reference_manifest_sha256: String,
    origin_rule: String,
    review: ArtifactRef,
    source_collection: ArtifactRef,
    tools: Vec<ArtifactRef>,
    required_helper_paths: Vec<String>,
    helper_profiles: Value,
    requirements: Requirements,
    origins: Vec<SourceOrigin>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Requirements {
    setup_execution: String,
    fixture_review: String,
    mandatory_profile_execution: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionReview {
    schema_version: u32,
    source_commit: String,
    origin_rule: String,
    registration_is_execution: bool,
    review_note: String,
    sections: Vec<Section>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Section {
    macro_module: String,
    source_path: String,
    source_sha256: String,
    consumer_uses: Vec<ConsumerUse>,
    scope_decision: String,
    scope_reason: String,
    environment_decision: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsumerUse {
    registration_path: String,
    registration_line: usize,
    registration_sha256: String,
    use_text: String,
    consumer_module: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceOrigin {
    origin_id: String,
    source_declaration_id: String,
    source_path: String,
    source_line: usize,
    source_sha256: String,
    source_text: String,
    registration_path: String,
    registration_line: usize,
    registration_sha256: String,
    use_text: String,
    macro_module: String,
    macro_definition_line: usize,
    consumer_module: String,
    name: String,
    runtime_case_id: String,
    scope_decision: String,
}

pub(crate) fn load(
    root: &Path,
    evidence: &ArtifactRef,
    details_ref: &ArtifactRef,
    details: &CollectionDetails,
) -> Result<BTreeMap<String, SourceOrigin>, CheckError> {
    let data: OriginArtifact = serde_json::from_slice(&read_ref(root, evidence)?)
        .map_err(|error| CheckError::new(format!("invalid macro origin artifact: {error}")))?;
    if data.schema_version != 1
        || data.source_commit != SOURCE_COMMIT
        || data.reference_manifest_sha256 != REFERENCE_MANIFEST_SHA256
        || data.origin_rule != "literal-section-origin-v2"
        || &data.source_collection != details_ref
    {
        return Err(CheckError::new("unsupported macro origin provenance"));
    }
    check_requirements(&data, details)?;
    check_tools(root, &data.tools)?;
    let review: SectionReview = serde_json::from_slice(&read_ref(root, &data.review)?)
        .map_err(|error| CheckError::new(format!("invalid macro origin review: {error}")))?;
    let sections = check_review(root, &review)?;
    let mut result = BTreeMap::new();
    for origin in data.origins {
        let section = sections
            .get(origin.macro_module.as_str())
            .ok_or_else(|| CheckError::new("origin has no reviewed section"))?;
        origin.check(root, details, section)?;
        if result.insert(origin.origin_id.clone(), origin).is_some() {
            return Err(CheckError::new("duplicate macro origin identity"));
        }
    }
    Ok(result)
}

fn check_requirements(
    data: &OriginArtifact,
    details: &CollectionDetails,
) -> Result<(), CheckError> {
    if data.requirements.setup_execution != "unexecuted"
        || data.requirements.fixture_review != "pending"
        || data.requirements.mandatory_profile_execution != "unexecuted"
    {
        return Err(CheckError::new(
            "origin requirements cannot certify execution or fixture review",
        ));
    }
    let helpers: Vec<_> = details
        .helper_results
        .iter()
        .filter_map(|helper| helper["source_path"].as_str())
        .collect();
    let expected: Vec<_> = data
        .required_helper_paths
        .iter()
        .map(String::as_str)
        .collect();
    if helpers.len() != details.helper_results.len()
        || helpers != expected
        || data.helper_profiles != details.helper_profiles
        || !data.helper_profiles.is_object()
    {
        return Err(CheckError::new(
            "macro origin helper/profile provenance differs from original registration",
        ));
    }
    Ok(())
}

fn check_tools(root: &Path, tools: &[ArtifactRef]) -> Result<(), CheckError> {
    let expected: BTreeSet<_> = [
        "scripts/runtime-macro-origins.exs",
        "scripts/reconcile-elixir-cases.exs",
    ]
    .into_iter()
    .collect();
    let found: BTreeSet<_> = tools.iter().map(|tool| tool.path.as_str()).collect();
    if expected != found || found.len() != tools.len() {
        return Err(CheckError::new("macro origin tool provenance differs"));
    }
    for tool in tools {
        read_ref(root, tool)?;
    }
    Ok(())
}

fn check_review<'a>(
    root: &Path,
    review: &'a SectionReview,
) -> Result<BTreeMap<&'a str, &'a Section>, CheckError> {
    if review.schema_version != 1
        || review.source_commit != SOURCE_COMMIT
        || review.origin_rule != "literal-section-origin-v2"
        || review.registration_is_execution
    {
        return Err(CheckError::new("invalid reviewed source-origin rule"));
    }
    artifact::read(root, &review.review_note, 1024 * 1024)?;
    let mut result = BTreeMap::new();
    for section in &review.sections {
        if section.environment_decision != "unproved" {
            return Err(CheckError::new("unsupported macro environment decision"));
        }
        if !matches!(
            section.scope_decision.as_str(),
            "included-workflow" | "pending-case-review"
        ) || section.scope_reason.trim().is_empty()
            || section.consumer_uses.is_empty()
            || result
                .insert(section.macro_module.as_str(), section)
                .is_some()
        {
            return Err(CheckError::new(
                "invalid or duplicate reviewed scope/section",
            ));
        }
    }
    Ok(result)
}

impl SourceOrigin {
    fn check(
        &self,
        root: &Path,
        details: &CollectionDetails,
        section: &Section,
    ) -> Result<(), CheckError> {
        if section.environment_decision == "unproved" {
            return Err(CheckError::new(
                "macro origin has unproved preceding environment",
            ));
        }
        let mut canonical =
            serde_json::to_value(self).map_err(|error| CheckError::new(error.to_string()))?;
        canonical
            .as_object_mut()
            .ok_or_else(|| CheckError::new("origin object"))?
            .remove("origin_id");
        let bytes =
            serde_json::to_vec(&canonical).map_err(|error| CheckError::new(error.to_string()))?;
        if artifact::digest(&bytes) != self.origin_id {
            return Err(CheckError::new("macro origin identity hash differs"));
        }
        self.check_anchors(root, details)?;
        if self.source_path != section.source_path
            || self.source_sha256 != section.source_sha256
            || self.scope_decision != section.scope_decision
            || !section.consumer_uses.iter().any(|used| {
                used.registration_path == self.registration_path
                    && used.registration_line == self.registration_line
                    && used.registration_sha256 == self.registration_sha256
                    && used.use_text == self.use_text
                    && used.consumer_module == self.consumer_module
            })
        {
            return Err(CheckError::new(
                "macro origin differs from reviewed scope/source/use",
            ));
        }
        Ok(())
    }

    fn check_anchors(&self, root: &Path, details: &CollectionDetails) -> Result<(), CheckError> {
        let source = source_bytes(root, &self.source_path, details)?;
        let consumer = source_bytes(root, &self.registration_path, details)?;
        let source = String::from_utf8(source)
            .map_err(|_| CheckError::new("origin source must be UTF-8"))?;
        let consumer = String::from_utf8(consumer)
            .map_err(|_| CheckError::new("origin consumer must be UTF-8"))?;
        let expected_use = format!(
            "use {}",
            self.macro_module.strip_prefix("Elixir.").unwrap_or("")
        );
        if artifact::digest(source.as_bytes()) != self.source_sha256
            || artifact::digest(consumer.as_bytes()) != self.registration_sha256
            || self.source_line <= self.macro_definition_line
            || !anchor(&source, self.source_line, &self.source_text)
            || !anchor(&consumer, self.registration_line, &self.use_text)
            || self.use_text != expected_use
            || !source
                .lines()
                .nth(self.macro_definition_line.saturating_sub(1))
                .is_some_and(|line| line.trim().starts_with("defmacro __using__("))
        {
            return Err(CheckError::new("macro origin source anchor/hash differs"));
        }
        Ok(())
    }

    pub(crate) fn matches(
        &self,
        case: &RegisteredTest,
        original: &RawCase,
    ) -> Result<(), CheckError> {
        if self.source_declaration_id != case.source_declaration_id
            || self.source_path != case.source_path
            || self.source_line != case.source_line
            || self.registration_path != original.source_path
            || self.registration_line != original.source_line
            || self.consumer_module != original.module
            || self.name != original.name
            || Some(&self.runtime_case_id) != case.runtime_case_id.as_ref()
            || Some(&self.scope_decision) != case.scope_decision.as_ref()
        {
            return Err(CheckError::new(
                "linked case differs from reviewed macro source origin",
            ));
        }
        Ok(())
    }
}

fn anchor(source: &str, line: usize, text: &str) -> bool {
    line > 0
        && source
            .lines()
            .nth(line - 1)
            .is_some_and(|actual| actual.trim() == text)
}

fn source_bytes(
    root: &Path,
    path: &str,
    details: &CollectionDetails,
) -> Result<Vec<u8>, CheckError> {
    artifact::portable_path(path)?;
    if let Some(source) = details
        .additional_source_files
        .iter()
        .find(|source| source.source_path == path)
    {
        let bytes = read_ref(root, &source.source_ref)?;
        if artifact::digest(&bytes) != source.sha256 {
            return Err(CheckError::new("additional pinned source SHA-256 differs"));
        }
        return Ok(bytes);
    }
    artifact::read(
        root,
        &format!("tests/reference/elixir-baseline/{path}"),
        16 * 1024 * 1024,
    )
}
