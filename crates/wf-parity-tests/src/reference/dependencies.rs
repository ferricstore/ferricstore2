//! Separately preserved workflow dependencies from the same pinned source commit.
//! These files add no invented primary declaration or native execution evidence.

use std::path::Path;

use crate::{CheckError, artifact};

pub(super) fn check(root: &Path) -> Result<(), CheckError> {
    for (path, expected) in [
        (
            "tests/reference/elixir-dependencies/apps/ferricstore/lib/ferricstore/term_codec.ex",
            "fa1f00dfb407569a19b0f36760fec83ba10defeab7178ba7154ed4227d3f5ee6",
        ),
        (
            "tests/reference/elixir-dependencies/apps/ferricstore/test/ferricstore/term_codec_test.exs",
            "b7ea7302f72bfa11cab66b0832890cd0fb215a7ccaa3d6c9791f1a23f37077d3",
        ),
    ] {
        let bytes = artifact::read(root, path, 8_192).map_err(|error| {
            CheckError::new(format!(
                "workflow dependency snapshot cannot be read: {path}: {error}"
            ))
        })?;
        if artifact::digest(&bytes) != expected {
            return Err(CheckError::new(format!(
                "workflow dependency snapshot differs: {path}"
            )));
        }
    }
    Ok(())
}
