//! Bounded, portable paths and artifact reads; snapshots may not contain symlinks.

use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::CheckError;

pub(crate) fn digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

pub(crate) fn valid_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn portable_path(relative: &str) -> Result<(), CheckError> {
    if relative.is_empty()
        || relative.contains(['\\', ':'])
        || relative.chars().any(char::is_control)
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(CheckError::new(format!(
            "unsafe artifact path: {relative:?}"
        )));
    }
    Ok(())
}

pub(crate) fn checked_path(root: &Path, relative: &str) -> Result<PathBuf, CheckError> {
    portable_path(relative)?;
    let mut path = root.to_path_buf();
    for component in relative.split('/') {
        path.push(component);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| CheckError::new(format!("cannot inspect {relative}: {error}")))?;
        if metadata.file_type().is_symlink() {
            return Err(CheckError::new(format!(
                "symlink in artifact path: {relative}"
            )));
        }
    }
    Ok(path)
}

pub(crate) fn read(root: &Path, relative: &str, limit: u64) -> Result<Vec<u8>, CheckError> {
    let path = checked_path(root, relative)?;
    let file = File::open(&path)
        .map_err(|error| CheckError::new(format!("cannot open {relative}: {error}")))?;
    if !file
        .metadata()
        .map_err(|error| CheckError::new(error.to_string()))?
        .is_file()
    {
        return Err(CheckError::new(format!(
            "artifact is not a regular file: {relative}"
        )));
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| CheckError::new(format!("cannot read {relative}: {error}")))?;
    if u64::try_from(bytes.len()).map_err(|error| CheckError::new(error.to_string()))? > limit {
        return Err(CheckError::new(format!(
            "artifact exceeds {limit} bytes: {relative}"
        )));
    }
    Ok(bytes)
}

pub(crate) fn snapshot_paths(root: &Path, relative: &str) -> Result<BTreeSet<String>, CheckError> {
    let mut paths = BTreeSet::new();
    visit(root, relative, &mut paths)?;
    Ok(paths)
}

fn visit(root: &Path, relative: &str, paths: &mut BTreeSet<String>) -> Result<(), CheckError> {
    let entries = fs::read_dir(checked_path(root, relative)?)
        .map_err(|error| CheckError::new(format!("cannot list {relative}: {error}")))?;
    for entry in entries {
        let entry = entry.map_err(|error| CheckError::new(error.to_string()))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| CheckError::new("non-UTF8 artifact filename"))?;
        let child = format!("{relative}/{name}");
        portable_path(&child)?;
        let kind = entry
            .file_type()
            .map_err(|error| CheckError::new(error.to_string()))?;
        if kind.is_dir() {
            visit(root, &child, paths)?;
        } else if kind.is_file() {
            paths.insert(child);
        } else {
            return Err(CheckError::new(format!(
                "non-file or symlink in snapshot: {child}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{error::Error, fs};

    use wf_test_support::resources::TestResources;

    use super::{checked_path, digest, portable_path, read, snapshot_paths};

    #[test]
    fn sha256_matches_independent_standard_vectors() {
        assert_eq!(
            digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn unsafe_portable_paths_are_rejected_before_filesystem_access() {
        for path in [
            "",
            "/absolute",
            "../escape",
            "a/../b",
            "a/./b",
            "a//b",
            "a/",
            "C:/escape",
            "a\\b",
            "a\0b",
            "a\nb",
        ] {
            assert!(
                portable_path(path).is_err(),
                "accepted unsafe path {path:?}"
            );
        }
        assert!(portable_path("apps/test/section.exs").is_ok());
    }

    #[test]
    fn file_reads_are_bounded_and_directories_are_not_artifacts() -> Result<(), Box<dyn Error>> {
        let resources = TestResources::new()?;
        fs::write(resources.path().join("bytes"), b"012345")?;
        assert_eq!(read(resources.path(), "bytes", 6)?, b"012345");
        assert!(read(resources.path(), "bytes", 5).is_err());
        fs::create_dir(resources.path().join("directory"))?;
        assert!(read(resources.path(), "directory", 6).is_err());
        assert!(checked_path(resources.path(), "missing").is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symlink_files_and_parent_directories_are_rejected() -> Result<(), Box<dyn Error>> {
        use std::os::unix::fs::symlink;

        let resources = TestResources::new()?;
        fs::create_dir(resources.path().join("snapshot"))?;
        fs::write(resources.path().join("snapshot/real"), b"exact")?;
        symlink("real", resources.path().join("snapshot/link"))?;
        symlink("snapshot", resources.path().join("parent-link"))?;
        assert!(checked_path(resources.path(), "snapshot/link").is_err());
        assert!(checked_path(resources.path(), "parent-link/real").is_err());
        assert!(snapshot_paths(resources.path(), "snapshot").is_err());
        Ok(())
    }
}
