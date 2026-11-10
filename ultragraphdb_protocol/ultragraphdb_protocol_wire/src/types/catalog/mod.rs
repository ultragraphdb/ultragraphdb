/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod catalog_body_builder;

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::header::Header;
use crate::utils::continuation::{Cut, continuation_frames};

/// A tenant's catalog version from the metadata group (`PROTOCOL.md` §13.7): its number, the epoch it applies from and
/// its bytes, carried opaque. A body with `position` 0 is a request.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `tenant` | u64 |
/// | 8 | `version` | u64: the catalog version, 0 in a request for the current one; at least 1 in a reply |
/// | 16 | `epoch` | epoch: the epoch from which the version applies, 0 while it is pending; 0 in a request |
/// | 24 | `position` | u64: the metadata group's log position the version was read at, from 1; 0 in a request |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the version's bytes; empty in a request |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    tenant: u64,
    version: u64,
    epoch: u64,
    position: u64,
    bytes: Vec<u8>,
}

impl Catalog {
    /// A request for `tenant`'s catalog `version`, 0 for its current one.
    pub fn request(tenant: u64, version: u64) -> Self {
        Self {
            tenant,
            version,
            epoch: 0,
            position: 0,
            bytes: Vec::new(),
        }
    }

    /// A reply: `tenant`'s catalog `version`, applying from `epoch` (0 while pending), read at `position`, holding
    /// `bytes`.
    pub fn reply(tenant: u64, version: u64, epoch: u64, position: u64, bytes: Vec<u8>) -> Self {
        Self {
            tenant,
            version,
            epoch,
            position,
            bytes,
        }
    }

    /// The tenant.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The catalog version; 0 in a request for the current one.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The epoch from which the version applies; 0 while pending, and in a request.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The metadata group's log position; 0 in a request.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.position
    }

    /// The version's bytes; empty in a request.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The reply as the frames that carry it within `max_frame` (`PROTOCOL.md` §13.7): one frame when its bytes fit,
    /// else as many as they take, each with this command header, the last alone flagged `LAST`. `header` gives every
    /// frame's other fields.
    ///
    /// # Errors
    ///
    /// [`ultragraphdb_error_protocol::ProtocolError::frame_too_large`] when a frame within `max_frame` holds no byte of
    /// the version left, naming the least frame that holds one.
    pub fn frames(&self, header: Header, max_frame: usize) -> Result<Vec<Vec<u8>>, ProtocolError> {
        let header = header
            .with_command(Command::Catalog)
            .with_command_header(self.command_header());
        continuation_frames(header, max_frame, &[(&self.bytes, Cut::Units(1))])
    }
}
