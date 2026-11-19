/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_common_role::Answer;
use ultragraphdb_core_catalog::{CatalogContents, describe_of};
use ultragraphdb_core_ids::Version;
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    Action, Begin, BeginView, BodyBuilder, BodyView, CATALOG_BYTES_MAX, Command, DescribeFrames,
    DescribeView, ErrorCode, ErrorReply, FrameView, InstallView, MAX_FRAME_SIZE, MigrateView,
    PurgeView, QueryView, ReadLevel, Register,
};

use super::{ClientSession, now};
use crate::traits::authorizer::Authorizer;
use crate::types::bind_state::BindState;
use crate::types::client_transaction::ClientTransaction;
use crate::types::gate_scope::GateScope;
use crate::types::view_facts::ViewFacts;

fn protocol(rule: &ultragraphdb_error_protocol::ProtocolError) -> ErrorReply {
    ErrorReply::protocol(rule)
}

/// A request naming a tenant the metadata group does not hold: code 7, `detail` the tenant (`PROTOCOL.md` §11.10).
pub(super) fn unknown_tenant(tenant: u64) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::Refused,
        format!("the metadata group holds no tenant {tenant}"),
    )
    .with_detail(tenant)
}

/// A command compute does not serve in this stage: code 7, naming the command and the stage (`PROTOCOL.md` §11.2).
fn later_stage(command: &str, stage: &str) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::Refused,
        format!("{command} is served from stage {stage}"),
    )
}

impl<A: Authorizer + Clone> ClientSession<A> {
    /// The answer to `frame` on `stream`, as its command says.
    pub(super) async fn request(
        &mut self,
        stream: u64,
        frame: ReceivedFrame,
    ) -> Result<Answer, ErrorReply> {
        let view = frame.view().map_err(|rule| protocol(&rule))?;
        // The session ended this stream, and its end waits to leave: the client sent this before it could read that end, which
        // is its answer, so this is late (`PROTOCOL.md` §11.2) and refusing it would put another answer first.
        if self.ending(stream) {
            return Ok(Answer::Open);
        }
        let command = view.command();
        // A Commit decides its transaction, and its reply ends the stream: a Write, Operation or Commit after it is late
        // (`PROTOCOL.md` §11.2), and taking it would end a transaction the partition still decides.
        if (command == Command::Write
            || command == Command::Operation
            || command == Command::Commit)
            && self
                .transactions
                .get(&stream)
                .is_some_and(ClientTransaction::commit_taken)
        {
            return Ok(Answer::Open);
        }
        if command == Command::Register {
            return self.register(stream).await;
        }
        if command == Command::Begin {
            return self.begin(stream, &view).await;
        }
        if command == Command::Read {
            return self.read(stream, &view);
        }
        if command == Command::Execute {
            return self.execute(stream, &view).await;
        }
        if command == Command::Query {
            return self.query(stream, &view);
        }
        if command == Command::Write || command == Command::Operation {
            return self.write(stream, &view, frame.bytes());
        }
        if command == Command::Commit {
            return self.commit(stream, &view, frame.bytes()).await;
        }
        if command == Command::Abort {
            self.abort(stream);
            return Ok(Answer::Done);
        }
        if command == Command::Prepare {
            return self.prepare(stream, &view).await;
        }
        if command == Command::Describe {
            return self.describe(stream, &view).await;
        }
        if command == Command::Install {
            return self.install(&view).await;
        }
        if command == Command::Purge {
            return self.purge(&view).await;
        }
        if command == Command::Migrate {
            return self.migrate(&view).await;
        }
        if command == Command::Drop {
            return self.drop_request(stream, &view, frame.bytes()).await;
        }
        if command == Command::DirectWrite || command == Command::ConditionalWrite {
            return self.one_frame(stream, &view, frame.bytes()).await;
        }
        Err(ErrorReply::new(
            ErrorCode::Protocol,
            format!("{command:?} is no request of a client"),
        ))
    }

    /// A session registered for the connection's principal, durable before the reply.
    async fn register(&mut self, stream: u64) -> Result<Answer, ErrorReply> {
        let mut session = [0; 16];
        // Never all zero: 128 random bits are all zero only when the source is broken, which this refuses as well.
        getrandom::fill(&mut session).map_err(|error| {
            ErrorReply::new(
                ErrorCode::Unavailable,
                format!("the random source: {error}"),
            )
        })?;
        if session == [0; 16] {
            return Err(ErrorReply::new(
                ErrorCode::Unavailable,
                "the random source gave no bits",
            ));
        }
        self.core.system().session(session, self.principal).await?;
        self.push_last(stream, &Register::new(session));
        Ok(Answer::Open)
    }

