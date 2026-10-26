/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bench::{parse_schedstat_ns, parse_stat_cpu_ticks, parse_status_kib};

#[test]
fn test_the_process_cpu_is_user_and_system_ticks_after_the_command_name() {
    // The name may hold spaces and parentheses; the fields count from the last ')'. utime 1234 + stime 567.
    let stat = "4242 (store bench) (x)) R 1 4242 4242 0 -1 4194560 100 0 0 0 1234 567 0 0 20 0 9 0 100 1000 200";
    assert_eq!(parse_stat_cpu_ticks(stat), Some(1_801));
    assert_eq!(parse_stat_cpu_ticks("garbage"), None);
}

#[test]
fn test_a_stat_line_cut_short_or_not_numeric_has_no_cpu() {
    // utime present, stime missing.
    assert_eq!(
        parse_stat_cpu_ticks("1 (b) R 1 1 1 0 -1 0 0 0 0 0 1234"),
        None
    );
    // Nothing after the name.
    assert_eq!(parse_stat_cpu_ticks("1 (b)"), None);
    assert_eq!(
        parse_stat_cpu_ticks("1 (b) R 1 1 1 0 -1 0 0 0 0 0 x 567"),
        None
    );
    assert_eq!(
        parse_stat_cpu_ticks("1 (b) R 1 1 1 0 -1 0 0 0 0 0 1234 -5"),
        None
    );
}

#[test]
fn test_a_stat_line_whose_ticks_sum_past_u64_has_no_cpu() {
    // u64::MAX + 1 ticks has no u64; the parser refuses it rather than wrapping or panicking.
    let stat = "1 (b) R 1 1 1 0 -1 0 0 0 0 0 18446744073709551615 1 0";
    assert_eq!(parse_stat_cpu_ticks(stat), None);
    let at_max = "1 (b) R 1 1 1 0 -1 0 0 0 0 0 18446744073709551614 1 0";
    assert_eq!(parse_stat_cpu_ticks(at_max), Some(u64::MAX));
}

#[test]
fn test_a_threads_cpu_is_the_first_field_of_its_schedstat() {
    assert_eq!(parse_schedstat_ns("123456789 1000 42\n"), Some(123_456_789));
    assert_eq!(parse_schedstat_ns(""), None);
    assert_eq!(parse_schedstat_ns("x 1000 42"), None);
}

#[test]
fn test_the_resident_size_is_read_by_field_in_kib() {
    let status = "Name:\tstore_bench\nVmHWM:\t  204800 kB\nVmRSS:\t  102400 kB\nThreads:\t9\n";
    assert_eq!(parse_status_kib(status, "VmRSS"), Some(102_400));
    assert_eq!(parse_status_kib(status, "VmHWM"), Some(204_800));
    assert_eq!(parse_status_kib(status, "VmSwap"), None);
}

#[test]
fn test_a_status_field_is_matched_whole() {
    // A longer field sharing the name's start is another field; a field without a number has none.
    assert_eq!(
        parse_status_kib("VmRSSx:\t5 kB\nVmRSS:\t7 kB\n", "VmRSS"),
        Some(7)
    );
    assert_eq!(parse_status_kib("VmRSS:\n", "VmRSS"), None);
    assert_eq!(parse_status_kib("VmRSS:\tlots kB\n", "VmRSS"), None);
    assert_eq!(parse_status_kib("", "VmRSS"), None);
}
