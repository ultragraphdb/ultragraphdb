/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{Credentials, ScramVerifier};

/// The client endpoint's principals, until the frontend hosts the metadata group that holds them: none, so every name
/// is refused as a wrong password is.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPrincipals;

impl Credentials for NoPrincipals {
    fn lookup(&self, _name: &str) -> Option<(u64, ScramVerifier)> {
        None
    }
}
