/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

/// The error of a [`crate::TestCatalogSource`] made unreachable: the metadata group cannot be asked now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CatalogUnreachable;

impl fmt::Display for CatalogUnreachable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the metadata group cannot be reached")
    }
}

impl std::error::Error for CatalogUnreachable {}
