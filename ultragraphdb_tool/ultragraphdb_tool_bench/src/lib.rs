/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `store_bench`: the storage measurements of stage 2, task 10.2. It drives the apply thread of
//! `ultragraphdb_store_apply` (a `ChangesetApplier` over `FjallStore` with `MokaListCache`) with commits shaped as the
//! Dgraph benchmark runs wrote them, at a fixed offered rate, in apply-step batches; and it measures rollups and the
//! compaction filter over one large predicate, and the store's files after each run.
//!
//! ```text
//! store_bench apply  --dir <path> --rate <commits/s> [--relations 1] [--seconds 30]
//!                    [--durability process|filesystem] [--rollups on|off] [--cache-mb 128]
//! store_bench rollup --dir <path> [--hubs 10000] [--versions 1000] [--relations 50]
//! ```
//!
//! `wal_bench`, stage 5's task 12.5, drives the same commits through stage 5's commit path: each a read-write
//! transaction at the visibility watermark, its proposal decided by the commit pipeline in rounds, each round appended
//! to the write-ahead log on compio and synced, then applied to fjall unsynced; it reports the rounds and the bytes the
//! log's segments and fjall's journal took for the same commits.
//!
//! ```text
//! wal_bench --dir <path> --rate <commits/s> [--relations 1] [--seconds 30] [--cache-mb 128] [--plp off|on]
//! ```
//!
//! Given `--partitions`, `wal_bench` runs N partitions in one process, each on a pinned compio runtime with its own store,
//! log and generator seeded from the run's seed, a share of its edges pointing at other partitions' entities, so
//! registration runs; it reports each partition's and the total relations and syncs a second, the disk bytes, and the
//! registrations a second and the wait a first reference costs. `--verify on` compares each reverse read at the last
//! closed epoch with the same commits on one partition.
//!
//! ```text
//! wal_bench --partitions <n> --dir <path> --rate <commits/s per partition> [--relations 1] [--seconds 30]
//!           [--commits 0] [--cross-percent 0] [--seed 1] [--cache-mb 128] [--verify off|on]
//! ```
//!
//! Each run prints one JSON line. The library holds what the runs are built of, so it is tested apart from the
//! clock: the arguments, the commit shape, the rate pacer behind the [`Clock`] trait, the start that hands a run's
//! threads its clock once each is set up, the summaries, the readers of `/proc`, the store wrapper timing each write
//! call, the wrappers counting list reads and the directory sizes.

mod errors;
mod traits;
mod types;
mod utils;

pub use crate::errors::bench_error::{BenchError, BenchErrorEnum};
pub use crate::traits::clock::Clock;
pub use crate::types::apply_args::ApplyArgs;
pub use crate::types::command::Command;
pub use crate::types::commit_shape::CommitShape;
pub use crate::types::counted_snapshot::{CountedScan, CountedSnapshot};
pub use crate::types::counting_cache::CountingCache;
pub use crate::types::dir_sizes::DirSizes;
pub use crate::types::pacer::Pacer;
pub use crate::types::partition_args::PartitionArgs;
pub use crate::types::partition_shape::PartitionShape;
pub use crate::types::pipeline_args::PipelineArgs;
pub use crate::types::read_counts::ReadCounts;
pub use crate::types::rollup_args::RollupArgs;
pub use crate::types::run_start::RunStart;
pub use crate::types::run_start_party::RunStartParty;
pub use crate::types::size::Size;
pub use crate::types::summary::Summary;
pub use crate::types::timed_store::TimedStore;
pub use crate::types::wall_clock::WallClock;
pub use crate::types::write_call::WriteCall;
pub use crate::utils::apply_run::run_apply;
pub use crate::utils::partitions_run::run_partitions;
pub use crate::utils::pipeline_run::run_pipeline;
pub use crate::utils::proc_stats::{parse_schedstat_ns, parse_stat_cpu_ticks, parse_status_kib};
pub use crate::utils::rollup_run::run_rollup;
pub use crate::utils::setup::wait_out_build;
pub use crate::utils::wal_bench_run::run_wal_bench;
