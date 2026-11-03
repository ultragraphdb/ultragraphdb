/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::catalog::Catalog;
use crate::types::catalog_view::CatalogView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::utils::fields::{at_least_one, epoch};
use crate::utils::sections::{Sections, expect_command};

impl<'a> BodyView<'a> for CatalogView<'a> {
    const COMMAND: Command = Command::Catalog;

    type Body = Catalog;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 32)])?;
        let (version, effective, position) =
            (header.u64_at(8), header.u64_at(16), header.u64_at(24));
        let request = position == 0;
        if request && effective != 0 {
            return Err(ProtocolError::field_value(command, "catalog", effective));
        }
        if !request {
            let _version = at_least_one(command, "version", version)?;
            let _epoch = epoch(command, "epoch", effective, true)?;
        }
        let sections = Sections::<1>::read(command, frame.body(), 0)?;
        let bytes = sections.get(0);
        if request && !bytes.is_empty() {
            return Err(ProtocolError::count_mismatch(
                command,
                "catalog",
                bytes.len() as u64,
                0,
            ));
        }
        Ok(Self {
            namespace: header.u64_at(0),
            version,
            epoch: effective,
            position,
            bytes,
        })
    }

    fn to_body(&self) -> Result<Catalog, ProtocolError> {
        Ok(if self.is_request() {
            Catalog::request(self.namespace, self.version)
        } else {
            Catalog::reply(
                self.namespace,
                self.version,
                self.epoch,
                self.position,
                self.bytes.to_vec(),
            )
        })
    }
}
