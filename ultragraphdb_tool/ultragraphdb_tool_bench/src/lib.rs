/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `store_bench`: the storage measurements of stage 2, task 10.2. It drives the apply thread of
//! `ultragraphdb_store_apply` (an `Applier` over `FjallStore` with `MokaListCache`) with commits shaped as the
//! Dgraph benchmark runs wrote them, at a fixed offered rate, in apply-step batches; and it measures rollups and the
//! compaction filter over one large predicate, and the store's files after each run.
//!
//! ```text
//! store_bench apply  --dir <path> --rate <commits/s> [--relations 1] [--seconds 30]
//!                    [--durability process|filesystem] [--rollups on|off] [--cache-mb 128]
//! store_bench rollup --dir <path> [--hubs 10000] [--versions 1000] [--relations 50]
//! ```
//!
//! Each run prints one JSON line. The library holds what the runs are built of, so it is tested apart from the
//! clock: the arguments, the commit shape, the rate pacer behind the [`Clock`] trait, the summaries, the readers of
//! `/proc`, the store wrapper timing each write call and the directory sizes.

mod errors;
mod traits;
mod types;
mod utils;

pub use crate::errors::bench_error::{BenchError, BenchErrorEnum};
pub use crate::traits::clock::Clock;
pub use crate::types::apply_args::ApplyArgs;
pub use crate::types::command::Command;
pub use crate::types::commit_shape::CommitShape;
pub use crate::types::dir_sizes::DirSizes;
pub use crate::types::pacer::Pacer;
pub use crate::types::rollup_args::RollupArgs;
pub use crate::types::size::Size;
pub use crate::types::summary::Summary;
pub use crate::types::timed_store::TimedStore;
pub use crate::types::wall_clock::WallClock;
pub use crate::types::write_call::WriteCall;
pub use crate::utils::apply_run::run_apply;
pub use crate::utils::proc_stats::{parse_schedstat_ns, parse_stat_cpu_ticks, parse_status_kib};
pub use crate::utils::rollup_run::run_rollup;
