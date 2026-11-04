/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::types::size::Size;

/// A store's files by kind: the data keyspace's directory, fjall's journals (`*.jnl` at the top), and every other
/// file, the metadata keyspace and fjall's own version files among them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirSizes {
    data: Size,
    journal: Size,
    metadata: Size,
}

impl DirSizes {
    /// The sizes of the store in `dir`, whose data keyspace is `data`.
    ///
    /// # Errors
    ///
    /// A directory or file that cannot be read.
    pub fn of(dir: &Path, data: &Path) -> std::io::Result<Self> {
        let mut sizes = Self::default();
        walk(dir, &mut |path, size| {
            if path.starts_with(data) {
                sizes.data = sizes.data.plus(size);
            } else if path.parent() == Some(dir) && path.extension().is_some_and(|e| e == "jnl") {
                sizes.journal = sizes.journal.plus(size);
            } else {
                sizes.metadata = sizes.metadata.plus(size);
            }
        })?;
        Ok(sizes)
    }

    /// The data keyspace's directory.
    pub fn data(&self) -> Size {
        self.data
    }

    /// fjall's journals.
    pub fn journal(&self) -> Size {
        self.journal
    }

    /// Every other file: the metadata keyspace and fjall's own version files.
    pub fn metadata(&self) -> Size {
        self.metadata
    }

    /// The whole store.
    pub fn total(&self) -> Size {
        self.data.plus(self.journal).plus(self.metadata)
    }
}

/// The directories below a store's directory [`walk`] descends through at most; fjall nests a handful.
const DIR_DEPTH_MAX: usize = 16;

/// Calls `f` with every file under `dir` and its size; `du`'s allocated size is the 512-byte blocks. Walks with a
/// stack of the directories still to read, each with its depth, and refuses a directory deeper than
/// [`DIR_DEPTH_MAX`].
fn walk(dir: &Path, f: &mut impl FnMut(&Path, Size)) -> std::io::Result<()> {
    let mut pending = vec![(dir.to_path_buf(), 0usize)];
    // Each turn reads one directory, pushed once by its parent: an entry's metadata does not follow a symbolic link,
    // so no directory is reached twice, and the loop ends within the directories under `dir`.
    while let Some((path, depth)) = pending.pop() {
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            if !meta.is_dir() {
                f(&entry.path(), Size::new(meta.len(), meta.blocks() * 512));
            } else if depth < DIR_DEPTH_MAX {
                pending.push((entry.path(), depth + 1));
            } else {
                return Err(std::io::Error::other(format!(
                    "{} lies deeper than {DIR_DEPTH_MAX} directories below {}",
                    entry.path().display(),
                    dir.display()
                )));
            }
        }
    }
    Ok(())
}
