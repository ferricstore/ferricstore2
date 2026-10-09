//! Owned temporary resources for isolated tests and same-directory reopen.

use std::{io, path::Path};

/// An isolated directory owned by the test controller, not a killable child.
///
/// Drop attempts cleanup. Use [`Self::close`] to observe cleanup failures.
/// Keep this owner alive while a process restarts against the borrowed path.
#[derive(Debug)]
pub struct TestResources {
    directory: tempfile::TempDir,
}

impl TestResources {
    /// Create an isolated directory in the platform temporary directory.
    ///
    /// # Errors
    /// Returns the operating system error if directory creation fails.
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            directory: tempfile::Builder::new().prefix("wf-test-").tempdir()?,
        })
    }

    /// Create an isolated directory inside an existing test-owned parent.
    ///
    /// # Errors
    /// Returns an error if the parent is unavailable or creation fails.
    pub fn in_parent(parent: &Path) -> io::Result<Self> {
        Ok(Self {
            directory: tempfile::Builder::new()
                .prefix("wf-test-")
                .tempdir_in(parent)?,
        })
    }

    /// Borrow the resource path without transferring directory ownership.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.directory.path()
    }

    /// Remove all owned resources and report cleanup failure.
    ///
    /// # Errors
    /// Returns the operating system error if removal fails.
    pub fn close(self) -> io::Result<()> {
        self.directory.close()
    }
}
