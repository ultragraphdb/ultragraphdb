/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use ultragraphdb_common_config_check::{TargetFiles, instances};

/// One stored revision: its files as received, and each instance's merged settings rendered once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Revision {
    pub(crate) files: Vec<u8>,
    pub(crate) instances: BTreeMap<String, String>,
}

impl Revision {
    /// The revision of `files`, a stored bundle of `target`. A record's bytes were checked before they were stored, so
    /// a body that no longer reads as a bundle holds no instance, and the start's check says so by name.
    pub(crate) fn new(target: &str, files: &[u8]) -> Self {
        let instances = TargetFiles::from_bundle(target, files)
            .map(|read| {
                instances(&read)
                    .into_iter()
                    .map(|instance| (instance.name().to_string(), instance.rendered().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            files: files.to_vec(),
            instances,
        }
    }
}
