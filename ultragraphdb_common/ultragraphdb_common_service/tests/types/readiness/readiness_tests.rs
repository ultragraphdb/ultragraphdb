/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::Readiness;
use ultragraphdb_tool_bdd::TestResult;

/// A role starts not ready, and a clone sees what the original sets, as the responder sees the role.
#[test]
fn test_readiness_is_shared_and_starts_false() {
    let role = Readiness::default();
    let responder = role.clone();
    assert!(!responder.is_ready());
    role.set(true);
    assert!(responder.is_ready());
    role.set(false);
    assert!(!responder.is_ready());
}

/// A binding role listens only once it says so, and a clone sees it, as the responder sees the role.
#[test]
fn test_a_binding_readiness_listens_once_set() {
    let role = Readiness::binding();
    let responder = role.clone();
    assert!(!responder.is_listening());
    assert!(!responder.is_ready());
    role.set_listening();
    assert!(responder.is_listening());
    assert!(!responder.is_ready());
}

/// Three cores' readiness, each its own, as each compute core's link to the system group holds one (design D14).
fn three_cores() -> [Readiness; 3] {
    [
        Readiness::default(),
        Readiness::default(),
        Readiness::default(),
    ]
}

/// A role following its cores is not ready while one core is not, whichever it is, and ready once all three are.
#[test]
fn test_a_role_is_ready_only_once_every_core_is() -> TestResult {
    let cores = three_cores();
    let role = Readiness::default();
    for unready in 0..cores.len() {
        cores
            .iter()
            .enumerate()
            .for_each(|(at, core)| core.set(at != unready));
        role.follow(&cores);
        assert!(!role.is_ready(), "core {unready} is not ready");
    }
    cores.iter().for_each(|core| core.set(true));
    role.follow(&cores);
    assert!(role.is_ready());
    Ok(())
}

/// A role that was ready is not once a core withdraws its readiness, and is again once the core holds it again.
#[test]
fn test_a_core_withdrawn_withdraws_the_role() -> TestResult {
    let cores = three_cores();
    let role = Readiness::default();
    cores.iter().for_each(|core| core.set(true));
    role.follow(&cores);
    assert!(role.is_ready());
    let withdrawn = cores.get(1);
    withdrawn.iter().for_each(|core| core.set(false));
    role.follow(&cores);
    assert!(!role.is_ready());
    withdrawn.iter().for_each(|core| core.set(true));
    role.follow(&cores);
    assert!(role.is_ready());
    Ok(())
}

/// A role following no core is not ready: nothing it serves on has a link that holds.
#[test]
fn test_a_role_of_no_core_is_not_ready() -> TestResult {
    let role = Readiness::default();
    role.set(true);
    role.follow(&[]);
    assert!(!role.is_ready());
    Ok(())
}
