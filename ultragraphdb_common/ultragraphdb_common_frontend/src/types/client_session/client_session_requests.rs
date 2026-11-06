/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_common_role::Answer;
use ultragraphdb_core_ids::{SessionToken, Version};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    Action, Begin, BeginView, BodyBuilder, BodyView, Command, DescribeView, ErrorCode, ErrorReply,
    FrameView, InstallView, QueryView, ReadLevel, Register,
};

use super::{ClientSession, now};
use crate::traits::authorizer::Authorizer;
use crate::types::bind_state::BindState;
use crate::types::client_transaction::ClientTransaction;
use crate::types::gate_scope::GateScope;
use crate::types::tenant_facts::TenantFacts;

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

/// A command the frontend does not serve in this stage: code 7, naming the command and the stage (`PROTOCOL.md` §11.2).
fn later_stage(command: &str, stage: u32) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::Refused,
        format!("{command} is served from stage {stage}"),
    )
}

/// A token above the sequencer's open epoch, which no commit can have carried: Error 1 on the request's stream, naming it.
pub(super) fn check_token(token: Option<&SessionToken>, open: u64) -> Result<(), ErrorReply> {
    match token.map(|token| token.epoch().get()) {
        Some(epoch) if epoch > open => Err(ErrorReply::new(
            ErrorCode::Protocol,
            format!("a token of epoch {epoch}, above the sequencer's open epoch {open}"),
        )),
        Some(_) | None => Ok(()),
    }
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
        self.core
            .owner()
            .register_session(u128::from_be_bytes(session), self.principal)
            .await?;
        self.push_last(stream, &Register::new(session));
        Ok(Answer::Open)
    }

    /// The gate on a request naming `tenant`, then whether the group holds it.
    pub(super) async fn gate(
        &self,
        action: Action,
        tenant: u64,
    ) -> Result<TenantFacts, ErrorReply> {
        self.core
            .gate()
            .check(self.principal, action.number(), GateScope::Tenant(tenant))?;
        let facts = self.core.owner().tenant_facts(tenant).await?;
        if !facts.known() {
            return Err(unknown_tenant(tenant));
        }
        Ok(facts)
    }

    /// A transaction on `stream`, its reads' version chosen.
    async fn begin(&mut self, stream: u64, view: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let begin = BeginView::from_frame(view).map_err(|rule| protocol(&rule))?;
        let tenant = begin.tenant();
        let facts = self.gate(Action::Read, tenant).await?;
        check_token(begin.token(), facts.open_epoch())?;
        let token_epoch = begin.token().map_or(0, |token| token.epoch().get());
        let named = (begin.read_version() != 0).then_some(begin.read_version());
        let snapshot = begin.read_only() && begin.level() == ReadLevel::Snapshot && named.is_none();
        let epoch = if snapshot {
            facts.last_closed().max(token_epoch)
        } else {
            token_epoch
        };
        let read_version = match (named, snapshot) {
            (Some(version), _) => version,
            (None, true) => bound_of(epoch)?,
            (None, false) => 0,
        };
        if snapshot && epoch > facts.last_closed() {
            // A snapshot read waits on the open epoch: its close comes early.
            self.core.owner().close_request(epoch).await?;
        }
        let began = now();
        let limit = u64::from(self.core.limits().transaction_ms());
        let deadline_ms = match begin.deadline_ms() {
            0 => limit,
            asked => u64::from(asked),
        };
        let identity = fresh_identity()?;
        let transaction = ClientTransaction {
            tenant,
            read_only: begin.read_only(),
            level: begin.level(),
            named,
            read_version,
            epoch,
            floor: token_epoch,
            catalog: facts.current_version(),
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
        self.push_error(stream, &later_stage("a Query", 7));
        Ok(Answer::Open)
    }

    /// An Install: the gate, the tenant, and the stage that serves it.
    async fn install(&mut self, view: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let install = InstallView::from_frame(view).map_err(|rule| protocol(&rule))?;
        let _facts = self.gate(Action::Declare, install.tenant()).await?;
        Err(later_stage("an Install", 6))
    }

    /// What the metadata group says of a tenant, answered within the client's `max_frame`.
    async fn describe(&mut self, stream: u64, view: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let asked = DescribeView::from_frame(view).map_err(|rule| protocol(&rule))?;
        let tenant = asked.tenant();
        let _facts = self.gate(Action::Read, tenant).await?;
        let answer = self.core.owner().describe(tenant).await?;
        let header = self
            .header(stream, Command::Describe)
            .with_flags(ultragraphdb_protocol_wire::FLAG_LAST);
        let frame = answer.frame(header).map_err(|_| self.past_frame())?;
        if frame.len() as u64 > u64::from(self.max_frame) {
            return Err(self.past_frame());
        }
        Ok(Answer::Reply(frame))
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
