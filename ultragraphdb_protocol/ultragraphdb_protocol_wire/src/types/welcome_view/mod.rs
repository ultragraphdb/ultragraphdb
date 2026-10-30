/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod welcome_view_body_view;

use crate::types::string_list::StringList;

/// The body of a received Welcome frame, read in place; see [`crate::Welcome`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WelcomeView<'a> {
    protocol: u16,
    authenticates: bool,
    release: u32,
    max_frame: u32,
    max_streams: u32,
    initial_credit: u32,
    max_write_set: u32,
    transaction_ms: u32,
    server: &'a str,
    mechanisms: StringList<'a>,
}

impl<'a> WelcomeView<'a> {
    pub fn protocol(&self) -> u16 {
        self.protocol
    }

    /// Whether the connection authenticates before its first request, the header's `auth`.
    pub fn authenticates(&self) -> bool {
        self.authenticates
    }

    pub fn release(&self) -> u32 {
        self.release
    }

    pub fn max_frame(&self) -> u32 {
        self.max_frame
    }

    pub fn max_streams(&self) -> u32 {
        self.max_streams
    }

    pub fn initial_credit(&self) -> u32 {
        self.initial_credit
    }

    pub fn max_write_set(&self) -> u32 {
        self.max_write_set
    }

    pub fn transaction_ms(&self) -> u32 {
        self.transaction_ms
    }

    pub fn server(&self) -> &'a str {
        self.server
    }

    /// The SASL mechanisms the server accepts, in its order; empty without authentication.
    pub fn mechanisms(&self) -> &StringList<'a> {
        &self.mechanisms
    }
}
