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
//! Given `--owner-checks`, `wal_bench` times the frontend's owner check as an endpoint core meets it: the production
//! owner on its own thread over its logs in the run's directory, closing and syncing epochs within `--epoch-ms`, one
//! session's `begin_commit` asked again and again, each from its ask to its answer.
//!
//! ```text
//! wal_bench --owner-checks <n> --dir <path> [--epoch-ms 2,10]
//! ```
//!
//! Given `--inbound`, `wal_bench` probes an inbound point read (P24's accepted cost): over N partitions, for each
//! in-degree an object with that many subjects pointing at it, each subject's edge a commit of its own partition, then
//! the object's reverse read at the last closed epoch, fanned out to the partitions its referrer set names, beside the
//! same read on one partition; it reports the partitions asked and each side's posting lists, cache hits, store reads
//! and point reads per read and its time.
//!
//! ```text
//! wal_bench --inbound <reads> --partitions <n> --dir <path> [--degrees 1,20,1000] [--cache-mb 128]
//! ```
//!
//! `client_bench`, task 12.5b, sends requests through the three binaries with the real client: the compio shell's
//! `Client` over TCP, logged in as the bootstrap administrator, each connection sending DirectWrites, read-write
//! transactions or read-only transactions one after another for the run's length, its commits through a session or
//! none; it reports the requests a second, the relations a second and each request's latency, with the Begin's apart.
//! Each commit writes into one partition of the cluster's even placement, so no commit spans partitions.
//!
//! ```text
//! client_bench --address <host:port> --password-file <path> --op direct|transaction|read [--connections 1]
//!              [--seconds 30] [--relations 1] [--session on|off] [--partitions 1] [--hubs 1000] [--subjects 1000]
//!              [--cluster 1] [--seed 1]
//! ```
//!
//! `alloc_bench`, task 12.5b, counts the heap allocations between a DirectWrite's frame and its record by a counting
//! global allocator: the frame viewed and its changes read, the transaction's write, and its commit building the record,
//! each frame's record then committed through the pipeline and the log.
//!
//! ```text
//! alloc_bench --dir <path> [--frames 10000] [--relations 1] [--hubs 1000]
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
pub use crate::traits::alloc_count::AllocCount;
pub use crate::traits::clock::Clock;
pub use crate::types::alloc_args::AllocArgs;
pub use crate::types::apply_args::ApplyArgs;
pub use crate::types::bound_args::BoundArgs;
pub use crate::types::client_args::ClientArgs;
pub use crate::types::client_op::ClientOp;
pub use crate::types::client_shape::{CLIENT_TENANT, ClientShape};
pub use crate::types::command::Command;
pub use crate::types::commit_shape::CommitShape;
pub use crate::types::counted_snapshot::{CountedScan, CountedSnapshot};
pub use crate::types::counting_cache::CountingCache;
pub use crate::types::dir_sizes::DirSizes;
pub use crate::types::inbound_args::InboundArgs;
pub use crate::types::owner_args::OwnerArgs;
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
pub use crate::utils::alloc_run::run_allocs;
pub use crate::utils::apply_run::run_apply;
pub use crate::utils::bound_run::run_bound;
pub use crate::utils::client_run::run_client;
pub use crate::utils::inbound_run::run_inbound;
pub use crate::utils::owner_run::run_owner;
pub use crate::utils::partitions_run::run_partitions;
pub use crate::utils::pipeline_run::run_pipeline;
pub use crate::utils::proc_stats::{parse_schedstat_ns, parse_stat_cpu_ticks, parse_status_kib};
pub use crate::utils::rollup_run::run_rollup;
pub use crate::utils::setup::wait_out_build;
pub use crate::utils::wal_bench_run::run_wal_bench;
