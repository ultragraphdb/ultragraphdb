/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod drop_record_codec;

use ultragraphdb_traits_store::Entry;

/// A drop in progress, as the metadata keyspace holds it until the drop is finished: its timestamp, its prefixes,
/// or none for a drop of everything, and the entries it writes once they are empty. A store that finds one at open
/// finishes the drop before it serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropRecord {
    pub ts: u64,
    pub prefixes: Option<Vec<Vec<u8>>>,
    pub entries: Vec<Entry>,
}
