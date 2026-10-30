/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::transport_error::TransportError;
use crate::traits::client_mechanism::ClientMechanism;

/// What a client authenticates with: a fresh mechanism for each Authenticate exchange, the first and every
/// re-authentication, chosen from the mechanisms a Welcome lists. Static dispatch: a client names its login's type.
pub trait Login {
    /// One exchange of a mechanism.
    type Mechanism: ClientMechanism;

    /// A mechanism for one exchange, one of `offered`, the Welcome's; `Ok(None)` when the login speaks none of them,
    /// and [`TransportError::Authentication`] when it cannot start one.
    fn mechanism(&mut self, offered: &[String]) -> Result<Option<Self::Mechanism>, TransportError>;
}
