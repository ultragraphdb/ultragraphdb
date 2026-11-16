/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Each partition's store, opened from a copy of an empty store made once in the process. A store made new creates its
//! keyspaces, each synced into being with its directory, a full flush to the disk each on macOS, and that was nine tenths of
//! what a core's test spent, measured 2026-11-16. A closed store copied whole is the store it was, so each test still opens an
//! empty store of its own, as the role opens one, by fjall's recovery in place of its creation. The template is held in memory,
//! its directory removed once read, so nothing of it outlives the process under Cargo or Bazel.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_tool_test::TestDir;

/// An entry of the template under its root: a directory, or a file and its bytes.
type Entry = (PathBuf, Option<Vec<u8>>);

/// The empty store every test's store is copied from, made, closed and read by the first test to need it, or why it was not.
static TEMPLATE: OnceLock<Result<Vec<Entry>, String>> = OnceLock::new();

/// An empty store in `dir`, which must not hold one yet, opened as the storage role opens it.
///
/// # Errors
///
/// A [`TestError`] when the template is not made, the copy fails or the store does not open.
pub(crate) fn empty_store(dir: &Path) -> TestResult<FjallStore> {
    let template = TEMPLATE
        .get_or_init(|| made().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| TestError::new(format!("the template store: {e}")))?;
    std::fs::create_dir_all(dir)?;
    // Each directory comes before what it holds, as the template was read.
    for (path, bytes) in template {
        let target = dir.join(path);
        match bytes {
            Some(bytes) => std::fs::write(target, bytes)?,
            None => std::fs::create_dir_all(target)?,
        }
    }
    Ok(FjallStore::open(dir, Durability::Log)?)
}

/// An empty store made and closed, so its files are whole, then read: its entries. Its directory goes with the [`TestDir`].
fn made() -> TestResult<Vec<Entry>> {
    let made = TestDir::new("store-template");
    drop(FjallStore::open(made.path(), Durability::Log)?);
    let mut entries = Vec::new();
    read_tree(made.path(), Path::new(""), &mut entries)?;
    Ok(entries)
}

/// Reads the directory `root.join(under)` into `entries`, every file and directory under it, each directory before what it holds.
fn read_tree(root: &Path, under: &Path, entries: &mut Vec<Entry>) -> TestResult {
    for entry in std::fs::read_dir(root.join(under))? {
        let entry = entry?;
        let path = under.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            entries.push((path.clone(), None));
            read_tree(root, &path, entries)?;
        } else {
            entries.push((path, Some(std::fs::read(entry.path())?)));
        }
    }
    Ok(())
}
