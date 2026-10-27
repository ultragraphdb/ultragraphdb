/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The write-ahead log: rounds as checksummed, chained frames in preallocated segments, the superblock and
//! recovery, written through a storage interface over compio and simulated.
//!
//! The pieces, from the disk up: [`DurableStorage`] and [`DurableFile`], the storage seam, implemented by
//! [`CompioStorage`] and the deterministic [`SimStorage`]; [`encode_round`], a round as the frame the log writes;
//! [`scan_segment`] and [`find_complete_round`], the recovery's reading of a segment's bytes; [`Superblock`], its two
//! copies and [`SuperblockRead`]'s three outcomes; and [`Wal`], the log itself, opened with a [`WalConfig`] and
//! reporting its [`Recovery`].

mod traits;
mod types;
mod utils;

pub use crate::traits::durable_file::DurableFile;
pub use crate::traits::durable_storage::DurableStorage;
pub use crate::types::aligned_buf::AlignedBuf;
pub use crate::types::compio_file::CompioFile;
pub use crate::types::compio_storage::CompioStorage;
pub use crate::types::damage::Damage;
pub use crate::types::fault_mode::FaultMode;
pub use crate::types::open_mode::OpenMode;
pub use crate::types::recovery::Recovery;
pub use crate::types::scan_start::ScanStart;
pub use crate::types::scanned_round::ScannedRound;
pub use crate::types::segment_scan::SegmentScan;
pub use crate::types::sim_file::SimFile;
pub use crate::types::sim_storage::SimStorage;
pub use crate::types::storage_operation::StorageOperation;
pub use crate::types::superblock::Superblock;
pub use crate::types::superblock_read::SuperblockRead;
pub use crate::types::truncation::Truncation;
pub use crate::types::wal::Wal;
pub use crate::types::wal_config::WalConfig;
pub use crate::utils::frame::encode_round;
pub use crate::utils::scan::{find_complete_round, scan_segment};
