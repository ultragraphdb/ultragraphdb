/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_log::LogError;
use ultragraphdb_traits_log::Position;

use crate::traits::durable_file::DurableFile;
use crate::traits::durable_storage::DurableStorage;
use crate::types::aligned_buf::AlignedBuf;
use crate::types::damage::Damage;
use crate::types::scan_start::ScanStart;
use crate::types::segment_scan::SegmentScan;
use crate::types::superblock::Superblock;
use crate::types::truncation::Truncation;
use crate::types::wal::Wal;
use crate::types::wal::wal_entry::WalEntry;
use crate::types::wal::wal_stop::WalStop;
use crate::types::wal::wal_write::reserve;
use crate::utils::block::{BLOCK, padded};
use crate::utils::io_error::io_error;
use crate::utils::scan::{find_complete_round, scan_segment};
use crate::utils::zeros::write_zeros;

impl<S: DurableStorage> Wal<S> {
    /// The bytes of segment `index` the log reads, its whole blocks up to the segment size; `None` when the segment
    /// does not exist. A segment is never read past the configured size, which bounds the allocation.
    async fn segment_bytes(&self, index: u64) -> Result<Option<AlignedBuf>, LogError> {
        let Some(file) = self.files.get(&index) else {
            return Ok(None);
        };
        let len = file.length().await.map_err(io_error("length"))?;
        let len = len.min(self.config.segment_size()) / BLOCK as u64 * BLOCK as u64;
        let bytes = file
            .read_at(0, len as usize)
            .await
            .map_err(io_error("read"))?;
        Ok(Some(bytes))
    }

    /// Recovers the log from its segments: the chain of rounds from the first segment kept, where it stops, whether
    /// the stop is a torn tail (truncated, reported) or interior damage (refused), the tail and cursor after it.
    pub(crate) async fn recover(&mut self) -> Result<Option<Truncation>, LogError> {
        let (stop, start) = self.scan_chain().await?;
        let expected = start
            .next()
            .map_or(self.checkpoint().get().saturating_add(1), |(p, _)| p.get());
        self.refuse_interior_damage(stop, expected).await?;
        let truncation = stop.damage.map(|damage| {
            let position = match damage {
                Damage::Chunk { position, .. } | Damage::ChunkTable { position } => position,
                Damage::Header | Damage::Chain { .. } => Position::new(expected),
            };
            tracing::warn!(
                position = position.get(),
                segment = stop.segment,
                offset = stop.offset,
                damage = ?damage,
                "truncating a torn tail of the write-ahead log"
            );
            Truncation::new(position, stop.segment, stop.offset, damage)
        });
        if let Some(last) = self.index.back() {
            self.tail = (last.position, last.checksum);
        }
        self.cursor = (stop.segment, stop.offset);
        if self.tail.0 < self.checkpoint() {
            self.restart_at_checkpoint(stop.segment + 1).await?;
        }
        self.committed = self.tail;
        self.sync_recovered().await?;
        if truncation.is_some() {
            self.clear_after_damage(stop).await?;
        }
        Ok(truncation)
    }

    /// Scans the segments from the first kept, following the chain into the next segment when its first block is the
    /// next round (a round that did not fit the rest of a segment starts the next one). Returns where the chain stopped
    /// and the scan's expectation there.
    async fn scan_chain(&mut self) -> Result<(WalStop, ScanStart), LogError> {
        let mut start = ScanStart::new(
            self.config.cluster(),
            self.superblock.incarnation(),
            self.checkpoint(),
            self.superblock.checkpoint_checksum(),
        );
        let size = self.config.segment_size();
        let mut segment = self.superblock.first_segment();
        let mut scan = self.scan(segment, &start).await?.unwrap_or_else(empty);
        loop {
            self.index_rounds(segment, &scan);
            if let Some(last) = scan.rounds().last()
                && let Some(next) = last.position().next()
            {
                start = start.continuing(next, last.checksum());
            }
            let peek = self.scan(segment + 1, &start).await?;
            let peeked_damage = match peek {
                Some(next) if !next.rounds().is_empty() => {
                    segment += 1;
                    scan = next;
                    continue;
                }
                Some(next) => next.damage(),
                None => None,
            };
            let stop = match (scan.damage(), peeked_damage) {
                (Some(damage), _) => WalStop {
                    segment,
                    offset: scan.end(),
                    damage: Some(damage),
                },
                (None, Some(damage)) => WalStop {
                    segment: segment + 1,
                    offset: 0,
                    damage: Some(damage),
                },
                (None, None) if scan.end() == size => WalStop {
                    segment: segment + 1,
                    offset: 0,
                    damage: None,
                },
                (None, None) => WalStop {
                    segment,
                    offset: scan.end(),
                    damage: None,
                },
            };
            return Ok((stop, start));
        }
    }

    /// Segment `index` scanned from `start`, `None` when it does not exist.
    async fn scan(&self, index: u64, start: &ScanStart) -> Result<Option<SegmentScan>, LogError> {
        match self.segment_bytes(index).await? {
            Some(bytes) => Ok(Some(scan_segment(bytes.as_slice(), start)?)),
            None => Ok(None),
        }
    }

