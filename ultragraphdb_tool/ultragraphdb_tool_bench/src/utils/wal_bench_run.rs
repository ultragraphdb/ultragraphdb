/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::bench_error::BenchError;
use crate::types::bound_args::BoundArgs;
use crate::types::inbound_args::InboundArgs;
use crate::types::owner_args::OwnerArgs;
use crate::types::partition_args::PartitionArgs;
use crate::types::pipeline_args::PipelineArgs;
use crate::utils::bound_run::run_bound;
use crate::utils::inbound_run::run_inbound;
use crate::utils::owner_run::run_owner;
use crate::utils::partitions_run::run_partitions;
use crate::utils::pipeline_run::run_pipeline;

/// The run `wal_bench`'s `args` name, the program's name left out: the frontend's owner check timed given
/// `--owner-checks`, a partition's walks of its bound transactions given `--bound-walks`, the probe of an inbound point read given `--inbound`, over partitions given `--partitions`, otherwise of one log beside fjall's journal. Its report, or why it
/// stopped.
///
/// # Errors
///
/// The outer error when the arguments name no run; the inner, the run's own.
pub fn run_wal_bench(args: &[String]) -> Result<Result<String, BenchError>, BenchError> {
    if args.iter().any(|arg| arg == "--owner-checks") {
        OwnerArgs::parse(args).map(|parsed| run_owner(&parsed))
    } else if args.iter().any(|arg| arg == "--bound-walks") {
        BoundArgs::parse(args).map(|parsed| run_bound(&parsed))
    } else if args.iter().any(|arg| arg == "--inbound") {
        InboundArgs::parse(args).map(|parsed| run_inbound(&parsed))
    } else if args.iter().any(|arg| arg == "--partitions") {
        PartitionArgs::parse(args).map(|parsed| run_partitions(&parsed))
    } else {
        PipelineArgs::parse(args).map(|parsed| run_pipeline(&parsed))
    }
}
