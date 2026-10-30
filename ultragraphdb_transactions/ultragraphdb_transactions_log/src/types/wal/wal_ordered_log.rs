/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_log::LogError;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{FrameView, MAX_FRAME_SIZE, RoundHeader};
use ultragraphdb_traits_log::{LogEvent, OrderedLog, Position, Round, RoundRef};

use crate::traits::durable_file::DurableFile;
use crate::traits::durable_storage::DurableStorage;
use crate::types::wal::Wal;
use crate::types::wal::wal_entry::WalEntry;
use crate::utils::block::padded;
use crate::utils::frame::encode_round;
use crate::utils::io_error::io_error;

impl<S: DurableStorage> Wal<S> {
    fn refuse_if_poisoned(&self) -> Result<(), LogError> {
        if self.poisoned {
            Err(LogError::Poisoned())
        } else {
            Ok(())
        }
    }

    /// The round of `entry`, read and verified: the frame validates for this cluster, through its chunks, and its
    /// checksum is the one indexed, which covers every field of its header. A failed chunk is named with its range.
    async fn read_entry(&self, entry: &WalEntry) -> Result<Round, LogError> {
        let corrupt = || LogError::Corrupt(entry.position.get(), entry.segment, entry.offset);
        let Some(file) = self.files.get(&entry.segment) else {
            return Err(corrupt());
        };
        let bytes = file
            .read_at(entry.offset, padded(u64::from(entry.size)) as usize)
            .await
            .map_err(io_error("read"))?;
        let frame = bytes
            .as_slice()
            .get(..entry.size as usize)
            .unwrap_or_default();
        let view = FrameView::validate(frame, self.config.cluster(), MAX_FRAME_SIZE).map_err(
            |refusal| match refusal.0 {
                ProtocolErrorEnum::Chunk { index, start, end } => LogError::CorruptChunk(
                    entry.position.get(),
                    entry.segment,
                    entry.offset,
                    index,
                    start,
                    end,
                ),
                _ => corrupt(),
            },
        )?;
        if view.header().checksum() != entry.checksum {
            return Err(corrupt());
        }
        let fields = RoundHeader::decode(view.header().command_header());
        Ok(Round::new(
            entry.position,
            entry.parent,
            entry.checksum,
            fields.first_version(),
            fields.transactions(),
            view.body().to_vec(),
        ))
    }
}

impl<S: DurableStorage> OrderedLog for Wal<S> {
    /// Refused, in this order: [`LogError::Poisoned`] after a failed write or sync; [`LogError::Sealed`];
    /// [`LogError::PositionMismatch`] unless the round is at the position after the tail;
    /// [`LogError::ParentMismatch`] unless its parent is the tail's checksum; [`LogError::TooLarge`] when its frame
    /// exceeds 1 MiB or its padded frame a segment; [`LogError::Busy`] when it would need a segment past the
    /// capacity. Nothing is written before [`OrderedLog::sync`].
    fn append(&mut self, round: RoundRef<'_>) -> Result<Position, LogError> {
        self.refuse_if_poisoned()?;
        if self.sealed {
            return Err(LogError::Sealed(self.tail.0.get()));
        }
        let expected = self.tail.0.next();
        if expected != Some(round.position()) {
            return Err(LogError::PositionMismatch(
                expected.map_or(u64::MAX, |p| p.get()),
                round.position().get(),
            ));
        }
        if round.parent() != self.tail.1 {
            return Err(LogError::ParentMismatch(
                round.position().get(),
                self.tail.1,
                round.parent(),
            ));
        }
        let (frame, checksum) = encode_round(
            self.config.cluster(),
            self.config.release(),
            self.superblock.incarnation(),
            &round,
        )?;
        let size = self.config.segment_size();
        let padded_len = frame.len() as u64;
        if padded_len > size {
            return Err(LogError::TooLarge(frame.len(), size as usize));
        }
        let (mut segment, mut offset) = self.cursor;
        if offset + padded_len > size {
            segment += 1;
            offset = 0;
        }
        let capacity = u64::from(self.config.capacity().get());
        if segment >= self.superblock.first_segment().saturating_add(capacity) {
            return Err(LogError::Busy(capacity));
        }
        let frame_size = frame
            .as_slice()
            .get(40..44)
            .and_then(|field| field.try_into().ok())
            .map_or(0, u32::from_le_bytes);
        let entry = WalEntry {
            position: round.position(),
            parent: round.parent(),
            checksum,
            segment,
            offset,
            size: frame_size,
        };
        self.staged.push_back((entry, frame));
        self.tail = (round.position(), checksum);
        self.cursor = (segment, offset + padded_len);
        Ok(round.position())
    }

