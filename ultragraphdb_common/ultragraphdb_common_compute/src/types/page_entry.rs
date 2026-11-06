/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::QuadKey;

/// One entry a page's rows came from.
#[derive(Debug)]
pub(super) struct PageEntry {
    pub(crate) key: Vec<u8>,
    pub(crate) parsed: QuadKey,
}
