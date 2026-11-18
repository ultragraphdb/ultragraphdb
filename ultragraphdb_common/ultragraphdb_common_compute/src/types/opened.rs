/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::partition_rows::PartitionRows;

/// The partitions a read goes on reading, and how many it has reached.
pub(crate) struct Opened {
    pub(crate) parts: Vec<PartitionRows>,
    pub(crate) reached: usize,
}
