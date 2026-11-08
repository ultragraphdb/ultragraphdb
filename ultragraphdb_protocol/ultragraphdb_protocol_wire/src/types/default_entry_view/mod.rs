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
    tenant: u64,
    found: bool,
    predicate: &'a str,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<DefaultEntryView<'static>>() <= 64);

impl<'a> DefaultEntryView<'a> {
    /// The tenant.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// Whether the record already existed; false in a request.
    #[must_use]
    pub fn found(&self) -> bool {
        self.found
    }

    /// The predicate's IRI, borrowed from the frame.
    #[must_use]
    pub fn predicate(&self) -> &'a str {
        self.predicate
    }

    /// The body as the request the frontend receives.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::field_value`] (`"found"`, 1) when `found` is set.
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
