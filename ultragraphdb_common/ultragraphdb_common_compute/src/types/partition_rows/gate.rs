/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::scan_stream::ScanStream;

/// A dictionary Scan under way, for the terms of one partition.
#[derive(Debug)]
pub(crate) struct Gate {
    pub(crate) stream: Option<ScanStream>,
    pub(crate) terms: Vec<(Vec<u8>, String)>,
}
