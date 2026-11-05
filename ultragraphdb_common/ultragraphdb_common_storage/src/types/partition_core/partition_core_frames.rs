/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::{BoundKey, BoundStep, PartitionError};
use ultragraphdb_common_role::Answer;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    BindKind, BindView, BodyBuilder, BodyView, Command, CommitView, ErrorCode, ErrorReply,
    FLAG_LAST, FrameView, Header, OperationView, ReferenceView, WriteView,
};
use ultragraphdb_transactions_txn::{FrameChanges, FrameTerms};

use crate::types::core_message::{CoreMessage, Pushes};
use crate::types::partition_core::{Committing, Parked, PartitionCore};
use crate::utils::error_reply::error_reply;
use crate::utils::operation_of::operation_of;

/// The write `frame` carries, its terms lent by the frame: what `run` makes of its changes.
fn with_changes<T>(
    frame: &ReceivedFrame,
    namespace: u64,
    run: impl FnOnce(&FrameChanges<'_>) -> Result<T, PartitionError>,
) -> Result<T, ErrorReply> {
    let view = frame.view().map_err(|rule| ErrorReply::protocol(&rule))?;
    let write = WriteView::from_frame(&view).map_err(|rule| ErrorReply::protocol(&rule))?;
    let terms = FrameTerms::of(write.terms()).map_err(|rule| ErrorReply::protocol(&rule))?;
    let changes =
        FrameChanges::new(namespace, &write, &terms).map_err(|rule| ErrorReply::protocol(&rule))?;
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
            } => {
                let key = (connection, stream);
                let command = frame.command();
                let is_write = command == Command::Write.number();
                let is_operation = command == Command::Operation.number();
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
            CoreMessage::Cancelled { connection, stream } => {
                let key = (connection, stream);
                self.host.abort_bound(key);
                let _ended = self.bound_namespace.remove(&key);
                let _parked = self.parked.remove(&key);
                self.cancel_scan(key);
            }
            CoreMessage::Ready { connection, stream } => self.scan_room((connection, stream)),
            CoreMessage::Left { connection, stream } => self.scan_left((connection, stream)),
            CoreMessage::Closed { connection } => {
                self.host.abort_connection(connection);
                self.bound_namespace.retain(|(of, _), _| *of != connection);
                self.parked.retain(|(of, _), _| *of != connection);
                self.close_scans(connection);
            }
        }
    }

    /// A Write of the transaction `key`: its references asked, and it holds the frame until they are answered.
    fn write(
        &mut self,
        key: BoundKey,
        frame: ReceivedFrame,
        done: futures::channel::oneshot::Sender<Answer>,
    ) {
        let Some(namespace) = self
            .bound_namespace
            .get(&key)
            .map(|(namespace, _)| *namespace)
        else {
            drop(done.send(ended()));
            return;
        };
        let now = self.now_us();
        let host = &mut self.host;
        match with_changes(&frame, namespace, |changes| {
            host.write_bound(key, changes, now)
        }) {
            Ok(BoundStep::Done) => drop(done.send(Answer::Open)),
            Ok(BoundStep::Waiting) => {
                let _none = self.parked.insert(
                    key,
                    Parked {
                        frame,
                        done,
                        namespace,
                        is_write: true,
                    },
                );
            }
            Err(refusal) => {
                let _ended = self.bound_namespace.remove(&key);
                self.host.abort_bound(key);
                drop(done.send(Answer::Refuse(refusal)));
            }
        }
    }

    /// An Operation of the transaction `key`.
    fn operate(
        &mut self,
        key: BoundKey,
        frame: ReceivedFrame,
        done: futures::channel::oneshot::Sender<Answer>,
    ) {
        let Some(namespace) = self
            .bound_namespace
            .get(&key)
            .map(|(namespace, _)| *namespace)
        else {
            drop(done.send(ended()));
            return;
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
            Ok(BoundStep::Done) => drop(done.send(Answer::Open)),
            Ok(BoundStep::Waiting) => {
                let _none = self.parked.insert(
                    key,
                    Parked {
                        frame,
                        done,
                        namespace,
                        is_write: false,
                    },
                );
            }
            Err(refusal) => {
                let _ended = self.bound_namespace.remove(&key);
                self.host.abort_bound(key);
                drop(done.send(Answer::Refuse(refusal)));
            }
        }
    }

    /// Runs each write whose references were answered, from the frame it held.
    pub(super) fn resume_ready(&mut self) {
        let now = self.now_us();
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
            let answer = if parked.is_write {
                let host = &mut self.host;
                with_changes(&parked.frame, parked.namespace, |changes| {
                    host.resume_write(key, changes, now)
                })
            } else {
                Ok(())
            };
            let reply = match answer {
                Ok(()) => Answer::Open,
                Err(refusal) => {
                    let _ended = self.bound_namespace.remove(&key);
                    self.host.abort_bound(key);
                    Answer::Refuse(refusal)
                }
            };
            drop(parked.done.send(reply));
        }
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
            self.host.abort_bound(key);
            let _ended = self.bound_namespace.remove(&key);
            Ok(Answer::Done)
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
        if bind.kind() == BindKind::Drop {
            return Err(ErrorReply::new(
                ErrorCode::Unavailable,
                "a Drop is not hosted here yet",
            ));
        }
        let epoch = Epoch::new(bind.epoch())
            .map_err(|error| ErrorReply::new(ErrorCode::Protocol, error.to_string()))?;
        let now = self.now_us();
        let read_version = self
            .host
            .bind(
                key,
                bind.transaction(),
                (bind.namespace(), bind.principal(), epoch),
                now,
            )
            .map_err(|refusal| error_reply(&refusal))?;
        let _previous = self
            .bound_namespace
            .insert(key, (bind.namespace(), pushes.clone()));
        let reply = bind
            .to_body()
            .map_err(|rule| ErrorReply::protocol(&rule))?
            .with_read_version(read_version);
        let header = Header::new(self.cluster, Command::Bind).with_stream(key.1);
        built(&reply, header)
    }

    fn commit(
        &mut self,
        key: BoundKey,
        view: &FrameView<'_>,
        pushes: &Pushes,
    ) -> Result<Answer, ErrorReply> {
        let commit = CommitView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        let session = u128::from_be_bytes(commit.session());
        let now = self.now_us();
        let _ended = self.bound_namespace.remove(&key);
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

fn ended() -> Answer {
    Answer::Refuse(ErrorReply::new(
        ErrorCode::Protocol,
        "the stream holds no transaction",
    ))
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
