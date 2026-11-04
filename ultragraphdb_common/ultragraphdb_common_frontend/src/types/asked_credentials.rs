/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::rc::Rc;

use ultragraphdb_protocol_scram::{Credentials, ScramVerifier};

/// The verifier the owner gave for the name a connection is authenticating as, held for that exchange alone: the
/// connection's session fills it and the connection's machine reads it, without waiting.
#[derive(Debug, Clone, Default)]
pub struct AskedCredentials(Rc<RefCell<Option<(String, u64, ScramVerifier)>>>);

impl AskedCredentials {
    /// What the owner answered for `name`.
    pub fn answered(&self, name: String, held: Option<(u64, ScramVerifier)>) {
        *self.0.borrow_mut() = held.map(|(principal, verifier)| (name, principal, verifier));
    }
}

impl Credentials for AskedCredentials {
    fn lookup(&self, name: &str) -> Option<(u64, ScramVerifier)> {
        self.0
            .borrow_mut()
            .take()
            .filter(|(asked, ..)| asked == name)
            .map(|(_, principal, verifier)| (principal, verifier))
    }
}
