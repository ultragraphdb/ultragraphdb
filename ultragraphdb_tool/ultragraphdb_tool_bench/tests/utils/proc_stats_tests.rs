/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bench::{parse_schedstat_ns, parse_stat_cpu_ticks, parse_status_kib};

#[test]
fn test_the_process_cpu_is_user_and_system_ticks_after_the_command_name() {
    // The name may hold spaces and parentheses; the fields count from the last ')'.
    let stat = "4242 (store bench) (x)) R 1 4242 4242 0 -1 4194560 100 0 0 0 1234 567 0 0 20 0 9 0 100 1000 200";
    assert_eq!(parse_stat_cpu_ticks(stat), Some(1_801));
    assert_eq!(parse_stat_cpu_ticks("garbage"), None);
}

#[test]
fn test_a_threads_cpu_is_the_first_field_of_its_schedstat() {
    assert_eq!(parse_schedstat_ns("123456789 1000 42\n"), Some(123_456_789));
    assert_eq!(parse_schedstat_ns(""), None);
}

#[test]
fn test_the_resident_size_is_read_by_field_in_kib() {
    let status = "Name:\tstore_bench\nVmHWM:\t  204800 kB\nVmRSS:\t  102400 kB\nThreads:\t9\n";
    assert_eq!(parse_status_kib(status, "VmRSS"), Some(102_400));
    assert_eq!(parse_status_kib(status, "VmHWM"), Some(204_800));
    assert_eq!(parse_status_kib(status, "VmSwap"), None);
}
