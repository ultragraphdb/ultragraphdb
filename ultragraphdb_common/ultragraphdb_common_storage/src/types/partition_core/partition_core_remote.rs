/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use compio::runtime::spawn;
use ultragraphdb_cluster_partition::{
    Answer as HomeAnswer, PartitionError, PartitionMessage, Reference as Registration,
    RemoteMessage,
};
use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_common_role::{Answer, RoleClient};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_transport::{Address, TransportError};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorCode, ErrorReply, Members, MembersView, Reference,
    ReferenceView,
};

use crate::types::core_message::Pushes;
use crate::types::partition_core::partition_core_frames::last;
use crate::types::partition_core::{HomeAsking, PartitionCore, Peers};
use crate::utils::error_reply::registration_outcome;

/// Outcomes a home's reply names (`PROTOCOL.md` §13.15).
const OUTCOME_REFUSED: u16 = 7;
const OUTCOME_WRONG_PARTITION: u16 = 26;

impl PartitionCore<'_> {
    /// What the partition sent beyond its process: registrations go out on their own tasks, a home's answers to the
    /// Reference that asked.
    pub(super) fn take_remote(&mut self) {
        while let Ok(message) = self.remote.try_recv() {
            match message {
                RemoteMessage::Register { home, write, batch } => {
                    spawn(register(self.peers.clone(), home, write, batch)).detach();
                }
                RemoteMessage::Answered {
                    write,
                    index,
                    answer,
                } => self.home_answered(write, index, answer),
            }
        }
    }

    /// A Reference reaching this partition as a home: each registration goes to the partition's own mailbox, and its
    /// answers come back through [`PartitionCore::take_remote`].
    pub(super) fn home_asked(
        &mut self,
        stream: u64,
        asked: &ReferenceView<'_>,
        pushes: &Pushes,
    ) -> Result<Answer, ErrorReply> {
        if asked.is_reply() {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                "a Reference reply as a request",
            ));
        }
        let mut reply = asked
            .to_body()
            .map_err(|rule| ErrorReply::protocol(&rule))?
            .into_reply();
        let asking = self.next_asking;
        self.next_asking += 1;
        let mut batch = Vec::new();
        for (index, registration) in asked.registrations().enumerate() {
            if registration.partition() != self.id.get() {
                let _filled = reply.refuse(index, OUTCOME_WRONG_PARTITION);
                continue;
            }
            let iri = registration
                .iri()
                .and_then(|at| reply.terms().get(at))
                .and_then(|term| {
                    if let ultragraphdb_protocol_wire::RequestTerm::Iri(iri) = term {
                        Some(iri.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            batch.push((
                index,
                Registration::new(
                    self.id,
                    asked.tenant(),
                    iri,
                    registration.entity(),
                    registration.group(),
                    registration.generation(),
                ),
            ));
        }
        let unanswered = batch.len();
        if unanswered == 0 {
            let frame = reply
                .frame(last(self.cluster, stream, Command::Reference))
                .map_err(|rule| ErrorReply::protocol(&rule))?;
            return Ok(Answer::Reply(frame));
        }
        let _none = self.home_askings.insert(
            asking,
            HomeAsking {
                stream,
                reply,
                unanswered,
                pushes: pushes.clone(),
            },
        );
        // A mailbox gone is a partition stopping, whose connection closes with it.
        drop(self.own.send(PartitionMessage::Register {
            from: None,
            write: asking,
            batch,
        }));
        Ok(Answer::Open)
    }

    fn home_answered(
        &mut self,
        asking: u64,
        index: usize,
        answer: Result<HomeAnswer, PartitionError>,
    ) {
        let Some(open) = self.home_askings.get_mut(&asking) else {
            return;
        };
        let _filled = match answer {
            Ok(answered) => open.reply.accept(
                index,
                answered.entity(),
                answered.generation().map_or(0, NonZeroU64::get),
                answered.close().get(),
            ),
            Err(refusal) => open.reply.refuse(index, registration_outcome(&refusal)),
        };
        open.unanswered = open.unanswered.saturating_sub(1);
        if open.unanswered > 0 {
            return;
        }
        let Some(done) = self.home_askings.remove(&asking) else {
            return;
        };
        match done
            .reply
            .frame(last(self.cluster, done.stream, Command::Reference))
        {
            // A connection gone asks again; a registration is idempotent.
            Ok(frame) => drop(done.pushes.unbounded_send((done.stream, frame))),
            Err(error) => tracing::error!(%error, "a Reference's reply could not be framed"),
        }
    }
}

/// Sends `batch` of the write `write` to the home `home` in another process, and hands its answers to the partition's
/// own mailbox. A failure is dropped: the host sends unanswered registrations again, and they are idempotent. A dropped
/// future leaves what [`exchange`] says.
async fn register(peers: Peers, home: PartitionId, write: u64, batch: Vec<(usize, Registration)>) {
    if let Err(error) = exchange(&peers, home, write, &batch).await {
        tracing::warn!(home = home.get(), %error, "a registration waits for its next sending");
    }
}

/// One Reference request of `batch` to `home`, each answer handed to the partition's mailbox. A future dropped at a request
/// drops the client it took out of `peers`, so no connection cut inside a frame is used again; answers already handed on
/// stay, and the host sends the rest again.
async fn exchange(
    peers: &Peers,
    home: PartitionId,
    write: u64,
    batch: &[(usize, Registration)],
) -> Result<(), TransportError> {
    let address = address_of(peers, home.get()).await?;
    let mut request = Reference::request();
    if let Some((_, first)) = batch.first() {
        request = request.in_tenant(first.tenant());
    }
    for (_, registration) in batch {
        let _at = match registration.entity() {
            Some(entity) => request.register_entity(
                home.get(),
                registration.group(),
                entity,
                registration.named(),
            ),
            None => request.register_iri(
                home.get(),
                registration.group(),
                registration.iri(),
                registration.named(),
            ),
        };
    }
    let mut client = client_of(peers, &address).await?;
    let reply = client.request(&request).await?;
    let view = reply.view().map_err(TransportError::protocol)?;
    let answered = ReferenceView::from_frame(&view).map_err(TransportError::protocol)?;
    for ((index, _), registration) in batch.iter().zip(answered.registrations()) {
        let answer = if registration.outcome() == 0 {
            match (registration.entity(), Epoch::new(registration.close())) {
                (Some(entity), Ok(close)) => Ok(HomeAnswer::new(
                    entity,
                    NonZeroU64::new(registration.generation()),
                    close,
                )),
                _ => Err(PartitionError::refused(OUTCOME_REFUSED)),
            }
        } else {
            Err(PartitionError::refused(registration.outcome()))
        };
        // A mailbox gone is a partition stopping.
        drop(peers.own.unbounded_send(PartitionMessage::Answered {
            write,
            index: *index,
            answer,
        }));
    }
    drop(reply);
    let _returned = peers.open.borrow_mut().insert(address, client);
    Ok(())
}

/// The connection open to `address`, taken from what the core holds, or dialled. A future dropped at its dial leaves no
/// connection.
async fn client_of(peers: &Peers, address: &str) -> Result<RoleClient, TransportError> {
    let open = peers.open.borrow_mut().remove(address);
    if let Some(client) = open {
        return Ok(client);
    }
    let parsed = Address::parse(address)?;
    RoleClient::dial(
        &parsed,
        peers.cluster.id(),
        &peers.name,
        peers.cluster.secret().password(),
    )
    .await
}

/// Where `partition` listens, asked of the frontend when the core does not know. A future dropped at its dial or request
/// drops that connection to the frontend, and the addresses known stay as they were.
async fn address_of(peers: &Peers, partition: u32) -> Result<String, TransportError> {
    if let Some(known) = peers.addresses.borrow().get(&partition) {
        return Ok(known.clone());
    }
    let mut client = RoleClient::dial(
        &peers.frontend,
        peers.cluster.id(),
        &peers.name,
        peers.cluster.secret().password(),
    )
    .await?;
    let answer = client.request(&Members::new()).await?;
    let view = answer.view().map_err(TransportError::protocol)?;
    let members = MembersView::from_frame(&view).map_err(TransportError::protocol)?;
    let mut known = peers.addresses.borrow_mut();
    for index in 0..members.len() {
        if let (Some(member), Some(address)) = (members.member(index), members.address(index))
            && member.partition() > 0
        {
            let _previous = known.insert(member.partition(), address.to_string());
        }
    }
    known.get(&partition).cloned().ok_or_else(|| {
        TransportError::authentication(format!("no address for partition {partition}"))
    })
}
