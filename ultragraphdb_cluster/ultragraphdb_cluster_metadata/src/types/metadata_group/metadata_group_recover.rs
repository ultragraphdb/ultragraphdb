/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;

use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_traits_log::{OrderedLog, Position};
use xxhash_rust::xxh3::xxh3_64;

use crate::types::metadata_command::MetadataCommand;
use crate::types::metadata_config::MetadataConfig;
use crate::types::metadata_group::MetadataGroup;
use crate::types::metadata_snapshot::MetadataSnapshot;
use crate::types::metadata_state::MetadataState;

/// The bytes of rounds one read of the log returns at most, 1 MiB: a round's frame at its largest.
const READ_BYTES: u32 = 1 << 20;

impl<L: OrderedLog> MetadataGroup<L> {
    /// The group over `log`, its state rebuilt from `snapshot`, the last the host stored, or from the empty state when
    /// it has none, by replaying every durable round after the snapshot's position in order. A large command's chunks
    /// with no commit after them, a crash having cut the run, are discarded. An empty log and no
    /// snapshot start a fresh group.
    ///
    /// # Errors
    ///
    /// [`ClusterError::snapshot_behind`] when the snapshot, or the empty state at position 0, lies below the log's
    /// checkpoint; [`ClusterError::replay_diverged`] when the snapshot lies past the log's committed position, a round
    /// is not at the position after the one before it, a chunk does not continue the chunks before it, a commit does not
    /// name them or they hold no command, or a round's command does not apply to the state before it;
    /// [`ClusterError::malformed`] and [`ClusterError::too_many`] when a round's body is no command;
    /// [`ClusterError::log`] when the log cannot be read.
    pub async fn recover(
        log: L,
        config: MetadataConfig,
        snapshot: Option<MetadataSnapshot>,
    ) -> Result<Self, ClusterError> {
        let (base, state) = snapshot.map_or((Position::ZERO, MetadataState::new()), |snapshot| {
            (snapshot.position(), snapshot.into_state())
        });
        let checkpoint = log.checkpoint();
        if base < checkpoint {
            return Err(ClusterError::snapshot_behind(base.get(), checkpoint.get()));
        }
        if base > log.committed() {
            return Err(ClusterError::replay_diverged(
                base.get(),
                "a snapshot at or below the log's committed position",
            ));
        }
        let mut group = Self {
            log,
            config,
            head: state.clone(),
            state,
            staged: VecDeque::new(),
            applied: base,
            snapshot_at: base,
            replayed: 0,
            buffer: Vec::new(),
        };
        group.replay().await?;
        group.head = group.state.clone();
        Ok(group)
    }

    /// Replays every durable round after the applied position, in order, reassembling the chunks of a large command
    /// until its commit; chunks no commit follows, before another command or at the log's end, are discarded.
    async fn replay(&mut self) -> Result<(), ClusterError> {
        let committed = self.log.committed();
        let mut last = self.applied;
        let mut chunks = Vec::new();
        // Each read returns at least one round while any is left, so the rounds left bound the reads.
        for _ in 0..committed.get().saturating_sub(last.get()) {
            if last >= committed {
                break;
            }
            let rounds = self
                .log
                .read(Position::new(last.get() + 1), READ_BYTES)
                .await?;
            for round in &rounds {
                let position = round.position();
                if position.get() != last.get() + 1 {
                    return Err(ClusterError::replay_diverged(
                        position.get(),
                        "a round at the position after the one before it",
                    ));
                }
                last = position;
                self.replay_round(position, round.body(), &mut chunks)?;
            }
        }
        Ok(())
    }

    /// Replays the round at `position` holding `body`, `chunks` the pieces of a large command read before it.
    fn replay_round(
        &mut self,
        position: Position,
        body: &[u8],
        chunks: &mut Vec<u8>,
    ) -> Result<(), ClusterError> {
        let diverged = |what| ClusterError::replay_diverged(position.get(), what);
        let command = match MetadataCommand::decode(body)? {
            MetadataCommand::Chunk { offset, bytes } => {
                if offset == 0 {
                    chunks.clear();
                }
                if offset != chunks.len() as u64 {
                    return Err(diverged("a chunk at the end of the chunks before it"));
                }
                if chunks.len() + bytes.len() > MetadataCommand::COMMAND_BYTES_MAX {
                    return Err(diverged("chunks within the most bytes of a command"));
                }
                chunks.extend_from_slice(&bytes);
                return Ok(());
            }
            MetadataCommand::Commit { length, checksum } => {
                let whole = !chunks.is_empty()
                    && length == chunks.len() as u64
                    && checksum == xxh3_64(chunks);
                if !whole {
                    return Err(diverged("a commit naming the chunks before it"));
                }
                let command = MetadataCommand::decode(chunks)
                    .map_err(|_| diverged("chunks holding a command"))?;
                chunks.clear();
                command
            }
            command @ (MetadataCommand::OfferCatalog { .. }
            | MetadataCommand::RecordDefault { .. }
            | MetadataCommand::LeaseBlock { .. }
            | MetadataCommand::InitPlacement { .. }
            | MetadataCommand::ChangePlacement { .. }
            | MetadataCommand::SetLeader { .. }
            | MetadataCommand::CreatePrincipal { .. }
            | MetadataCommand::RemovePrincipal { .. }
            | MetadataCommand::RegisterSession { .. }
            | MetadataCommand::LearnClose { .. }
            | MetadataCommand::ReportUntrimmed { .. }
            | MetadataCommand::CreateTenant { .. }
            | MetadataCommand::DrawSecret { .. }) => {
                // A run of chunks no commit followed is no command: it is discarded.
                chunks.clear();
                command
            }
        };
        self.state.apply(&command).map_err(|refusal| {
            let what = if let ClusterErrorEnum::Contradicts { what } = refusal.0 {
                what
            } else {
                "a command the state before it takes"
            };
            ClusterError::replay_diverged(position.get(), what)
        })?;
        self.applied = position;
        self.replayed += 1;
        Ok(())
    }
}