    /// The gate on a request naming `tenant`, then whether the core's view holds it, its current version read once when it is
    /// first used (`PROTOCOL.md` §11.13, *Compute's reads*). A view not yet answering is waited for within
    /// `transaction_ms`.
    pub(super) async fn gate(&self, action: Action, tenant: u64) -> Result<ViewFacts, ErrorReply> {
        self.core
            .gate()
            .check(self.principal, action.number(), GateScope::Tenant(tenant))?;
        let system = self.core.system();
        let limit = Duration::from_millis(u64::from(self.core.limits().transaction_ms()));
        system.answering(now() + limit).await?;
        if !system.view().holds_tenant(tenant) {
            return Err(unknown_tenant(tenant));
        }
        system.known(tenant).await?;
        let view = system.view();
        Ok(ViewFacts {
            open_epoch: view.open_epoch(),
            last_closed: view.latest_closed(),
            current_version: view.current_version(tenant),
        })
    }

    /// A transaction on `stream`, its reads' version chosen.
    async fn begin(&mut self, stream: u64, view: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let begin = BeginView::from_frame(view).map_err(|rule| protocol(&rule))?;
        let tenant = begin.tenant();
        let mut facts = self.gate(Action::Read, tenant).await?;
        let token_epoch = begin.token().map_or(0, |token| token.epoch().get());
        let began = now();
        let limit = u64::from(self.core.limits().transaction_ms());
        let deadline_ms = match begin.deadline_ms() {
            0 => limit,
            asked => u64::from(asked),
        };
        if token_epoch > facts.open_epoch {
            // A token ahead of the view waits for the close before its epoch, until the deadline (`PROTOCOL.md` §4.8).
            let deadline = began + Duration::from_millis(deadline_ms);
            self.core.system().token(token_epoch, deadline).await?;
            facts = self.gate(Action::Read, tenant).await?;
        }
        let named = (begin.read_version() != 0).then_some(begin.read_version());
        let snapshot = begin.read_only() && begin.level() == ReadLevel::Snapshot && named.is_none();
        let epoch = if snapshot {
            facts.last_closed.max(token_epoch)
        } else {
            token_epoch
        };
        let read_version = match (named, snapshot) {
            (Some(version), _) => version,
            (None, true) => bound_of(epoch)?,
            (None, false) => 0,
        };
        if snapshot && epoch > facts.last_closed {
            // A snapshot read waits on the open epoch: its close comes early.
            self.core.system().close_request(epoch);
        }
        let identity = fresh_identity()?;
        let transaction = ClientTransaction {
            tenant,
            read_only: begin.read_only(),
            level: begin.level(),
            named,
            read_version,
            epoch,
            floor: token_epoch,
            catalog: facts.current_version,
            deadline: began + Duration::from_millis(deadline_ms),
            limit: (!begin.read_only()).then(|| began + Duration::from_millis(limit)),
            identity,
            bind: BindState::None,
            attempt: 0,
            resent: false,
            forced: None,
            awaiting_map: false,
            awaiting_claim: false,
            claimed: None,
            queue: std::collections::VecDeque::new(),
            write_out: None,
            first_open: false,
            first_sent: false,
            first_read: None,
            reads_out: std::collections::BTreeSet::new(),
            level_partition: None,
            crossed: None,
            commit_out: None,
            session_held: 0,
            ended: None,
            declared: Vec::new(),
            touched: false,
        };
        let frame = self.began(stream, &begin, read_version)?;
        let _none = self.transactions.insert(stream, transaction);
        Ok(Answer::Reply(frame))
    }

    /// The Begin answering `begin` on `stream`: what it asked, with the read version chosen.
    fn began(
        &self,
        stream: u64,
        begin: &BeginView<'_>,
        read_version: u64,
    ) -> Result<Vec<u8>, ErrorReply> {
        let reply = Begin::new(begin.tenant(), read_version, begin.read_only())
            .with_level(begin.level())
            .with_deadline_ms(begin.deadline_ms());
        let reply = match begin.token() {
            Some(token) => reply.with_token(token.clone()),
            None => reply,
        };
        let header = self.header(stream, Command::Begin);
        reply.frame(header).map_err(|rule| protocol(&rule))
    }

    /// A Query, which this stage serves in no transaction: its transaction stays open.
    fn query(&mut self, stream: u64, view: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let query = QueryView::from_frame(view).map_err(|rule| protocol(&rule))?;
        if let Some(refusal) = self
            .transactions
            .get(&query.transaction())
            .and_then(|transaction| transaction.ended.clone())
        {
            return Err(refusal);
        }
        self.push_error(stream, &later_stage("a Query", "7"));
        Ok(Answer::Open)
    }

    /// An Install: the gate, the tenant, and the stage that serves it.
    async fn install(&mut self, view: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let install = InstallView::from_frame(view).map_err(|rule| protocol(&rule))?;
        let _facts = self.gate(Action::Declare, install.tenant()).await?;
        Err(later_stage("an Install", "6.6"))
    }

    /// A Purge: the gate, `purge` on the tenant, the tenant, and the stage that serves it, 8 (`PROTOCOL.md` §8.5,
    /// §11.2, §11.10).
    async fn purge(&mut self, view: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let purge = PurgeView::from_frame(view).map_err(|rule| protocol(&rule))?;
        let _facts = self.gate(Action::Purge, purge.tenant()).await?;
        Err(later_stage("a Purge", "8"))
    }

