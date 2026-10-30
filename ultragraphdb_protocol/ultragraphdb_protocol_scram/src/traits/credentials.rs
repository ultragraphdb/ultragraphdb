/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::scram_verifier::ScramVerifier;

/// Where a server finds a principal's SCRAM verifier by the name a client authenticates with. Static dispatch: a
/// server names its store's type.
pub trait Credentials {
    /// The principal holding the SASLprep-prepared `name`, and its verifier; `None` when no principal holds it.
    fn lookup(&self, name: &str) -> Option<(u64, ScramVerifier)>;
}
