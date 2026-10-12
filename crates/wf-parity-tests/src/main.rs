//! CLI for preservation, progress accounting and the incomplete full-parity gate.

#![forbid(unsafe_code)]

use std::{env, path::PathBuf, process::ExitCode};

use wf_parity_tests::{
    check_compiler_lineage, check_progress, check_reference, require_parity, workspace_root,
};

fn usage(message: &str) -> String {
    format!(
        "{message}; usage: wf-parity-tests <check-reference|check-progress|check-compiler-lineage|require-parity> [--root PATH]"
    )
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let command = args.next().ok_or_else(|| usage("missing subcommand"))?;
    let root = match args.next() {
        None => workspace_root(),
        Some(option) if option == "--root" => PathBuf::from(
            args.next()
                .ok_or_else(|| usage("--root requires a checkout path"))?,
        ),
        Some(_) => return Err(usage("unknown argument")),
    };
    if args.next().is_some() {
        return Err(usage("unexpected trailing arguments"));
    }
    if command == "check-reference" {
        let result = check_reference(&root).map_err(|error| error.to_string())?;
        println!(
            "M0 artifact integrity verified: {} pinned files, {} textual declarations, \
             {} separate working-tree inputs, {} literal section-file edges. \
             No ExUnit runtime cases collected or Rust workflow behavior executed; all behavior unported.",
            result.files,
            result.declarations,
            result.working_tree_changes,
            result.static_require_edges
        );
        Ok(())
    } else if command == "check-progress" {
        let result = check_progress(&root).map_err(|error| error.to_string())?;
        println!(
            "progress ledger validated: {} frozen declarations; {} source-derived declarations; \
            {} Rust counterpart mappings; {} assertion links.",
            result.declarations,
            result.source_derived_declarations,
            result.rust_counterparts,
            result.assertions
        );
        if result.collection_present {
            if result.raw_runtime_accounting {
                println!(
                    "{} registered runtime cases; {} linked cases; {} unmatched runtime cases; {} unresolved declarations.",
                    result.registered_runtime_cases,
                    result.linked_runtime_cases,
                    result.unmatched_runtime_cases,
                    result.unresolved_declarations
                );
            } else {
                println!(
                    "{} registered runtime case links; {} unresolved declarations. Legacy collection lacks raw runtime complement validation.",
                    result.linked_runtime_cases, result.unresolved_declarations
                );
            }
            if result.reconciliation_complete {
                println!(
                    "runtime registration reconciled; registration did not execute source tests"
                );
            } else {
                println!(
                    "runtime reconciliation incomplete: partial registration or collection blockers"
                );
            }
        } else {
            println!(
                "runtime reconciliation incomplete: collection absent; registered case count unavailable; \
                {} declarations unresolved",
                result.unresolved_declarations
            );
        }
        println!(
            "full parity incomplete; checked assertion links do not certify execution or complete behavioral parity"
        );
        Ok(())
    } else if command == "check-compiler-lineage" {
        let result = check_compiler_lineage(&root).map_err(|error| error.to_string())?;
        println!(
            "compiler proposals verified separately: {} raw cases; {} candidates; {} unmatched cases; {} unresolved declarations; {} module versions; {} trace events. Registration and compiler lineage do not certify execution or full parity.",
            result.registered_cases,
            result.candidate_origins,
            result.unresolved_runtime_cases,
            result.unresolved_declarations,
            result.module_versions,
            result.macro_events
        );
        Ok(())
    } else if command == "require-parity" {
        require_parity(&root).map_err(|error| error.to_string())
    } else {
        Err(usage("unknown subcommand"))
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
