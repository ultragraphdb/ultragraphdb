/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::scram_client::{ClientState, ScramClient};

impl fmt::Debug for ScramClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScramClient")
            .field("name", &self.name)
            .field("password", &format_args!("<redacted>"))
            .field("nonce", &format_args!("<{} bytes>", self.nonce.len()))
            .field("state", &self.state)
            .finish()
    }
}

impl fmt::Debug for ClientState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initial => f.write_str("Initial"),
            Self::First { client_first_bare } => f
                .debug_struct("First")
                .field(
                    "client_first_bare",
                    &format_args!("<{} bytes>", client_first_bare.len()),
                )
                .finish(),
            Self::Final { .. } => f
                .debug_struct("Final")
                .field("server_signature", &format_args!("<redacted>"))
                .finish(),
            Self::Done => f.write_str("Done"),
        }
    }
}
