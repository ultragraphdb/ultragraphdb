/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod metadata_group_access;
mod metadata_group_blocks;
mod metadata_group_catalog;
mod metadata_group_recover;
mod metadata_group_snapshot;

use std::collections::VecDeque;
use std::num::NonZeroU32;

use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_error_log::LogError;
use ultragraphdb_traits_log::{DurabilityTicket, OrderedLog, Position, RoundRef};
use xxhash_rust::xxh3::xxh3_64;

use crate::types::metadata_command::MetadataCommand;
use crate::types::metadata_config::MetadataConfig;
use crate::types::metadata_state::MetadataState;

/// The derivation version the group's rounds name: they hold no quads, so the first.
const DERIVATION: NonZeroU32 = NonZeroU32::MIN;

/// The metadata group: a state machine over the ordered log `L` holding the cluster's metadata, with one replica in this
/// stage behind the interface stage 8 replicates.
///
/// Its state is a function of its log. Every command is decided against the state of every command appended before it,
/// appended as a [`MetadataCommand`](crate::MetadataCommand), one per round or, when large, as chunks and their commit,
/// and applied to the state the group answers
/// from ([`state`](Self::state)) only once [`sync`](Self::sync) has made it durable, so a command is acknowledged only
/// once durable and a read never sees a command a crash could lose. Each command method returns the log position the
/// command was appended at, which the caller answers once `sync` reports a durable position at or past it.
/// [`recover`](Self::recover) rebuilds the state from the last [`MetadataSnapshot`](crate::MetadataSnapshot) and the
/// rounds after it. The methods that decide are synchronous and do no I/O; `recover`, `sync` and `trim` wait on the
/// log. Control plane: a command is encoded into one reused buffer.
pub struct MetadataGroup<L: OrderedLog> {
    log: L,
    config: MetadataConfig,
    /// The state of every durable command, which reads answer from.
    state: MetadataState,
    /// The state of every command appended, durable or not, which decides the next command.
    head: MetadataState,
    /// The commands appended and not yet durable, in log order, each at its position.
    staged: VecDeque<(Position, MetadataCommand)>,
    /// The position of the last command applied to `state`.
    applied: Position,
    /// The position the last snapshot was taken at, or the group recovered from.
    snapshot_at: Position,
    /// The commands the last recovery replayed after its snapshot.
    replayed: u64,
    /// The buffer every command is encoded into, reused.
    buffer: Vec<u8>,
}

impl<L: OrderedLog> MetadataGroup<L> {
    /// The settings the group runs with.
    #[must_use]
    pub fn config(&self) -> &MetadataConfig {
        &self.config
    }

    /// The state of every durable command, as of [`applied`](Self::applied).
    #[must_use]
    pub fn state(&self) -> &MetadataState {
        &self.state
    }

    /// The log position the state reads at: its last durable command's, or the snapshot's before any.
    #[must_use]
    pub fn applied(&self) -> Position {
        self.applied
    }

    /// The commands the recovery that started the group replayed after its snapshot, a chunked one counted once.
    #[must_use]
    pub fn replayed(&self) -> u64 {
        self.replayed
    }

    /// Makes every appended command durable, applies it to the state reads answer from, and returns the durable
    /// position.
    ///
    /// # Errors
    ///
    /// [`ClusterError::log`] carrying the log's refusal; a failed sync poisons the log, so every later append fails;
    /// the refusal of [`MetadataState::apply`](crate::MetadataState::apply) when a durable command does not apply as it
    /// did when decided, a bug.
    ///
    /// A future dropped at the log's sync leaves the staged commands staged and the state reads answer from unchanged;
    /// a later `sync` applies them.
    pub async fn sync(&mut self) -> Result<Position, ClusterError> {
        let durable = self.log.sync().await?;
        // Each staged command became durable in log order; the staged are bounded by the log's capacity.
        while let Some((next, _)) = self.staged.front() {
            if *next > durable {
                break;
            }
            let Some((position, command)) = self.staged.pop_front() else {
                break;
            };
            self.state.apply(&command)?;
            self.applied = position;
        }
        Ok(durable)
    }

    /// Applies `command` to the head state and appends it: as one round when its encoding fits
    /// [`MetadataCommand::CHUNK_BYTES`], or else as a run of chunks and their commit, the command taking effect at the
    /// commit's position, which it returns. On a refusal nothing changes in the state; a run the log refused partway
    /// leaves chunks with no commit, which replay discards.
    fn append(&mut self, command: MetadataCommand) -> Result<Position, ClusterError> {
        self.buffer.clear();
        command.encode(&mut self.buffer);
        self.head.apply(&command)?;
        // Within COMMAND_BYTES_MAX: the state's limits on a version's bytes and a change's ranges bound every command.
        let appended = if self.buffer.len() <= MetadataCommand::CHUNK_BYTES {
            append_body(&mut self.log, &self.buffer)
        } else {
            self.append_chunked()
        };
        match appended {
            Ok(position) => {
                self.staged.push_back((position, command));
                Ok(position)
            }
            Err(refusal) => {
                self.rebuild_head()?;
                Err(refusal)
            }
        }
    }

    /// Appends the encoding in the buffer as chunks of [`MetadataCommand::CHUNK_BYTES`] and their commit; returns the
    /// commit's position.
    fn append_chunked(&mut self) -> Result<Position, ClusterError> {
        let mut round = Vec::with_capacity(MetadataCommand::CHUNK_BYTES + 32);
        let mut offset = 0_u64;
        for piece in self.buffer.chunks(MetadataCommand::CHUNK_BYTES) {
            round.clear();
            MetadataCommand::Chunk {
                offset,
                bytes: piece.to_vec(),
            }
            .encode(&mut round);
            let _chunk: Position = append_body(&mut self.log, &round)?;
            // At most COMMAND_BYTES_MAX, so the cast is lossless and the offset cannot overflow.
            offset += piece.len() as u64;
        }
        round.clear();
        MetadataCommand::Commit {
            length: offset,
            checksum: xxh3_64(&self.buffer),
        }
        .encode(&mut round);
        append_body(&mut self.log, &round)
    }

    /// The head state rebuilt from the durable state and the staged commands, after a refused append left it ahead of
    /// the log.
    fn rebuild_head(&mut self) -> Result<(), ClusterError> {
        let mut head = self.state.clone();
        for (_, command) in &self.staged {
            head.apply(command)?;
        }
        self.head = head;
        Ok(())
    }
}

/// Appends `body` to `log` as the round after its tail.
fn append_body<L: OrderedLog>(log: &mut L, body: &[u8]) -> Result<Position, ClusterError> {
    let (tail, parent) = log.tail();
    let position = tail.next().ok_or_else(|| LogError::exhausted(tail.get()))?;
    let round = RoundRef::new(position, parent, 0, 1, DERIVATION, body);
    let _ticket: DurabilityTicket = log.append(round)?;
    Ok(position)
}
