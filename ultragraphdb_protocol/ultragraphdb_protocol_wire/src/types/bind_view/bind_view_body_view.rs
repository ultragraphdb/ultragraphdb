/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::marker::PhantomData;

use ultragraphdb_core_body::Sections;
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::bind::Bind;
use crate::types::bind_kind::BindKind;
use crate::types::bind_view::BindView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::utils::body::read_sections;
use crate::utils::command_header::expect_command;
use crate::utils::fields::{epoch, identity};

impl<'a> BodyView<'a> for BindView<'a> {
    const COMMAND: Command = Command::Bind;

    type Body = Bind;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 5), (8, 56)])?;
        let partition = header.u32_at(0);
        if partition == 0 {
            return Err(ProtocolError::field_value(command, "partition", 0));
        }
        let number = header.u8_at(4);
        let kind = BindKind::from_number(number).ok_or(ProtocolError::field_value(
            command,
            "kind",
            number.into(),
        ))?;
        let read_version = header.u64_at(16);
        if kind == BindKind::Drop && read_version != 0 {
            return Err(ProtocolError::field_value(
                command,
                "read_version",
                read_version,
            ));
        }
        let token_epoch = epoch(command, "epoch", header.u64_at(24), true)?;
        let transaction = identity(header.0, 40);
        if transaction == [0; 16] {
            return Err(ProtocolError::field_value(command, "transaction", 0));
        }
        let _sections: Sections<'_, 0> = read_sections(command, frame.body(), 0)?;
        Ok(Self {
            frame: PhantomData,
            partition,
            kind,
            tenant: header.u64_at(8),
            read_version,
            epoch: token_epoch,
            principal: header.u64_at(32),
            transaction,
        })
    }

    fn to_body(&self) -> Result<Bind, ProtocolError> {
        Ok(Bind::new(
            self.partition,
            self.kind,
            self.tenant,
            self.epoch,
            self.principal,
            self.transaction,
        )
        .with_read_version(self.read_version))
    }
}
