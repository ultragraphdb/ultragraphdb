/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Declare, Declared, ErrorCode, ErrorReply, Header,
};

/// Whether a Declare of kind 1 of `declared` for `tenant`, built on `built_on` for `principal`, fits one frame within the
/// role connection's `max_frame` (`PROTOCOL.md` §11.13, *A new version*).
///
/// # Errors
///
/// Code 6 (TooLarge), `detail` `max_frame`, when it does not: the Write taking the declarations past it is refused, ending
/// its transaction.
pub fn declare_fits(
    tenant: u64,
    built_on: u64,
    principal: u64,
    declared: &[Declared],
    max_frame: u32,
) -> Result<(), ErrorReply> {
    let declare = Declare::declarations(tenant, built_on, principal, declared.to_vec());
    let fits = declare
        .frame(Header::new(0, Command::Declare))
        .is_ok_and(|frame| u64::try_from(frame.len()).is_ok_and(|len| len <= u64::from(max_frame)));
    if fits {
        return Ok(());
    }
    Err(ErrorReply::new(
        ErrorCode::TooLarge,
        format!(
            "the transaction's declarations pass one Declare frame of {max_frame} bytes; a larger schema comes by Install"
        ),
    )
    .with_detail(u64::from(max_frame)))
}
