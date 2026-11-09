/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};

use std::collections::BTreeMap;

use ultragraphdb_protocol_wire::ReadLevel;

use crate::errors::bench_error::BenchError;
use crate::types::client_op::ClientOp;
use crate::types::command::{flags, number};

const USAGE: &str = concat!(
    "client_bench --address <host:port> --password-file <path> ",
    "--op direct|transaction|read|scan|span|stall|list-read|hot ",
    "[--connections 1] [--seconds 30] [--relations 1] [--session on|off] [--partitions 1] [--hubs 1000] ",
    "[--subjects 1000] [--cluster 1] [--seed 1] [--level snapshot|monotonic] [--limit 0] [--predicates 8] ",
    "[--fresh on|off] [--writes 16] [--value-bytes 1]"
);
const FLAGS: [&str; 18] = [
    "--address",
    "--password-file",
    "--op",
    "--connections",
    "--seconds",
    "--relations",
    "--session",
    "--partitions",
    "--hubs",
    "--subjects",
    "--cluster",
    "--seed",
    "--level",
    "--limit",
    "--predicates",
    "--fresh",
    "--writes",
    "--value-bytes",
];
/// The most connections a run opens: each is a task on the run's one runtime.
const CONNECTIONS_MAX: u64 = 1_024;
/// The most relations a commit writes: two changes each, well inside a frame's 1 MiB.
const RELATIONS_MAX: u64 = 1_000;
/// The most partitions the cluster's even placement splits a tenant over (`PlacementMap::PARTITIONS_MAX`).
const PARTITIONS_MAX: u64 = 1_024;
/// The most predicates the hot subject holds.
const PREDICATES_MAX: u64 = 1_024;
/// The most Writes a stalling transaction pipelines: a stream's requests, `max_streams` less its Begin and Abort.
const WRITES_MAX: u64 = 254;
/// The largest string a relation writes: a thousand of them still fit a frame's 1 MiB only when few.
const VALUE_BYTES_MAX: u64 = 65_536;

/// A run of `client_bench`: the client endpoint it dials over TCP, the administrator's password file, what each request
/// is, how many connections send requests one after another and for how long, the relations each commit writes, whether
/// commits go through a session, the partitions the cluster's even placement holds, the hubs the edges point at, the
/// subjects each connection of a read run writes before its reads, the cluster's ID and the seed of the draws; for the
/// reads, their level; for the hot subject's readers and writers, the rows a reader reads, the predicates the subject holds
/// and whether a write names a new one; for a stalling run, the Writes each transaction pipelines; and the bytes of the
/// string each relation writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientArgs {
    address: SocketAddr,
    password_file: PathBuf,
    op: ClientOp,
    connections: u64,
    seconds: u64,
    relations: usize,
    session: bool,
    partitions: u32,
    hubs: u64,
    subjects: u64,
    cluster: u64,
    seed: u64,
    level: ReadLevel,
    limit: u32,
    predicates: u64,
    fresh: bool,
    writes: u64,
    value_bytes: usize,
}

/// `address`, a host's IP and a port or a host's name and a port, as the run dials it: a name, a Kubernetes Service's in a bench
/// Job, resolved once, at start, to its first address.
fn resolved(address: &str) -> Result<SocketAddr, BenchError> {
    if let Ok(socket) = address.parse() {
        return Ok(socket);
    }
    let usage = || BenchError::usage(format!("--address takes a host and a port, not {address}"));
    address
        .to_socket_addrs()
        .map_err(|_| usage())?
        .next()
        .ok_or_else(usage)
}

impl ClientArgs {
    /// The run of `args`, the program's name left out: `--address`, `--password-file` and `--op` required.
    ///
    /// # Errors
    ///
    /// [`BenchError::usage`] naming what cannot run.
    pub fn parse(args: &[String]) -> Result<Self, BenchError> {
        if args.is_empty() {
            return Err(BenchError::usage(USAGE.to_string()));
        }
        let flags = flags(args, &FLAGS)?;
        let required = |flag: &str| {
            flags
                .get(flag)
                .copied()
                .ok_or_else(|| BenchError::usage(format!("{flag} is required")))
        };
        let address = required("--address")?;
        let op = required("--op")?;
        let op = ClientOp::of_name(op).ok_or_else(|| {
            BenchError::usage(format!(
                "--op is direct, transaction, read, scan, span, stall, list-read or hot, not {op}"
            ))
        })?;
        let partitions = bounded(&flags, "--partitions", (1, 1), PARTITIONS_MAX)?;
        let relations = bounded(&flags, "--relations", (1, 1), RELATIONS_MAX)?;
        Ok(Self {
            address: resolved(address)?,
            password_file: PathBuf::from(required("--password-file")?),
            op,
            connections: bounded(&flags, "--connections", (1, 1), CONNECTIONS_MAX)?,
            seconds: number(&flags, "--seconds", Some(30), 1)?,
            // At most `RELATIONS_MAX` and `PARTITIONS_MAX`, so each cast is lossless.
            relations: relations as usize,
            session: on_off(&flags, "--session", true)?,
            partitions: partitions as u32,
            hubs: number(&flags, "--hubs", Some(1_000), partitions)?,
            subjects: number(&flags, "--subjects", Some(1_000), 1)?,
            cluster: number(&flags, "--cluster", Some(1), 1)?,
            seed: number(&flags, "--seed", Some(1), 0)?,
            level: level(&flags)?,
            // At most `u32::MAX`, so the cast is lossless.
            limit: bounded(&flags, "--limit", (0, 0), u64::from(u32::MAX))? as u32,
            predicates: bounded(&flags, "--predicates", (8, 1), PREDICATES_MAX)?,
            fresh: on_off(&flags, "--fresh", false)?,
            writes: bounded(&flags, "--writes", (16, 1), WRITES_MAX)?,
            // At most `VALUE_BYTES_MAX`, so the cast is lossless.
            value_bytes: bounded(&flags, "--value-bytes", (1, 1), VALUE_BYTES_MAX)? as usize,
        })
    }

