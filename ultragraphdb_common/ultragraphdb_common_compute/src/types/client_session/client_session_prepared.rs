/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_role::Answer;
use ultragraphdb_protocol_wire::{
    Action, BodyBuilder, BodyView, Command, ErrorCode, ErrorReply, ExecuteView, FLAG_LAST,
    FrameView, Header, Language, MAX_FRAME_SIZE, PrepareView, Read, ReadView,
};

use super::ClientSession;
use crate::traits::authorizer::Authorizer;
use crate::types::read_pattern::ReadPattern;
use crate::utils::term_rules::{check_pattern, check_table};

impl<A: Authorizer + Clone> ClientSession<A> {
    /// A Prepare: the gate and the tenant, then a pattern held for this connection; a text waits for stage 7.
    pub(super) async fn prepare(
        &mut self,
        stream: u64,
        view: &FrameView<'_>,
    ) -> Result<Answer, ErrorReply> {
        let asked = PrepareView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        asked
            .check_request()
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        let tenant = asked.tenant();
        let _facts = self.gate(Action::Read, tenant).await?;
        if asked.language() != Language::Pattern {
            return Err(ErrorReply::new(
                ErrorCode::Refused,
                "a Prepare of a text is served from stage 7",
            ));
        }
        check_table(asked.terms())?;
        if let Some(pattern) = asked.pattern() {
            check_pattern(asked.terms(), pattern)?;
        }
        let schema = self.core.system().schema_id(tenant).await?;
        let reply = self
            .statements
            .prepare(&asked, schema)
            .map_err(|refusal| refusal.reply())?;
        let header = self.header(stream, Command::Prepare).with_flags(FLAG_LAST);
        reply
            .frame(header)
            .map(Answer::Reply)
            .map_err(|rule| ErrorReply::protocol(&rule))
    }

    /// An Execute: refused in `PROTOCOL.md` §11.7's order, and otherwise a read of its statement with its parameters bound.
    pub(super) async fn execute(
        &mut self,
        stream: u64,
        view: &FrameView<'_>,
    ) -> Result<Answer, ErrorReply> {
        let asked = ExecuteView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        let statement = asked.statement();
        let transaction = asked.transaction();
        let Some(txn) = self.transactions.get(&transaction) else {
            return self.not_open(stream, transaction);
        };
        if let Some(refusal) = txn.ended.clone() {
            return Err(refusal);
        }
        let tenant = txn.tenant;
        if self
            .statements
            .tenant(statement)
            .is_some_and(|of| of != tenant)
        {
            return Err(ErrorReply::new(
                ErrorCode::UnknownStatement,
                format!(
                    "statement {statement} was prepared in another tenant than the transaction's"
                ),
            )
            .with_detail(statement));
        }
        let schema = self.core.system().schema_id(tenant).await?;
        let (terms, pattern) = self
            .statements
            .execute(&asked, schema)
            .map_err(|refusal| refusal.reply())?;
        // The Read of its statement's pattern with its parameters bound, whose sections the resume key's pattern hashes
        // (`PROTOCOL.md` §11.8): an Execute is read as that Read, which fits a frame.
        let frame = Read::new(
            transaction,
            asked.limit(),
            terms,
            pattern,
            asked.resume().to_vec(),
        )
        .frame(Header::new(self.cluster(), Command::Read).with_stream(stream))
        .map_err(|_| too_large())?;
        if frame.len() > MAX_FRAME_SIZE {
            return Err(too_large());
        }
        let read = FrameView::validate(&frame, self.cluster(), MAX_FRAME_SIZE)
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        let bound = ReadView::from_frame(&read).map_err(|rule| ErrorReply::protocol(&rule))?;
        let request = ReadPattern::of_read(&bound)?;
        self.take_read(
            stream,
            transaction,
            (bound.terms(), bound.pattern()),
            request,
        )
    }
}

/// An Execute whose Read, its parameters bound, passes a frame: code 6, `detail` 1,048,576 (quad-reads, "A read's batches fit
/// the client's frame").
fn too_large() -> ErrorReply {
    ErrorReply::new(
        ErrorCode::TooLarge,
        "the Execute's Read, its parameters bound, does not fit a frame",
    )
    .with_detail(MAX_FRAME_SIZE as u64) // 2^20, lossless
}
