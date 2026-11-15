/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_common_service::CpuBinding;

/// A binding reads as pinned only when the thread holds exactly the CPU asked for, since a refused one looks the same
/// from the call that made it.
#[test]
fn test_a_binding_says_what_the_thread_holds() {
    let read_back = |asked, held: &[usize]| CpuBinding::ReadBack {
        asked,
        held: held.iter().copied().collect::<BTreeSet<usize>>(),
    };
    assert_eq!(read_back(3, &[3]).to_string(), "pinned to CPU 3");
    assert!(read_back(3, &[3, 4]).to_string().starts_with("unpinned"));
    assert!(read_back(3, &[4]).to_string().starts_with("unpinned"));
    assert!(CpuBinding::NotAsked.to_string().starts_with("unpinned"));
    assert!(CpuBinding::Unavailable.to_string().starts_with("unpinned"));
}

#[test]
fn test_a_role_binding_says_whether_the_thread_holds_the_roles_cpus() {
    let cpus = |ids: &[usize]| {
        ids.iter()
            .copied()
            .collect::<std::collections::BTreeSet<usize>>()
    };
    let held = CpuBinding::Role {
        asked: cpus(&[1, 2, 3]),
        held: cpus(&[1, 2, 3]),
    };
    assert!(held.to_string().starts_with("bound to the role's CPUs"));
    let refused = CpuBinding::Role {
        asked: cpus(&[1, 2, 3]),
        held: cpus(&[0, 1, 2, 3]),
    };
    assert!(refused.to_string().starts_with("unpinned"));
}
