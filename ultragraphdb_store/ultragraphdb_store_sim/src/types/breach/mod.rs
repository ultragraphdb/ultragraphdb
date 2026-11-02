/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod breach_display;
mod breach_error;

use crate::types::invariant::Invariant;

/// An invariant the shadow model found broken, and how.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Breach {
    invariant: Invariant,
    detail: String,
}

impl Breach {
    /// A breach of `invariant`, described by `detail`.
    pub fn new(invariant: Invariant, detail: impl Into<String>) -> Self {
        Self {
            invariant,
            detail: detail.into(),
        }
    }

    /// The invariant broken.
    pub fn invariant(&self) -> Invariant {
        self.invariant
    }

    /// How the invariant was broken.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}
