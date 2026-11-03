/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod default_entry_view_body_view;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;

/// The body of a received DefaultEntry frame, read in place; see [`crate::DefaultEntry`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultEntryView<'a> {
    namespace: u64,
    found: bool,
    predicate: &'a str,
}

impl<'a> DefaultEntryView<'a> {
    /// The namespace.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// Whether the record already existed; false in a request.
    pub fn found(&self) -> bool {
        self.found
    }

    /// The predicate's IRI, borrowed from the frame.
    pub fn predicate(&self) -> &'a str {
        self.predicate
    }

    /// The body as the request the frontend receives: refused as [`ProtocolError::field_value`] (`"found"`, 1) when
    /// `found` is set.
    pub fn check_request(&self) -> Result<(), ProtocolError> {
        if self.found {
            return Err(ProtocolError::field_value(
                Command::DefaultEntry.number(),
                "found",
                1,
            ));
        }
        Ok(())
    }
}
