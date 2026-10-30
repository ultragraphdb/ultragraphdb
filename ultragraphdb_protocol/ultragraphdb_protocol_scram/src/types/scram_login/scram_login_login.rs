/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::{Login, TransportError};

use crate::errors::scram_error::ScramError;
use crate::types::scram_client::ScramClient;
use crate::types::scram_login::ScramLogin;
use crate::utils::constants::MECHANISM;

impl<N: FnMut() -> Result<String, ScramError>> Login for ScramLogin<N> {
    type Mechanism = ScramClient;

    /// A [`ScramClient`] with the next nonce when `offered` lists [`crate::MECHANISM`]; `Ok(None)` otherwise. A
    /// nonce the source fails to draw, or draws malformed, is [`TransportError::Authentication`].
    fn mechanism(&mut self, offered: &[String]) -> Result<Option<ScramClient>, TransportError> {
        if !offered.iter().any(|name| name == MECHANISM) {
            return Ok(None);
        }
        let refused = |error: ScramError| TransportError::Authentication(error.to_string());
        let nonce = self.next_nonce().map_err(refused)?;
        self.client(&nonce).map(Some).map_err(refused)
    }
}
