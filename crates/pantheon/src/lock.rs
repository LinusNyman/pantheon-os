//! The record write primitive (§6.4). Every record write takes an advisory lock
//! (`fd-lock`) on the record file itself and writes via temp-file-and-rename. Since
//! the rename swaps the inode, a writer that acquires the lock re-opens the path and
//! confirms it still holds the file now there — retrying if another writer's rename
//! slipped in first — before its read-modify-write. The lock lives on the tree file,
//! not a sidecar: there is no cache dir (§18).

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use fd_lock::RwLock;

use crate::{Error, Result};

/// Run `edit` under an exclusive advisory lock on `path`, writing its result via
/// temp-file-and-rename. `edit` receives the file's current bytes, or `None` if the
/// record does not yet exist. The record file (and, on unix, the inode it names) is
/// the lock's whole scope.
pub fn with_record_lock<F>(path: &Path, edit: F) -> Result<()>
where
    F: FnOnce(Option<&[u8]>) -> Result<Vec<u8>>,
{
    let parent = path
        .parent()
        .ok_or_else(|| Error::runtime("record path has no parent directory"))?;

    #[cfg(unix)]
    {
        // Acquire the lock, then confirm the path still names the inode we locked;
        // a rename by another writer invalidates it, so re-open and try again.
        loop {
            let existed = path.exists();
            let file = open_for_lock(path)?;
            let locked_ino = inode_of(&file.metadata()?);
            let mut lock = RwLock::new(file);
            let guard = lock
                .write()
                .map_err(|e| Error::runtime(format!("record lock: {e}")))?;
            let current_ino = std::fs::metadata(path).ok().map(|m| inode_of(&m));
            if current_ino == Some(locked_ino) {
                return do_write(&guard, existed, parent, path, edit);
            }
            drop(guard);
        }
    }

    #[cfg(not(unix))]
    {
        let existed = path.exists();
        let file = open_for_lock(path)?;
        let mut lock = RwLock::new(file);
        let guard = lock
            .write()
            .map_err(|e| Error::runtime(format!("record lock: {e}")))?;
        do_write(&guard, existed, parent, path, edit)
    }
}

fn open_for_lock(path: &Path) -> Result<File> {
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(Error::from)
}

fn do_write<F>(file: &File, existed: bool, parent: &Path, path: &Path, edit: F) -> Result<()>
where
    F: FnOnce(Option<&[u8]>) -> Result<Vec<u8>>,
{
    let mut buf = Vec::new();
    let mut reader = file;
    reader.read_to_end(&mut buf)?;
    let prev = if existed { Some(buf.as_slice()) } else { None };
    let new_bytes = edit(prev)?;
    write_via_temp(parent, path, &new_bytes)
}

fn write_via_temp(parent: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    let stem = path.file_name().map_or_else(
        || "record".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    // Writers serialize on the advisory lock, so a pid-tagged temp name never clashes.
    let tmp = parent.join(format!(".{stem}.{}.tmp", std::process::id()));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path).map_err(Error::from)
}

#[cfg(unix)]
fn inode_of(m: &std::fs::Metadata) -> u64 {
    std::os::unix::fs::MetadataExt::ino(m)
}
