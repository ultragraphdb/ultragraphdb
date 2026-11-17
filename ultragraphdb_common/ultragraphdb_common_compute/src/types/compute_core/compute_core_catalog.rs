/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_role::RoleClient;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    BodyView, CATALOG_BYTES_MAX, Catalog, CatalogFrames, Command, Credit, ErrorReply,
    ErrorReplyView, MAX_FRAME_FLOOR,
};

use super::ComputeCore;
use crate::utils::bounded::bounded;
use crate::utils::refusals::unavailable;

/// The Catalog frames one version takes at most (Ultra rule 2): its bytes at their cap, each frame but the last filled
/// within the requester's `max_frame`, which is at least the protocol's floor and holds more than half the floor in
/// bytes past its headers, and the last.
const CATALOG_FRAMES_MAX: usize = CATALOG_BYTES_MAX.div_ceil(MAX_FRAME_FLOOR / 2) + 1;

impl ComputeCore {
    /// Version `version` of `tenant`'s catalog, 0 for the tenant's current one, read from the system group across as many
    /// Catalog frames as it takes, a frame's credit granted as each arrives (`PROTOCOL.md` §11.3, §13.7; design D13).
    ///
    /// A dropped future drops its connection to the group, and the frames read so far with it.
    ///
    /// # Errors
    ///
    /// The group's Error, its code, message and detail, as code 7 for a version the tenant never held; code 17 for a group
    /// that cannot be reached or answers past the answer bound, and for a frame compute refuses, one breaking `PROTOCOL.md`
    /// §13.7's rules among them: the connection compute dialled closes under the read, which the side that dialled answers
    /// with code 17 (§11.12, *Errors*).
    pub async fn catalog(&self, tenant: u64, version: u64) -> Result<CatalogFrames, ErrorReply> {
        let read = async {
            let mut client = self
                .dial_group()
                .await
                .map_err(|error| unavailable(format!("the catalog of tenant {tenant}: {error}")))?;
            read_catalog(&mut client, &Catalog::request(tenant, version)).await
        };
        bounded(
            self.times.answer_wait(),
            || format!("the system group's catalog version {version} of tenant {tenant}"),
            read,
        )
        .await
    }
}

/// The Catalog answer to `request` on `client`, read frame by frame into one version, a frame's credit granted as each
/// arrives while more are due, so the stream's credit stays what the group's Welcome gave it.
///
/// A dropped future cuts the frame being read, and the client with it is given up.
async fn read_catalog(
    client: &mut RoleClient,
    request: &Catalog,
) -> Result<CatalogFrames, ErrorReply> {
    let lost = |error: &dyn std::fmt::Display| unavailable(format!("the catalog: {error}"));
    let refused =
        |rule: &ProtocolError| unavailable(format!("a frame of the catalog was refused: {rule}"));
    client
        .send_only(request)
        .await
        .map_err(|error| lost(&error))?;
    let stream = client.last_stream();
    let mut credit = client.welcome().initial_credit();
    let mut frames = CatalogFrames::new();
    for _ in 0..CATALOG_FRAMES_MAX {
        let received = client.next_frame().await.map_err(|error| lost(&error))?;
        let view = received.view().map_err(|rule| refused(&rule))?;
        if view.command() == Command::Error {
            let error = ErrorReplyView::from_frame(&view).map_err(|rule| refused(&rule))?;
            return Err(ErrorReply::new(error.code(), error.message()).with_detail(error.detail()));
        }
        if view.header().stream() != stream {
            return Err(refused(&ProtocolError::field_value(
                Command::Catalog.number(),
                "stream",
                view.header().stream(),
            )));
        }
        credit = credit
            .checked_sub(1)
            .ok_or_else(|| refused(&ProtocolError::past_credit(stream)))?;
        frames.push(&view).map_err(|rule| refused(&rule))?;
        if frames.is_last() {
            return Ok(frames);
        }
        if credit == 0 {
            client
                .send_on(stream, &Credit::new(1))
                .await
                .map_err(|error| lost(&error))?;
            credit = 1;
        }
    }
    Err(refused(&ProtocolError::field_value(
        Command::Catalog.number(),
        "catalog",
        CATALOG_FRAMES_MAX as u64, // lossless: a `usize` is at most 64 bits
    )))
}
