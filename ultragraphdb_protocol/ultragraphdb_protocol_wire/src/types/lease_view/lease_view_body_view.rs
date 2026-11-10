/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::marker::PhantomData;

use ultragraphdb_core_body::Sections;
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::lease::Lease;
use crate::types::lease_view::LeaseView;
use crate::utils::body::read_sections;
use crate::utils::command_header::expect_command;
use crate::utils::fields::at_least_one;

impl<'a> BodyView<'a> for LeaseView<'a> {
    const COMMAND: Command = Command::Lease;

    type Body = Lease;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 2), (4, 32)])?;
        let type_prefix = header.u16_at(0);
        let _type = at_least_one(command, "type", type_prefix.into())?;
        let partition = header.u32_at(4);
        let _partition = at_least_one(command, "partition", partition.into())?;
        let size = at_least_one(command, "size", header.u64_at(8))?;
        let (start, end) = (header.u64_at(16), header.u64_at(24));
        let request = start == 0 && end == 0;
        if !request && (start == 0 || start.checked_add(size) != Some(end)) {
            return Err(ProtocolError::field_value(command, "block", start));
        }
        let _sections: Sections<'_, 0> = read_sections(command, frame.body(), 0)?;
        Ok(Self {
            frame: PhantomData,
            type_prefix,
            partition,
            size,
            start,
            end,
        })
    }

    fn to_body(&self) -> Result<Lease, ProtocolError> {
        let lease = Lease::request(self.type_prefix, self.partition, self.size);
        Ok(if self.is_request() {
            lease
        } else {
            lease.with_block(self.start)
        })
    }
}
