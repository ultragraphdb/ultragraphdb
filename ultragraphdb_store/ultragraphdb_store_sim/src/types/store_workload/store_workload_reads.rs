/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::{Scan, StoreSnapshot};

use crate::types::sim_snapshot::SimSnapshot;

/// A read a reader made through its open snapshot, kept so that it can make it again: a point read, a many-key read,
/// or a scan, a key's versions included.
#[derive(Debug, Clone)]
pub enum ReadShape {
    Get(Vec<u8>),
    GetMany(Vec<Vec<u8>>),
    Scan(Scan),
}

impl ReadShape {
    /// Makes the read through `snapshot`, which logs its answer, and says whether it failed.
    pub fn issue(&self, snapshot: &SimSnapshot) -> bool {
        match self {
            ReadShape::Get(key) => snapshot.get(key).is_err(),
            ReadShape::GetMany(keys) => snapshot.get_many(keys).is_err(),
            ReadShape::Scan(scan) => snapshot.scan(scan).any(|item| item.is_err()),
        }
    }
}
