/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use ultragraphdb_protocol_scram::ScramError;
use ultragraphdb_protocol_transport::{MechanismStep, ServerMechanism};

use crate::types::relay_state::RelayState;

/// One relayed exchange, the transport's server mechanism: each step answers what the session prepared for it. It
/// shares its [`crate::RelayAuthenticator`]'s state by an `Rc` of a `RefCell`, on the connection's one task, a borrow
/// lasting one step (Ultra rule 9).
pub struct RelayMechanism(Rc<RefCell<RelayState>>);

impl RelayMechanism {
    /// The exchange answering from `state`, its authenticator's.
    pub(crate) fn over(state: Rc<RefCell<RelayState>>) -> Self {
        Self(state)
    }
}

impl ServerMechanism for RelayMechanism {
    type Error = ScramError;

    /// The answer the session prepared for this step; an exchange none was prepared for fails.
    fn step(&mut self, _message: &[u8]) -> Result<MechanismStep, ScramError> {
        self.0
            .borrow_mut()
            .answer
            .take()
            .unwrap_or_else(|| Err(ScramError::out_of_step()))
    }
}

impl fmt::Debug for RelayMechanism {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("RelayMechanism").finish_non_exhaustive()
    }
}
