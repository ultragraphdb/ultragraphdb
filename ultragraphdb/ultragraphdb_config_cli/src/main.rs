/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Checks a central file and every settings file it declares, exactly as the config service would, before anything starts:
//! `ultragraphdb_config_cli <central file>`. Exits 0 when the settings pass and 78 when they do not.

use mimalloc::MiMalloc;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use ultragraphdb_common_config_check::check_central;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

/// `sysexits`: the command was used incorrectly.
const EX_USAGE: u8 = 64;
/// `sysexits`: a configuration error.
const EX_CONFIG: u8 = 78;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout().lock();
    let [central] = args.as_slice() else {
        // A closed stdout leaves the exit code to say it.
        drop(writeln!(
            out,
            "usage: ultragraphdb_config_cli <central file>"
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
