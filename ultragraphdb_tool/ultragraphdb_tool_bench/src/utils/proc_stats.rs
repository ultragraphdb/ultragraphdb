/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What Linux's `/proc` reports of the process: its CPU time, a thread's CPU time and the resident set. Off Linux
//! the readers return `None`.

/// Linux's `USER_HZ`, the unit of `/proc/<pid>/stat`'s CPU times on every architecture it supports.
const TICKS_PER_SECOND: f64 = 100.0;

/// `utime` plus `stime` from a `/proc/<pid>/stat` line, in ticks; the fields count from the last `)`, since the
/// command name may hold spaces and parentheses.
pub fn parse_stat_cpu_ticks(stat: &str) -> Option<u64> {
    let rest = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After the name: state (field 3) first, so utime (field 14) and stime (field 15) are the 12th and 13th.
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

/// The time on CPU, in nanoseconds, from a `schedstat` line: its first field.
pub fn parse_schedstat_ns(schedstat: &str) -> Option<u64> {
    schedstat.split_whitespace().next()?.parse().ok()
}

/// The value of `field` in a `/proc/<pid>/status` text, in KiB.
pub fn parse_status_kib(status: &str, field: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|l| l.strip_prefix(field)?.strip_prefix(':'))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// The process's CPU time so far, in seconds.
pub fn process_cpu_secs() -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    parse_stat_cpu_ticks(&stat).map(|t| t as f64 / TICKS_PER_SECOND)
}

/// The calling thread's CPU time so far, in seconds.
pub fn thread_cpu_secs() -> Option<f64> {
    let s = std::fs::read_to_string("/proc/thread-self/schedstat").ok()?;
    parse_schedstat_ns(&s).map(|ns| ns as f64 / 1e9)
}

/// The process's resident set now and at its peak, in KiB.
pub fn rss_kib() -> (Option<u64>, Option<u64>) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    (
        parse_status_kib(&status, "VmRSS"),
        parse_status_kib(&status, "VmHWM"),
    )
}
