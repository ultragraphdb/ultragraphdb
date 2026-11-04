/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Checks a central file and every settings file it declares, exactly as the config service would, before anything starts:
//! `ultragraphdb_config_cli <central file>`. Exits 0 when the settings pass and 78 when they do not.
//!
//! Restores the latest revision of every target a running config service holds into a folder a service can begin from:
//! `ultragraphdb_config_cli restore <url> <admin token> <folder> [--ca-file <file>]`.

mod restore;

use mimalloc::MiMalloc;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use ultragraphdb_common_config_check::check_central;

use crate::restore::{Restore, run_restore};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

/// `sysexits`: the command was used incorrectly.
const EX_USAGE: u8 = 64;
/// `sysexits`: a configuration error.
const EX_CONFIG: u8 = 78;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout().lock();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let restore = match words.as_slice() {
        ["restore", url, token, folder] => Some((*url, *token, *folder, None)),
        ["restore", url, token, folder, "--ca-file", file] => {
            Some((*url, *token, *folder, Some(*file)))
        }
        _ => None,
    };
    if let Some((url, token, folder, ca_file)) = restore {
        let outcome = run_restore(Restore {
            url,
            token,
            folder: Path::new(folder),
            ca_file,
        });
        let (lines, code) = match outcome {
            Ok(lines) => (lines, ExitCode::SUCCESS),
            Err(failure) => (
                vec![format!("error: {}", failure.message)],
                ExitCode::from(failure.code),
            ),
        };
        for line in lines {
            // A closed stdout leaves the exit code to say it.
            drop(writeln!(out, "{line}"));
        }
        return code;
    }
    let [central] = args.as_slice() else {
        // A closed stdout leaves the exit code to say it.
        drop(writeln!(
            out,
            "usage: ultragraphdb_config_cli <central file>\n       ultragraphdb_config_cli restore <url> <admin token> <folder> [--ca-file <file>]"
        ));
        return ExitCode::from(EX_USAGE);
    };
    let report = check_central(Path::new(central));
    let mut lines: Vec<String> = report
        .warnings()
        .iter()
        .map(|warning| format!("warning: {warning}"))
        .collect();
    lines.extend(
        report
            .defects()
            .iter()
            .map(|defect| format!("error: {defect}")),
    );
    if report.passes() {
        for (target, instances) in report.targets() {
            lines.push(format!("target {target}: {}", instances.join(", ")));
        }
        lines.push(format!(
            "{central} passes. Stored revisions were not compared: the service does that."
        ));
    } else {
        lines.push(format!(
            "{central} fails with {} defects.",
            report.defects().len()
        ));
    }
    for line in lines {
        // A closed stdout leaves the exit code to say it.
        drop(writeln!(out, "{line}"));
    }
    if report.passes() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EX_CONFIG)
    }
}
