/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::server_mechanism::ServerMechanism;

/// What a server authenticates its connections with: a SASL exchange of the mechanism a client names, started from
/// the server's credentials. Static dispatch: a server names its authenticator's type.
pub trait Authenticator {
    /// One exchange of a mechanism.
    type Mechanism: ServerMechanism;

    /// An exchange of the mechanism named `mechanism`; `None` for a mechanism the server does not offer.
    fn start(&mut self, mechanism: &str) -> Option<Self::Mechanism>;
}
