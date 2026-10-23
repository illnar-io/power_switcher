//! Private, atomic writes for user data.
//!
//! Config, snippets, learned words, exceptions and statistics are personal:
//! snippets in particular may contain addresses, tokens or other text the
//! user pasted into a trigger.  Files are written `0600` through a sibling
//! temporary file so a crash cannot leave a half-written store, and newly
//! created directories are `0700`.
//!
//! Existing directories are deliberately *not* chmodded here: the stores may
//! point at a user-chosen path (for example `/tmp/my-snippets.json`) and a
//! blanket `chmod` on its parent would be destructive.  The daemon and GUI
//! harden their own default config directory explicitly.

use std::fs::{self, DirBuilder, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub const FILE_MODE: u32 = 0o600;
pub const DIR_MODE: u32 = 0o700;

/// Create `path` and any missing parents with mode `0700`.  Existing
/// directories keep their mode; only components created here are private.
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    let mut builder = DirBuilder::new();
    builder.recursive(true).mode(DIR_MODE);
    builder.create(path)
}

/// Tighten an application-owned directory to `0700`.  Best effort, and never
/// follows a symlink (chmodding the target would touch an unrelated file).
pub fn harden_private_dir(path: &Path) {
    harden(path, DIR_MODE);
}

/// Tighten a data file to `0600`.  Best effort, and never follows a symlink.
pub fn harden_private_file(path: &Path) {
    harden(path, FILE_MODE);
}

fn harden(path: &Path, mode: u32) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    if meta.file_type().is_symlink() {
        return;
    }
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
}

/// Read a user data file and tighten its permissions if it already exists.
pub fn read_private(path: &Path) -> io::Result<String> {
    let text = fs::read_to_string(path)?;
    harden_private_file(path);
    Ok(text)
}

/// Atomically replace `path` with `contents`, mode `0600`.
pub fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    create_private_dir(&parent)?;

    let mut tmp = tempfile::NamedTempFile::new_in(&parent)?;
    tmp.as_file()
        .set_permissions(Permissions::from_mode(FILE_MODE))?;
    tmp.write_all(contents)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|err| err.error)?;
    harden_private_file(path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_private_files_and_new_directories() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("nested").join("words.json");
        write_private(&path, b"secret").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "secret");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            FILE_MODE
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            DIR_MODE
        );
    }

    #[test]
    fn overwrites_without_leaving_a_temp_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("stats.json");
        write_private(&path, b"one").unwrap();
        write_private(&path, b"two").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            FILE_MODE
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn hardening_does_not_follow_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        let link = root.path().join("link");
        fs::write(&target, "{}").unwrap();
        fs::set_permissions(&target, Permissions::from_mode(0o644)).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        harden_private_file(&link);

        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }

    #[test]
    fn read_hardens_an_existing_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("snippets.json");
        fs::write(&path, "{}").unwrap();
        fs::set_permissions(&path, Permissions::from_mode(0o644)).unwrap();

        assert_eq!(read_private(&path).unwrap(), "{}");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            FILE_MODE
        );
    }
}
