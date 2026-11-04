/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod test_dir_drop;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The directories made so far in this process, which names the next: an atomic, since the test threads of one
/// binary make directories at once, and each needs a number of its own and nothing else.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// A directory of its own under Bazel's `TEST_TMPDIR`, or the system's temporary directory under Cargo.
#[derive(Debug)]
pub struct TestDir(PathBuf);

impl TestDir {
    /// A fresh directory for the test `name`, empty: one an earlier run left behind is removed.
    pub fn new(name: &str) -> Self {
        let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = root.join(format!("kv-{name}-{}-{n}", std::process::id()));
        // A directory an earlier run left behind, which is usually absent.
        drop(std::fs::remove_dir_all(&dir));
        Self(dir)
    }

    /// The directory's path.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The fjall store in the directory, opened with process durability.
    ///
    /// # Errors
    ///
    /// A [`TestError`](ultragraphdb_tool_bdd::TestError) when the store does not open.
    pub fn open(&self) -> TestResult<FjallStore> {
        FjallStore::open(&self.0, Durability::Process).context("the store opens")
    }
}