    /// The client endpoint the run dials over TCP.
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// The file holding the bootstrap administrator's password, which the run logs in with.
    #[must_use]
    pub fn password_file(&self) -> &Path {
        &self.password_file
    }

    /// What each request is.
    #[must_use]
    pub fn op(&self) -> ClientOp {
        self.op
    }

    /// How many connections send requests, each one at a time.
    #[must_use]
    pub fn connections(&self) -> u64 {
        self.connections
    }

    /// How long the timed part of the run lasts, in seconds.
    #[must_use]
    pub fn seconds(&self) -> u64 {
        self.seconds
    }

    /// The relations each commit writes.
    #[must_use]
    pub fn relations(&self) -> usize {
        self.relations
    }

    /// Whether each connection registers a session and commits through it, so the frontend's owner check runs per commit.
    #[must_use]
    pub fn session(&self) -> bool {
        self.session
    }

    /// The partitions the cluster's even placement holds.
    #[must_use]
    pub fn partitions(&self) -> u32 {
        self.partitions
    }

    /// The hubs the edges point at, at least one per partition.
    #[must_use]
    pub fn hubs(&self) -> u64 {
        self.hubs
    }

    /// The subjects each connection of a read run writes before its reads.
    #[must_use]
    pub fn subjects(&self) -> u64 {
        self.subjects
    }

    /// The cluster's ID, which every frame names.
    #[must_use]
    pub fn cluster(&self) -> u64 {
        self.cluster
    }

    /// The seed of the run's draws.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The level of a read-only transaction's reads: snapshot, the default, or monotonic-fast.
    #[must_use]
    pub fn level(&self) -> ReadLevel {
        self.level
    }

    /// The most rows a hot subject's reader reads; 0 for every row.
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// The predicates the hot subject holds, each written once before the timed part.
    #[must_use]
    pub fn predicates(&self) -> u64 {
        self.predicates
    }

    /// Whether a hot write names a predicate the subject does not hold yet.
    #[must_use]
    pub fn fresh(&self) -> bool {
        self.fresh
    }

    /// The Writes each stalling transaction pipelines.
    #[must_use]
    pub fn writes(&self) -> u64 {
        self.writes
    }

    /// The bytes of the string each relation writes.
    #[must_use]
    pub fn value_bytes(&self) -> usize {
        self.value_bytes
    }
}

/// `flag`, on or off, `default` when not given.
fn on_off(flags: &BTreeMap<&str, &str>, flag: &str, default: bool) -> Result<bool, BenchError> {
    match flags.get(flag).copied() {
        None => Ok(default),
        Some("on") => Ok(true),
        Some("off") => Ok(false),
        Some(other) => Err(BenchError::usage(format!(
            "{flag} is on or off, not {other}"
        ))),
    }
}

/// `--level`: snapshot by default, or monotonic for monotonic-fast.
fn level(flags: &BTreeMap<&str, &str>) -> Result<ReadLevel, BenchError> {
    match flags.get("--level").copied().unwrap_or("snapshot") {
        "snapshot" => Ok(ReadLevel::Snapshot),
        "monotonic" => Ok(ReadLevel::MonotonicFast),
        other => Err(BenchError::usage(format!(
            "--level is snapshot or monotonic, not {other}"
        ))),
    }
}

/// The number `flag` gives, `default` when not given, refused below `min` and past `max`.
fn bounded(
    flags: &BTreeMap<&str, &str>,
    flag: &str,
    (default, min): (u64, u64),
    max: u64,
) -> Result<u64, BenchError> {
    at_most(number(flags, flag, Some(default), min)?, flag, max)
}

/// `n`, refused past `max`.
fn at_most(n: u64, flag: &str, max: u64) -> Result<u64, BenchError> {
    if n > max {
        return Err(BenchError::usage(format!("{flag} takes at most {max}")));
    }
    Ok(n)
}
