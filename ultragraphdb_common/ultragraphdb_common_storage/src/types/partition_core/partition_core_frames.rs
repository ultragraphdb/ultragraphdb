/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;

use ultragraphdb_cluster_partition::{BoundKey, BoundStep, PartitionError};
use ultragraphdb_common_role::{Answer, Push};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_wire::{
    BindKind, BindView, BodyBuilder, BodyView, Command, CommitView, ErrorCode, ErrorReply,
    FLAG_LAST, FrameView, Header, HeaderView, Held, MAX_FRAME_SIZE, OperationView, ReferenceView,
    WriteView,
};
use ultragraphdb_transactions_txn::{FrameChanges, FrameTerms};

use crate::types::core_message::{CoreMessage, Pushes};
use crate::types::parked_frame::ParkedFrame;
use crate::types::partition_core::partition_core_one_frame::ONE_FRAME_ATTEMPTS;
use crate::types::partition_core::{Committing, Parked, ParkedWork, PartitionCore, RequestStep};
use crate::utils::error_reply::error_reply;
use crate::utils::operation_of::operation_of;
use ultragraphdb_transactions_log::DurableStorage;

/// The write `frame` carries, its terms lent by the frame: what `run` makes of its changes.
fn with_changes<T>(
    view: &FrameView<'_>,
    tenant: u64,
    run: impl FnOnce(&FrameChanges<'_>) -> Result<T, PartitionError>,
) -> Result<T, ErrorReply> {
    let write = WriteView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
    let terms = FrameTerms::of(write.terms()).map_err(|rule| ErrorReply::protocol(&rule))?;
    let changes =
        FrameChanges::new(tenant, &write, &terms).map_err(|rule| ErrorReply::protocol(&rule))?;
    run(&changes).map_err(|refusal| error_reply(&refusal))
}

impl<S: DurableStorage> PartitionCore<'_, S> {
    /// Takes what a connection handed over.
    pub(super) fn take(&mut self, message: CoreMessage) {
        match message {
            CoreMessage::Request {
                connection,
                stream,
                frame,
                pushes,
                max_frame,
            } => self.request((connection, stream), frame, pushes, max_frame),
            CoreMessage::Cancelled { connection, stream } => {
                let key = (connection, stream);
                self.host.abort_bound(key);
                let _ended = self.bound_tenant.remove(&key);
                let _drop = self.drop_streams.remove(&key);
                let _parked = self.parked.remove(&key);
                self.cancel_scan(key);
            }
            CoreMessage::Catalog(contents) => self.install(contents),
            CoreMessage::Ready { connection, stream } => self.scan_room((connection, stream)),
            CoreMessage::Left { connection, stream } => self.scan_left((connection, stream)),
            CoreMessage::Closed { connection } => {
                self.host.abort_connection(connection);
                self.bound_tenant.retain(|(of, _), _| *of != connection);
                self.drop_streams.retain(|(of, _), _| *of != connection);
                self.parked.retain(|(of, _), _| *of != connection);
                self.close_scans(connection);
            }
        }
    }

    /// A request of `key`, as the core takes it: behind its stream's parked request it waits its turn, and an Abort ends what it
    /// would wait behind (`PROTOCOL.md` §11.12). Any other runs now, and parks, holding its copy, should it wait on a home. Its
    /// answer, if it has one now, is pushed.
    fn request(&mut self, key: BoundKey, frame: ParkedFrame, pushes: Pushes, max_frame: u32) {
        self.taking = self.mark();
        let is_abort = HeaderView::new(frame.bytes())
            .is_ok_and(|header| header.command() == Command::Abort.number());
        if !is_abort && let Some(parked) = self.parked.get_mut(&key) {
            parked.behind.push_back(frame);
            return;
        }
        let step = match FrameView::validate(frame.bytes(), self.cluster, MAX_FRAME_SIZE) {
            Ok(view) => self.step(key, &view, &pushes, max_frame),
            Err(rule) => RequestStep::Answered(Answer::Refuse(ErrorReply::protocol(&rule))),
        };
        match step {
            RequestStep::Answered(answer) => self.push_answer(key.1, answer, &pushes),
            RequestStep::Parks { tenant, work } => {
                let parked = Parked {
                    frame,
                    tenant,
                    work,
                    pushes,
                    max_frame,
                    // Each request behind is a copy counted in its connection's held bytes, whose bound stops the connection's
                    // reads, so that bound limits its length.
                    behind: VecDeque::new(),
                    taken: self.taking,
                };
                let _none = self.parked.insert(key, parked);
            }
            RequestStep::InCommit(ticket) => {
                let taken = self.taking;
                let held = frame.into_bytes();
                self.committing_one_frame(key, (held, taken), ONE_FRAME_ATTEMPTS, ticket, pushes);
            }
        }
    }

    /// Pushes `answer` on `stream`: a reply, a refusal as an Error ending the stream, or the stream's end with no frame; an open
    /// stream's answer comes later.
    pub(super) fn push_answer(&self, stream: u64, answer: Answer, pushes: &Pushes) {
        match answer {
            // A connection gone has no one to tell.
            Answer::Reply(reply) => drop(pushes.unbounded_send((stream, Push::Frame(reply)))),
            Answer::Done => drop(pushes.unbounded_send((stream, Push::End))),
            Answer::Refuse(refusal) => self.push_error(stream, &refusal, pushes),
            Answer::Open => {}
        }
    }

    /// What a request of `key` comes to in its turn: one of a Write, an Operation or a Commit behind a read's wait on its home waits for
    /// that reply (`PROTOCOL.md` §13.13), and any other is taken now.
    fn step(
        &mut self,
        key: BoundKey,
        view: &FrameView<'_>,
        pushes: &Pushes,
        max_frame: u32,
    ) -> RequestStep {
        let command = view.command();
        let is_write = command == Command::Write;
        let is_operation = command == Command::Operation;
        if is_write || is_operation || command == Command::Commit {
            let tenant = self.bound_tenant.get(&key).map(|(tenant, _)| *tenant);
            if let Some(tenant) = tenant.filter(|_| self.host.read_waits(key)) {
                return RequestStep::Parks {
                    tenant,
                    work: ParkedWork::Turn,
                };
            }
        }
        if is_write {
            self.write(key, view)
        } else if is_operation {
            self.operate(key, view)
        } else if command == Command::DirectWrite || command == Command::ConditionalWrite {
            self.one_frame(key, view)
        } else {
            RequestStep::Answered(self.answer(key, view, pushes, max_frame))
        }
    }

    /// The transaction a request of `key` needs, named `command` should a Bind of kind 1 have opened no such stream.
    fn tenant_of(&mut self, key: BoundKey, command: &str) -> Result<u64, ErrorReply> {
        match self.bound_tenant.get(&key) {
            Some((tenant, _)) => Ok(*tenant),
            None => {
                let _ended = self.drop_streams.remove(&key);
                Err(unbound(command, 1))
            }
        }
    }

    /// A Write of the transaction `key`: its references asked, and it parks until they are answered.
    fn write(&mut self, key: BoundKey, view: &FrameView<'_>) -> RequestStep {
        let tenant = match self.tenant_of(key, "Write") {
            Ok(tenant) => tenant,
            Err(refusal) => return RequestStep::Answered(Answer::Refuse(refusal)),
        };
        let now = self.now_us();
        let host = &mut self.host;
        match with_changes(view, tenant, |changes| host.write_bound(key, changes, now)) {
            Ok(BoundStep::Done) => RequestStep::Answered(self.held(key.1)),
            Ok(BoundStep::Waiting) => RequestStep::Parks {
                tenant,
                work: ParkedWork::Write,
            },
            Err(refusal) => RequestStep::Answered(self.ended_by(key, refusal)),
        }
    }

    /// Ends the transaction `key` as `refusal` says.
    fn ended_by(&mut self, key: BoundKey, refusal: ErrorReply) -> Answer {
        let _ended = self.bound_tenant.remove(&key);
        self.host.abort_bound(key);
        Answer::Refuse(refusal)
    }

    /// An Operation of the transaction `key`.
    fn operate(&mut self, key: BoundKey, view: &FrameView<'_>) -> RequestStep {
        let tenant = match self.tenant_of(key, "Operation") {
            Ok(tenant) => tenant,
            Err(refusal) => return RequestStep::Answered(Answer::Refuse(refusal)),
        };
        let read = (|| {
            let operation =
                OperationView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
            let terms =
                FrameTerms::of(operation.terms()).map_err(|rule| ErrorReply::protocol(&rule))?;
            operation_of(&operation.operation(), operation.terms(), &terms)
                .map_err(|rule| ErrorReply::protocol(&rule))
        })();
        let now = self.now_us();
        let step = read.and_then(|operation| {
            self.host
                .operate_bound(key, operation, now)
                .map_err(|refusal| error_reply(&refusal))
        });
        match step {
            Ok(BoundStep::Done) => RequestStep::Answered(self.held(key.1)),
            Ok(BoundStep::Waiting) => RequestStep::Parks {
                tenant,
                work: ParkedWork::Operation,
            },
            Err(refusal) => RequestStep::Answered(self.ended_by(key, refusal)),
        }
    }

    /// The answer to a Write or Operation the partition holds, on the Bind stream `stream`, which stays open.
    fn held(&self, stream: u64) -> Answer {
        let header = Header::new(self.cluster, Command::Held).with_stream(stream);
        built(&Held, header).unwrap_or_else(Answer::Refuse)
    }

    /// Runs each request whose transaction's references were answered, from the frame it held.
    pub(super) fn resume_ready(&mut self) {
        for (key, refusal) in self.host.take_bound_refused() {
            let reply = error_reply(&refusal);
            self.tell_ended(key, &reply);
            self.refuse_references(key, &reply);
        }
        for key in self.host.take_bound_ready() {
            self.wake_references(key);
            let Some(parked) = self.parked.remove(&key) else {
                continue;
            };
            self.resume_parked(key, parked);
        }
    }

    /// The parked request of `key`, whose references were answered, on its way, and each frame behind it in its turn, every answer
    /// pushed: one parking again takes the rest with it, and an answer ending the stream, or a Commit, which decides the
    /// transaction, drops them.
    fn resume_parked(&mut self, key: BoundKey, mut parked: Parked) {
        loop {
            self.taking = parked.taken;
            match self.resumed_step(key, &parked) {
                RequestStep::Parks { tenant, work } => {
                    parked.tenant = tenant;
                    parked.work = work;
                    let _none = self.parked.insert(key, parked);
                    return;
                }
                RequestStep::InCommit(ticket) => {
                    let held = parked.frame.into_bytes();
                    let taken = parked.taken;
                    self.committing_one_frame(
                        key,
                        (held, taken),
                        ONE_FRAME_ATTEMPTS,
                        ticket,
                        parked.pushes,
                    );
                    return;
                }
                RequestStep::Answered(Answer::Reply(reply)) => {
                    self.push_answer(key.1, Answer::Reply(reply), &parked.pushes);
                }
                RequestStep::Answered(answer @ (Answer::Refuse(_) | Answer::Done)) => {
                    self.push_answer(key.1, answer, &parked.pushes);
                    return;
                }
                RequestStep::Answered(Answer::Open) => return,
            }
            let Some(next) = parked.behind.pop_front() else {
                return;
            };
            parked.frame = next;
            parked.work = ParkedWork::Turn;
        }
    }

    /// What the parked `parked` of `key` comes to, read again from its copy, whose checksums the connection's machine checked.
    fn resumed_step(&mut self, key: BoundKey, parked: &Parked) -> RequestStep {
        let view = match FrameView::validate(parked.frame.bytes(), self.cluster, MAX_FRAME_SIZE) {
            Ok(view) => view,
            Err(rule) => return RequestStep::Answered(Answer::Refuse(ErrorReply::protocol(&rule))),
        };
        match parked.work {
            ParkedWork::Turn => self.step(key, &view, &parked.pushes, parked.max_frame),
            ParkedWork::Write => {
                let now = self.now_us();
                let host = &mut self.host;
                let written = with_changes(&view, parked.tenant, |changes| {
                    host.resume_write(key, changes, now)
                });
                match written {
                    Ok(()) => RequestStep::Answered(self.held(key.1)),
                    Err(refusal) => RequestStep::Answered(self.ended_by(key, refusal)),
                }
            }
            ParkedWork::Operation => RequestStep::Answered(self.held(key.1)),
            ParkedWork::OneFrame { at } => self.resume_one_frame(key, &view, at),
        }
    }

    /// Any request that is not a write: answered now, or later through `pushes`.
    fn answer(
        &mut self,
        key: BoundKey,
        view: &FrameView<'_>,
        pushes: &Pushes,
        max_frame: u32,
    ) -> Answer {
        let command = view.command();
        let answered = if command == Command::Bind {
            self.bind(key, view, pushes)
        } else if command == Command::Commit {
            self.commit(key, view, pushes)
        } else if command == Command::Abort {
            self.abort(key)
        } else if command == Command::Drop {
            self.drop_on(key, view, pushes)
        } else if command == Command::Reference {
            self.reference(key, view, pushes)
        } else if command == Command::Scan {
            self.scan(key, view, pushes, max_frame)
        } else if command == Command::Closed {
            self.closed_epochs(key.1, view)
        } else if command == Command::Recall {
            self.recall(key.1, view)
        } else {
            Err(ErrorReply::new(
                ErrorCode::Unavailable,
                format!("command {} is not hosted here yet", command.number()),
            ))
        };
        answered.unwrap_or_else(Answer::Refuse)
    }

    fn bind(
        &mut self,
        key: BoundKey,
        view: &FrameView<'_>,
        pushes: &Pushes,
    ) -> Result<Answer, ErrorReply> {
        let bind = BindView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        if bind.partition() != self.id.get() {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                format!(
                    "this address hosts partition {}, not {}",
                    self.id.get(),
                    bind.partition()
                ),
            ));
        }
        let header = Header::new(self.cluster, Command::Bind).with_stream(key.1);
        if !self.tenants.contains(&bind.tenant()) {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                format!(
                    "this partition's placement map lists no run for tenant {}",
                    bind.tenant()
                ),
            ));
        }
        if bind.kind() == BindKind::Drop {
            let _new = self
                .drop_streams
                .insert(key, (bind.tenant(), bind.principal()));
            let reply = bind.to_body().map_err(|rule| ErrorReply::protocol(&rule))?;
            return built(&reply, header);
        }
        let epoch = Epoch::new(bind.epoch())
            .map_err(|error| ErrorReply::new(ErrorCode::Protocol, error.to_string()))?;
        let now = self.now_us();
        let read_version = self
            .host
            .bind(
                key,
                bind.transaction(),
                (bind.tenant(), bind.principal(), epoch),
                now,
            )
            .map_err(|refusal| error_reply(&refusal))?;
        let _previous = self
            .bound_tenant
            .insert(key, (bind.tenant(), pushes.clone()));
        let reply = bind
            .to_body()
            .map_err(|rule| ErrorReply::protocol(&rule))?
            .with_read_version(read_version);
        built(&reply, header)
    }

    /// An Abort of the transaction `key`.
    fn abort(&mut self, key: BoundKey) -> Result<Answer, ErrorReply> {
        let _tenant = self.tenant_of(key, "Abort")?;
        let _parked = self.parked.remove(&key);
        self.host.abort_bound(key);
        let _ended = self.bound_tenant.remove(&key);
        Ok(Answer::Done)
    }

    /// A Drop on `key`: on a stream a Bind of kind 2 opened it runs; on one a Bind of kind 1 opened it ends that transaction.
    fn drop_on(
        &mut self,
        key: BoundKey,
        view: &FrameView<'_>,
        pushes: &Pushes,
    ) -> Result<Answer, ErrorReply> {
        if let Some(bound) = self.drop_streams.remove(&key) {
            return self.drop_bound(key, bound, view, pushes);
        }
        self.host.abort_bound(key);
        let _ended = self.bound_tenant.remove(&key);
        Err(unbound("Drop", 2))
    }

    fn commit(
        &mut self,
        key: BoundKey,
        view: &FrameView<'_>,
        pushes: &Pushes,
    ) -> Result<Answer, ErrorReply> {
        let _tenant = self.tenant_of(key, "Commit")?;
        let commit = CommitView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        let session = u128::from_be_bytes(commit.session());
        let now = self.now_us();
        let _ended = self.bound_tenant.remove(&key);
        let ticket = self
            .host
            .commit_bound(key, session, commit.request(), now)
            .map_err(|refusal| error_reply(&refusal))?;
        let _none = self.committing.insert(
            ticket,
            Committing {
                stream: key.1,
                session,
                request: commit.request(),
                pushes: pushes.clone(),
                retry: None,
                dropped: None,
                taken: self.taking,
                stepped: self.mark(),
            },
        );
        Ok(Answer::Open)
    }

    fn reference(
        &mut self,
        key: BoundKey,
        view: &FrameView<'_>,
        pushes: &Pushes,
    ) -> Result<Answer, ErrorReply> {
        let asked = ReferenceView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        self.home_asked(key.1, &asked, pushes)
    }
}

/// The refusal of `command` on a stream no Bind of `kind` opened (`PROTOCOL.md` §11.2).
fn unbound(command: &str, kind: u8) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::Protocol,
        format!("{command} on a stream no Bind of kind {kind} opened"),
    )
}

pub(super) fn built<B: BodyBuilder>(body: &B, header: Header) -> Result<Answer, ErrorReply> {
    body.frame(header)
        .map(Answer::Reply)
        .map_err(|rule| ErrorReply::protocol(&rule))
}

/// A frame's header flagged last, for a reply that ends its stream.
pub(super) fn last(cluster: u64, stream: u64, command: Command) -> Header {
    Header::new(cluster, command)
        .with_stream(stream)
        .with_flags(FLAG_LAST)
}
