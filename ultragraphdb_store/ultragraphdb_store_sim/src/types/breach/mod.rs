/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod breach_display;

use crate::types::invariant::Invariant;

/// An invariant the shadow model found broken, and how.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Breach {
    invariant: Invariant,
    detail: String,
}

impl Breach {
    pub fn new(invariant: Invariant, detail: impl Into<String>) -> Self {
        Self {
            invariant,
            detail: detail.into(),
        }
    }

    pub fn invariant(&self) -> Invariant {
        self.invariant
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}
