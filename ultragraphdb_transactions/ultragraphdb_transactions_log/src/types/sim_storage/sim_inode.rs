/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

/// A file or directory of the simulated filesystem, with what a read sees now and what a power loss leaves.
#[derive(Debug, Clone)]
pub(crate) enum SimInode {
    File {
        buffered: Vec<u8>,
        stable: Vec<u8>,
    },
    Directory {
        entries: BTreeMap<String, usize>,
        stable: BTreeMap<String, usize>,
    },
}

impl SimInode {
    pub(crate) fn file(bytes: Vec<u8>) -> Self {
        Self::File {
            buffered: bytes.clone(),
            stable: bytes,
        }
    }

    pub(crate) fn empty_file() -> Self {
        Self::file(Vec::new())
    }

    pub(crate) fn directory() -> Self {
        Self::Directory {
            entries: BTreeMap::new(),
            stable: BTreeMap::new(),
        }
    }

    /// Back to the stable view: a power loss.
    pub(crate) fn restore(&mut self) {
        match self {
            Self::File { buffered, stable } => buffered.clone_from(stable),
            Self::Directory { entries, stable } => entries.clone_from(stable),
        }
    }
}