    /// Each staged round in order: its segment created, preallocated and synced with its directory when new, then
    /// the round's one write and one sync. A failure poisons the log and is [`LogError::Io`].
    async fn sync(&mut self) -> Result<Position, LogError> {
        self.refuse_if_poisoned()?;
        while let Some((entry, frame)) = self.staged.pop_front() {
            if let Err(failure) = self.write_round(entry, frame).await {
                self.poisoned = true;
                self.staged.clear();
                return Err(failure);
            }
            self.committed = (entry.position, entry.checksum);
            self.index.push_back(entry);
        }
        Ok(self.committed.0)
    }

    fn committed(&self) -> Position {
        self.committed.0
    }

    fn tail(&self) -> (Position, u64) {
        self.tail
    }

    /// Refused as [`LogError::PositionMismatch`] when `from` lies below the first round kept or past the one after
    /// the committed position; a round that no longer verifies is [`LogError::Corrupt`] or
    /// [`LogError::CorruptChunk`]. A failed read is [`LogError::Io`] and does not poison the log.
    async fn read(&mut self, from: Position, max_bytes: u32) -> Result<Vec<Round>, LogError> {
        self.refuse_if_poisoned()?;
        let after = self.committed.0.get().saturating_add(1);
        let first = self
            .index
            .front()
            .map_or(after, |entry| entry.position.get());
        if from.get() < first {
            return Err(LogError::PositionMismatch(first, from.get()));
        }
        if from.get() > after {
            return Err(LogError::PositionMismatch(after, from.get()));
        }
        let mut rounds = Vec::new();
        let mut total = 0u64;
        for entry in self.index.iter().skip((from.get() - first) as usize) {
            let size = u64::from(entry.size);
            if !rounds.is_empty() && total + size > u64::from(max_bytes) {
                break;
            }
            rounds.push(self.read_entry(entry).await?);
            total += size;
        }
        Ok(rounds)
    }

    /// Records `position`, its round's checksum and `version`, the last commit version it covers, as the checkpoint in
    /// the superblock, then removes every segment whose rounds all lie at or below it, short of the segment the next
    /// round is written to. A position at or below the checkpoint changes nothing; one past the committed position is
    /// [`LogError::PositionMismatch`]. A failure poisons the log.
    async fn trim_through(&mut self, position: Position, version: u64) -> Result<(), LogError> {
        self.refuse_if_poisoned()?;
        if position > self.committed.0 {
            return Err(LogError::PositionMismatch(
                self.committed.0.get(),
                position.get(),
            ));
        }
        if position <= self.checkpoint() {
            return Ok(());
        }
        let result = self.checkpoint_at(position, version).await;
        self.poisoned = result.is_err();
        result
    }

    /// The checkpoint's last commit version, from the superblock.
    fn checkpoint_version(&self) -> u64 {
        self.superblock.checkpoint_version()
    }

    fn seal(&mut self) -> Result<Position, LogError> {
        self.refuse_if_poisoned()?;
        self.sealed = true;
        Ok(self.tail.0)
    }

    /// Always `None`: one node has no views and never replaces a round it accepted.
    fn next_event(&mut self) -> Option<LogEvent> {
        None
    }
}