    /// A Migrate: the gate, the tenant, and the stage that serves it, 9.5 (`PROTOCOL.md` §8.6, §11.2).
    async fn migrate(&mut self, view: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let migrate = MigrateView::from_frame(view).map_err(|rule| protocol(&rule))?;
        let _facts = self.gate(Action::Declare, migrate.tenant()).await?;
        Err(later_stage("a Migrate", "9.5"))
    }

    /// What the system group says of a tenant (`PROTOCOL.md` §8.3, §11.13, *Compute's reads*): the catalog version a
    /// client reads at the view's latest closed epoch, by a Catalog of `version` 0 naming it, which the answer names in
    /// `catalog` on every frame, and the default-entry records by Defaults; answered across frames within the client's
    /// `max_frame`, under its credit.
    async fn describe(&mut self, stream: u64, view: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let asked = DescribeView::from_client_frame(view).map_err(|rule| protocol(&rule))?;
        let tenant = asked.tenant();
        let facts = self.gate(Action::Read, tenant).await?;
        let closed = facts.last_closed;
        let system = self.core.system();
        let (catalog, declarations) = match system.catalog_at(tenant, closed).await {
            Ok((version, bytes)) => (
                version,
                CatalogContents::decode(tenant, version, &bytes)
                    .map_err(|error| ErrorReply::new(ErrorCode::Unavailable, error.to_string()))?
                    .into_declarations(),
            ),
            // No version is in effect at the epoch: the initial catalog's, version 0, which declares nothing.
            Err(refusal) if refusal.code() == ErrorCode::Refused && refusal.detail() == 0 => {
                (0, Vec::new())
            }
            Err(refusal) => return Err(refusal),
        };
        let recorded = system.defaults(tenant).await?;
        let read_version = if closed == 0 { 0 } else { bound_of(closed)? };
        let answer = describe_of(
            tenant,
            read_version,
            &declarations,
            recorded.iter().map(String::as_str),
        )
        .with_catalog(catalog);
        let frames = answer
            .frames(
                self.header(stream, Command::Describe),
                self.max_frame as usize,
            ) // lossless: a `u32` fits a `usize` here
            .map_err(|_| self.past_frame())?;
        let mut bounded = DescribeFrames::new();
        for frame in &frames {
            let read = FrameView::validate(frame, self.cluster(), MAX_FRAME_SIZE)
                .map_err(|rule| protocol(&rule))?;
            if bounded.push(&read).is_err() {
                return Err(ErrorReply::new(
                    ErrorCode::TooLarge,
                    format!("the answer passes catalog_bytes_max, {CATALOG_BYTES_MAX} bytes"),
                )
                .with_detail(CATALOG_BYTES_MAX as u64)); // lossless: a `usize` is at most 64 bits
            }
        }
        let credit = self.core.limits().initial_credit();
        let _none = self.describes.insert(stream, (frames.into(), credit));
        self.describe_credited(stream, 0);
        Ok(Answer::Open)
    }

    /// The client granted a Describe's `stream` credit for `frames` more frames: as many as its credit allows leave
    /// (`PROTOCOL.md` §8.3, *Continuation*; §11.3).
    pub(super) fn describe_credited(&mut self, stream: u64, frames: u32) {
        let mut ready = Vec::new();
        if let Some((waiting, credit)) = self.describes.get_mut(&stream) {
            *credit = credit.saturating_add(frames);
            // Each pass spends one credit on one frame, so the lesser of the two bounds it (Ultra rule 2).
            while *credit > 0 {
                let Some(frame) = waiting.pop_front() else {
                    break;
                };
                *credit -= 1;
                ready.push(frame);
            }
            if waiting.is_empty() {
                let _done = self.describes.remove(&stream);
            }
        }
        for frame in ready {
            self.push(stream, frame);
        }
    }

    fn past_frame(&self) -> ErrorReply {
        ErrorReply::new(
            ErrorCode::TooLarge,
            format!(
                "the answer does not fit a frame of {} bytes",
                self.max_frame
            ),
        )
        .with_detail(u64::from(self.max_frame))
    }
}

/// The version bound of `epoch`.
fn bound_of(epoch: u64) -> Result<u64, ErrorReply> {
    let epoch = ultragraphdb_core_ids::Epoch::new(epoch)
        .map_err(|error| ErrorReply::new(ErrorCode::Protocol, error.to_string()))?;
    Ok(Version::bound_of(epoch))
}

/// A transaction's identity, drawn at its Begin.
fn fresh_identity() -> Result<[u8; 16], ErrorReply> {
    let mut identity = [0; 16];
    getrandom::fill(&mut identity).map_err(|error| {
        ErrorReply::new(
            ErrorCode::Unavailable,
            format!("the random source: {error}"),
        )
    })?;
    Ok(identity)
}