    fn index_rounds(&mut self, segment: u64, scan: &SegmentScan) {
        self.index
            .extend(scan.rounds().iter().map(|round| WalEntry {
                position: round.position(),
                parent: round.parent(),
                checksum: round.checksum(),
                segment,
                offset: round.offset(),
                size: round.size(),
            }));
    }

    /// Refuses the start when a complete round of this incarnation, at the expected position or later, lies past the
    /// stop: the damage, or the gap, is then not a torn tail, and truncating would drop acknowledged rounds.
    async fn refuse_interior_damage(&self, stop: WalStop, expected: u64) -> Result<(), LogError> {
        let cluster = self.config.cluster();
        let incarnation = self.superblock.incarnation();
        let at = Position::new(expected);
        let last = self
            .files
            .keys()
            .next_back()
            .copied()
            .unwrap_or(stop.segment);
        let mut from = stop.offset as usize + 1;
        for segment in stop.segment..=last {
            if let Some(bytes) = self.segment_bytes(segment).await?
                && find_complete_round(bytes.as_slice(), from, cluster, incarnation, at).is_some()
            {
                return Err(match stop.damage {
                    Some(Damage::Chunk {
                        position,
                        index,
                        start,
                        end,
                    }) => LogError::CorruptChunk(
                        position.get(),
                        stop.segment,
                        stop.offset,
                        index,
                        start,
                        end,
                    ),
                    Some(Damage::ChunkTable { position }) => {
                        LogError::Corrupt(position.get(), stop.segment, stop.offset)
                    }
                    _ => LogError::Corrupt(expected, stop.segment, stop.offset),
                });
            }
            from = 0;
        }
        Ok(())
    }

    /// The log holds nothing above its checkpoint, so it continues from the checkpoint in segment `first`, a segment
    /// of its own: the superblock records `first` as the first kept, and the segments below it, holding only rounds the
    /// checkpoint covers, are recycled.
    async fn restart_at_checkpoint(&mut self, first: u64) -> Result<(), LogError> {
        self.tail = (self.checkpoint(), self.superblock.checkpoint_checksum());
        self.cursor = (first, 0);
        self.index.clear();
        let record = Superblock::new(
            self.superblock.sequence() + 1,
            self.config.cluster(),
            self.superblock.incarnation(),
            self.checkpoint(),
            self.superblock.checkpoint_checksum(),
            self.superblock.checkpoint_version(),
            first,
        );
        record.write(&self.storage, &self.dir).await?;
        self.superblock = record;
        self.recycle_segments_below(first).await
    }

    /// Syncs every segment holding a recovered round, and reserves the blocks of the segment the cursor is in again
    /// if it is short (a crash may have cut its creation short of its preallocation): what a crashed process left in
    /// the page cache was read back as rounds, and is not committed until it is durable.
    async fn sync_recovered(&mut self) -> Result<(), LogError> {
        let held: Vec<u64> = self.index.iter().map(|entry| entry.segment).collect();
        let files = self
            .files
            .iter()
            .filter(|(index, _)| held.binary_search(index).is_ok());
        for (_, file) in files {
            file.sync().await.map_err(io_error("file sync"))?;
        }
        let size = self.config.segment_size();
        if let Some(file) = self.files.get_mut(&self.cursor.0)
            && let length = file.length().await.map_err(io_error("length"))?
            && length < size
        {
            // A short tear can leave the length inside a block, whose bytes the scan never read.
            let from = length / BLOCK as u64 * BLOCK as u64;
            reserve(file, from, size, &mut self.unreserved).await?;
            file.sync().await.map_err(io_error("file sync"))?;
        }
        Ok(())
    }

    /// Writes zeros over the segment after the damaged block, up to its length rounded up to a whole block, and syncs
    /// them, so a round later
    /// written over that block, shorter than the one torn there, leaves no stale bytes behind it; past the length a
    /// segment holds no bytes, only blocks reserved for later writes. The damaged block itself is left for the next
    /// append to overwrite: zeroing it first would destroy the only evidence of the torn round, and a failure after
    /// that would let the next open start without reporting it. Until a round is written there, every open finds the
    /// same torn tail and reports it again.
    async fn clear_after_damage(&mut self, stop: WalStop) -> Result<(), LogError> {
        let at = stop.offset + BLOCK as u64;
        let Some(file) = self.files.get_mut(&stop.segment) else {
            return Ok(());
        };
        let length = file.length().await.map_err(io_error("length"))?;
        let size = self.config.segment_size().min(padded(length));
        if at >= size {
            return Ok(());
        }
        write_zeros(file, at, size)
            .await
            .map_err(io_error("write"))?;
        file.sync().await.map_err(io_error("file sync"))
    }
}

fn empty() -> SegmentScan {
    SegmentScan::new(Vec::new(), 0, None)
}
