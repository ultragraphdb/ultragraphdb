/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use ultragraphdb_protocol_scram::{MECHANISM, ScramError, ScramRelay};
use ultragraphdb_protocol_transport::{Authenticator, Credential, MechanismStep};

use crate::types::relay_mechanism::RelayMechanism;
use crate::types::relay_state::RelayState;

/// A client connection's authentication relayed to the system group (design D8; `PROTOCOL.md` §11.1): the connection's
/// session reads each client message with [`ScramRelay`], asks the system group by Challenge and Verify, and prepares the
/// server's message here before the connection's machine, which runs without waiting, takes it as the step's answer. It
/// holds no key: a verifier never reaches compute. Shared by an `Rc` of a `RefCell` between the session and the
/// machine on the connection's one task, a borrow lasting one call, never across a wait (Ultra rule 9). Its `Debug` holds
/// whether an answer waits and the relay's own redacted form, never a message's text.
///
/// Control plane (Ultra rule 3): an exchange runs once per connection, and each step allocates its messages.
#[derive(Clone, Default)]
pub struct RelayAuthenticator(Rc<RefCell<RelayState>>);

impl RelayAuthenticator {
    /// Reads the client-first-message `message` of a new exchange whose server nonce part is `nonce`: the name to ask
    /// the system group for.
    ///
    /// # Errors
    ///
    /// What [`ScramRelay::first`] refuses; the step then fails the exchange.
    pub fn first(&self, message: &[u8], nonce: &str) -> Result<String, ScramError> {
        let mut relay = ScramRelay::new(nonce);
        let mut state = self.0.borrow_mut();
        state.answer = None;
        state.name = None;
        let name = relay.first(message);
        state.relay = Some(relay);
        let name = name?;
        state.name = Some(name.clone());
        Ok(name)
    }

    /// The name the exchange in hand authenticates as.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.0.borrow().name.clone()
    }

    /// The system group's Challenge answered `salt` and `iterations`: the server-first-message is the next step's answer.
    pub fn challenged(&self, salt: &[u8], iterations: u32) {
        let mut state = self.0.borrow_mut();
        let answer = match state.relay.as_mut() {
            Some(relay) => relay
                .challenge(salt, iterations)
                .map(MechanismStep::Continue),
            None => Err(ScramError::out_of_step()),
        };
        state.answer = Some(answer);
    }

    /// Reads the client-final-message `message`: the AuthMessage and the ClientProof to send the system group in a Verify.
    ///
    /// # Errors
    ///
    /// What [`ScramRelay::last`] refuses; the step then fails the exchange.
    pub fn last(&self, message: &[u8]) -> Result<(String, [u8; 32]), ScramError> {
        let mut state = self.0.borrow_mut();
        state.answer = None;
        state
            .relay
            .as_mut()
            .ok_or_else(ScramError::out_of_step)?
            .last(message)
    }

    /// The system group's Verify held for `principal` with `signature`: the server-final-message is the next step's answer.
    pub fn verified(&self, principal: u64, signature: &[u8; 32]) {
        let mut state = self.0.borrow_mut();
        let answer = match state.relay.as_mut() {
            Some(relay) => relay
                .finish(signature)
                .map(|last| MechanismStep::Done(last, Credential::new(principal))),
            None => Err(ScramError::out_of_step()),
        };
        state.answer = Some(answer);
    }

    /// The system group refused the proof, or a message could not be read: the next step fails the exchange, with the
    /// authentication error that says nothing of why (`PROTOCOL.md` §11.1).
    pub fn refused(&self) {
        self.0.borrow_mut().answer = Some(Err(ScramError::proof()));
    }
}

impl Authenticator for RelayAuthenticator {
    type Mechanism = RelayMechanism;

    /// An exchange of `SCRAM-SHA-256`; `None` for any other mechanism.
    fn start(&mut self, mechanism: &str) -> Option<RelayMechanism> {
        (mechanism == MECHANISM).then(|| RelayMechanism::over(Rc::clone(&self.0)))
    }
}

impl fmt::Debug for RelayAuthenticator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.0.borrow();
        f.debug_struct("RelayAuthenticator")
            .field("relay", &state.relay)
            .field("named", &state.name.is_some())
            .field("answer", &state.answer.as_ref().map(Result::is_ok))
            .finish()
    }
}
