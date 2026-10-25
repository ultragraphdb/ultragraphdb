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

    pub fn data(&self) -> Size {
        self.data
    }

    pub fn journal(&self) -> Size {
        self.journal
    }

    pub fn metadata(&self) -> Size {
        self.metadata
    }

    pub fn total(&self) -> Size {
        self.data.plus(self.journal).plus(self.metadata)
    }
}

/// Calls `f` with every file under `dir` and its size; `du`'s allocated size is the 512-byte blocks.
fn walk(dir: &Path, f: &mut impl FnMut(&Path, Size)) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_dir() {
            walk(&entry.path(), f)?;
        } else {
            f(&entry.path(), Size::new(meta.len(), meta.blocks() * 512));
        }
    }
    Ok(())
}
