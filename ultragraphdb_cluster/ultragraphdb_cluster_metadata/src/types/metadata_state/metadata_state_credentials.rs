/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{Credentials, ScramVerifier};

use crate::types::metadata_state::MetadataState;

impl Credentials for MetadataState {
    /// The principal named `name`, as SASLprep prepared it, with its identity and verifier.
    fn lookup(&self, name: &str) -> Option<(u64, ScramVerifier)> {
        self.principals
            .get(name)
            .map(|principal| (principal.id(), principal.verifier().clone()))
    }
}
