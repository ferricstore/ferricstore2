//! CLI entrypoint for M0 preservation and the incomplete full-parity gate.

#![forbid(unsafe_code)]

use std::{env, path::PathBuf, process::ExitCode};

use wf_parity_tests::{check_reference, require_parity, workspace_root};

fn usage(message: &str) -> String {
    format!("{message}; usage: wf-parity-tests <check-reference|require-parity> [--root PATH]")
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
