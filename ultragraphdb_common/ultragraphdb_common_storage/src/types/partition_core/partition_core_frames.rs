/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::oneshot;
use ultragraphdb_cluster_partition::{BoundKey, BoundStep, PartitionError};
use ultragraphdb_common_role::Answer;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    BindKind, BindView, BodyBuilder, BodyView, Command, CommitView, ErrorCode, ErrorReply,
    FLAG_LAST, FrameView, Header, Held, OperationView, ReferenceView, WriteView,
};
use ultragraphdb_transactions_txn::{FrameChanges, FrameTerms};

use crate::types::core_message::{CoreMessage, Pushes};
use crate::types::partition_core::{Committing, Parked, ParkedWork, PartitionCore};
use crate::utils::error_reply::error_reply;
use crate::utils::operation_of::operation_of;

/// The write `frame` carries, its terms lent by the frame: what `run` makes of its changes.
fn with_changes<T>(
    frame: &ReceivedFrame,
    tenant: u64,
    run: impl FnOnce(&FrameChanges<'_>) -> Result<T, PartitionError>,
) -> Result<T, ErrorReply> {
    let view = frame.view().map_err(|rule| ErrorReply::protocol(&rule))?;
    let write = WriteView::from_frame(&view).map_err(|rule| ErrorReply::protocol(&rule))?;
    let terms = FrameTerms::of(write.terms()).map_err(|rule| ErrorReply::protocol(&rule))?;
    let changes =
        FrameChanges::new(tenant, &write, &terms).map_err(|rule| ErrorReply::protocol(&rule))?;
    run(&changes).map_err(|refusal| error_reply(&refusal))
}

impl PartitionCore<'_> {
    /// Takes what a connection handed over.
    pub(super) fn take(&mut self, message: CoreMessage) {
        match message {
            CoreMessage::Request {
                connection,
                stream,
                frame,
                done,
                pushes,
                max_frame,
            } => self.request((connection, stream), frame, done, pushes, max_frame),
            CoreMessage::Cancelled { connection, stream } => {
                let key = (connection, stream);
                self.host.abort_bound(key);
                let _ended = self.bound_tenant.remove(&key);
                let _drop = self.drop_streams.remove(&key);
                let _parked = self.parked.remove(&key);
                self.cancel_scan(key);
            }
            CoreMessage::Ready { connection, stream } => self.scan_room((connection, stream)),
            CoreMessage::Left { connection, stream } => self.scan_left((connection, stream)),
            CoreMessage::Closed { connection } => {
                self.host.abort_connection(connection);
                self.bound_tenant.retain(|(of, _), _| *of != connection);
                self.drop_streams.retain(|(of, _)| *of != connection);
                self.parked.retain(|(of, _), _| *of != connection);
                self.close_scans(connection);
            }
        }
    }

    /// A request of `key`, in its turn: one of a Write, an Operation or a Commit behind a read's wait on its home waits for that
    /// reply (`PROTOCOL.md` §13.13), and any other is taken now.
    fn request(
        &mut self,
        key: BoundKey,
        frame: ReceivedFrame,
        done: oneshot::Sender<Answer>,
        pushes: Pushes,
        max_frame: u32,
    ) {
        let command = frame.command();
        let is_write = command == Command::Write.number();
        let is_operation = command == Command::Operation.number();
        if is_write || is_operation || command == Command::Commit.number() {
            let tenant = self.bound_tenant.get(&key).map(|(tenant, _)| *tenant);
            if let Some(tenant) = tenant.filter(|_| self.host.read_waits(key)) {
                let work = ParkedWork::Turn { pushes, max_frame };
                let _none = self.parked.insert(
                    key,
                    Parked {
                        frame,
                        done,
                        tenant,
                        work,
                    },
                );
                return;
            }
        }
        if is_write {
            self.write(key, frame, done);
        } else if is_operation {
            self.operate(key, frame, done);
        } else {
            let answered = self.answer(key, &frame, &pushes, max_frame);
            // An asker gone has no one to tell.
            drop(done.send(answered));
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

    /// A Write of the transaction `key`: its references asked, and it holds the frame until they are answered.
    fn write(&mut self, key: BoundKey, frame: ReceivedFrame, done: oneshot::Sender<Answer>) {
        let tenant = match self.tenant_of(key, "Write") {
            Ok(tenant) => tenant,
            Err(refusal) => {
                drop(done.send(Answer::Refuse(refusal)));
                return;
            }
        };
        let now = self.now_us();
        let host = &mut self.host;
        match with_changes(&frame, tenant, |changes| {
            host.write_bound(key, changes, now)
        }) {
            Ok(BoundStep::Done) => drop(done.send(self.held(key.1))),
            Ok(BoundStep::Waiting) => {
                let _none = self.parked.insert(
                    key,
                    Parked {
                        frame,
                        done,
                        tenant,
                        work: ParkedWork::Write,
                    },
                );
            }
            Err(refusal) => {
                let _ended = self.bound_tenant.remove(&key);
                self.host.abort_bound(key);
                drop(done.send(Answer::Refuse(refusal)));
            }
        }
    }

    /// An Operation of the transaction `key`.
    fn operate(&mut self, key: BoundKey, frame: ReceivedFrame, done: oneshot::Sender<Answer>) {
        let tenant = match self.tenant_of(key, "Operation") {
            Ok(tenant) => tenant,
            Err(refusal) => {
                drop(done.send(Answer::Refuse(refusal)));
                return;
            }
        };
        let read = (|| {
            let view = frame.view().map_err(|rule| ErrorReply::protocol(&rule))?;
            let operation =
                OperationView::from_frame(&view).map_err(|rule| ErrorReply::protocol(&rule))?;
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
            Ok(BoundStep::Done) => drop(done.send(self.held(key.1))),
            Ok(BoundStep::Waiting) => {
                let _none = self.parked.insert(
                    key,
                    Parked {
                        frame,
                        done,
                        tenant,
                        work: ParkedWork::Operation,
                    },
                );
            }
            Err(refusal) => {
                let _ended = self.bound_tenant.remove(&key);
                self.host.abort_bound(key);
                drop(done.send(Answer::Refuse(refusal)));
            }
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

    /// The parked request of `key`, whose references were answered, on its way: a request behind a read's wait waits on while another
    /// read of its transaction asks.
    fn resume_parked(&mut self, key: BoundKey, parked: Parked) {
        let Parked {
            frame,
            done,
            tenant,
            work,
        } = parked;
        let answer = match work {
            ParkedWork::Turn { pushes, max_frame } if !self.host.read_waits(key) => {
                self.request(key, frame, done, pushes, max_frame);
                return;
            }
            work @ ParkedWork::Turn { .. } => {
                let _none = self.parked.insert(
                    key,
                    Parked {
                        frame,
                        done,
                        tenant,
                        work,
                    },
                );
                return;
            }
            ParkedWork::Write => {
                let now = self.now_us();
                let host = &mut self.host;
                with_changes(&frame, tenant, |changes| {
                    host.resume_write(key, changes, now)
                })
            }
            ParkedWork::Operation => Ok(()),
        };
        let reply = match answer {
            Ok(()) => self.held(key.1),
            Err(refusal) => {
                let _ended = self.bound_tenant.remove(&key);
                self.host.abort_bound(key);
                Answer::Refuse(refusal)
            }
        };
        drop(done.send(reply));
    }

    /// Any request that is not a write: answered now, or later through `pushes`.
    fn answer(
        &mut self,
        key: BoundKey,
        frame: &ReceivedFrame,
        pushes: &Pushes,
        max_frame: u32,
    ) -> Answer {
        let view = match frame.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let command = view.command();
        let answered = if command == Command::Bind {
            self.bind(key, &view, pushes)
        } else if command == Command::Commit {
            self.commit(key, &view, pushes)
        } else if command == Command::Abort {
            self.abort(key)
        } else if command == Command::Drop {
            self.drop_on(key)
        } else if command == Command::Reference {
            self.reference(key, &view, pushes)
        } else if command == Command::Scan {
            self.scan(key, &view, pushes, max_frame)
        } else if command == Command::Closed {
            self.closed_epochs(key.1, &view)
        } else if command == Command::Recall {
            self.recall(key.1, &view)
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
        if bind.kind() == BindKind::Drop {
            let _new = self.drop_streams.insert(key);
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
        self.host.abort_bound(key);
        let _ended = self.bound_tenant.remove(&key);
        Ok(Answer::Done)
    }

    /// A Drop on `key`: on a stream a Bind of kind 1 opened it ends that transaction, and hosting one is the endpoint's task.
    fn drop_on(&mut self, key: BoundKey) -> Result<Answer, ErrorReply> {
        if self.drop_streams.remove(&key) {
            return Err(ErrorReply::new(
                ErrorCode::Unavailable,
                "a Drop is not hosted here yet",
            ));
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
