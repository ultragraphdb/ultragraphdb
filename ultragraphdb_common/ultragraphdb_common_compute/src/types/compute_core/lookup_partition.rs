/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;

use crate::types::link::Link;

/// A lookup's one partition, its version chosen by the first Scan.
#[derive(Debug)]
pub(super) struct LookupPartition {
    pub(crate) link: Rc<Link>,
    pub(crate) version: u64,
}
