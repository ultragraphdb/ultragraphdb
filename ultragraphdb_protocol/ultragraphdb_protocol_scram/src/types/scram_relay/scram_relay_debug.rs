/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::scram_relay::{Exchange, RelayState, ScramRelay};

impl fmt::Debug for ScramRelay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScramRelay")
            .field("nonce", &format_args!("<{} bytes>", self.nonce.len()))
            .field("state", &self.state)
            .finish()
    }
}

impl fmt::Debug for RelayState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initial => f.write_str("Initial"),
            Self::Read {
                binding,
                client_first_bare,
                nonce,
            } => f
                .debug_struct("Read")
                .field("binding", &format_args!("<{} bytes>", binding.len()))
                .field(
                    "client_first_bare",
                    &format_args!("<{} bytes>", client_first_bare.len()),
                )
                .field("nonce", &format_args!("<{} bytes>", nonce.len()))
                .finish(),
            Self::Challenged { exchange, answered } => f
                .debug_struct("Challenged")
                .field("exchange", exchange)
                .field("answered", answered)
                .finish(),
            Self::Done => f.write_str("Done"),
        }
    }
}

impl fmt::Debug for Exchange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Exchange")
            .field("binding", &format_args!("<{} bytes>", self.binding.len()))
            .field(
                "client_first_bare",
                &format_args!("<{} bytes>", self.client_first_bare.len()),
            )
            .field("nonce", &format_args!("<{} bytes>", self.nonce.len()))
            .field(
                "server_first",
                &format_args!("<{} bytes>", self.server_first.len()),
            )
            .finish()
    }
}
