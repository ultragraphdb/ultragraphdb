/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::path::PathBuf;

use ultragraphdb_store_kv::Durability;

use crate::errors::bench_error::BenchError;
use crate::types::apply_args::ApplyArgs;
use crate::types::rollup_args::RollupArgs;

const USAGE: &str = "store_bench apply|rollup --dir <path> [options]";
const APPLY_FLAGS: [&str; 8] = [
    "--dir",
    "--rate",
    "--relations",
    "--seconds",
    "--durability",
    "--rollups",
    "--cache-mb",
    "--max-batch",
];
const ROLLUP_FLAGS: [&str; 4] = ["--dir", "--hubs", "--versions", "--relations"];

/// The run the arguments name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Apply(ApplyArgs),
    Rollup(RollupArgs),
}

impl Command {
    /// The run of `args`, the program's name left out.
    ///
    /// # Errors
    ///
    /// [`BenchError::Usage`] naming what cannot run.
    pub fn parse(args: &[String]) -> Result<Self, BenchError> {
        let Some((run, rest)) = args.split_first() else {
            return Err(BenchError::Usage(USAGE.to_string()));
        };
        match run.as_str() {
            "apply" => {
                let flags = flags(rest, &APPLY_FLAGS)?;
                Ok(Command::Apply(ApplyArgs::new(
                    dir(&flags)?,
                    number(&flags, "--rate", None, 0)?,
                    usize::try_from(number(&flags, "--relations", Some(1), 1)?)
                        .unwrap_or(usize::MAX),
                    number(&flags, "--seconds", Some(30), 1)?,
                    durability(&flags)?,
                    rollups(&flags)?,
                    number(&flags, "--cache-mb", Some(128), 0)?,
                    match flags.contains_key("--max-batch") {
                        true => Some(
                            usize::try_from(number(&flags, "--max-batch", None, 1)?)
                                .unwrap_or(usize::MAX),
                        ),
                        false => None,
                    },
                )))
            }
            "rollup" => {
                let flags = flags(rest, &ROLLUP_FLAGS)?;
                Ok(Command::Rollup(RollupArgs::new(
                    dir(&flags)?,
                    number(&flags, "--hubs", Some(10_000), 1)?,
                    number(&flags, "--versions", Some(1_000), 1)?,
                    usize::try_from(number(&flags, "--relations", Some(50), 1)?)
                        .unwrap_or(usize::MAX),
                )))
            }
            other => Err(BenchError::Usage(format!("unknown run {other}"))),
        }
    }
}

/// The flags of `rest`, each with its value, refused unless `known` lists it.
fn flags<'a>(rest: &'a [String], known: &[&str]) -> Result<BTreeMap<&'a str, &'a str>, BenchError> {
    let mut flags = BTreeMap::new();
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        if !known.contains(&flag.as_str()) {
            return Err(BenchError::Usage(format!("unknown flag {flag}")));
        }
        let Some(value) = it.next() else {
            return Err(BenchError::Usage(format!("{flag} takes a value")));
        };
        flags.insert(flag.as_str(), value.as_str());
    }
    Ok(flags)
}

fn dir(flags: &BTreeMap<&str, &str>) -> Result<PathBuf, BenchError> {
    flags
        .get("--dir")
        .map(PathBuf::from)
        .ok_or_else(|| BenchError::Usage("--dir is required".to_string()))
}

/// The number `flag` gives, or `default`; required when there is none, and at least `min`.
fn number(
    flags: &BTreeMap<&str, &str>,
    flag: &str,
    default: Option<u64>,
    min: u64,
) -> Result<u64, BenchError> {
    let n = match (flags.get(flag), default) {
        (Some(v), _) => v
            .parse::<u64>()
            .map_err(|_| BenchError::Usage(format!("{flag} takes a number, not {v}")))?,
        (None, Some(d)) => d,
        (None, None) => return Err(BenchError::Usage(format!("{flag} is required"))),
    };
    if n < min {
        return Err(BenchError::Usage(format!("{flag} must be at least {min}")));
    }
    Ok(n)
}

fn durability(flags: &BTreeMap<&str, &str>) -> Result<Durability, BenchError> {
    match flags.get("--durability").copied().unwrap_or("process") {
        "process" => Ok(Durability::Process),
        "filesystem" => Ok(Durability::Filesystem),
        other => Err(BenchError::Usage(format!(
            "--durability is process or filesystem, not {other}"
        ))),
    }
}

fn rollups(flags: &BTreeMap<&str, &str>) -> Result<bool, BenchError> {
    match flags.get("--rollups").copied().unwrap_or("on") {
        "on" => Ok(true),
        "off" => Ok(false),
        other => Err(BenchError::Usage(format!(
            "--rollups is on or off, not {other}"
        ))),
    }
}
