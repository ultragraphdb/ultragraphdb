/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod shapes_pass;

use std::collections::BTreeMap;

use crate::types::label_shape::LabelShape;

/// The shapes of a namespace's labels, as one pass over a snapshot found them: each label, an entity type's IRI,
/// with what its nodes hold, and the snapshot's timestamp. A pass replaces them whole; no write reads or updates them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Shapes {
    read_ts: u64,
    labels: BTreeMap<String, LabelShape>,
}

impl Shapes {
    pub fn read_ts(&self) -> u64 {
        self.read_ts
    }

    pub fn labels(&self) -> &BTreeMap<String, LabelShape> {
        &self.labels
    }
}
