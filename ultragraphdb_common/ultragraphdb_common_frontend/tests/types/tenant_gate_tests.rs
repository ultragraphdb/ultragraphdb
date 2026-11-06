/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_frontend::{Authorizer, EveryAuthenticated, GateScope, TenantGate};
use ultragraphdb_protocol_wire::{Action, ErrorCode};
use ultragraphdb_tool_bdd::{Context, TestResult};

#[derive(Clone, Copy)]
struct OnlyTenantOne;

impl Authorizer for OnlyTenantOne {
    fn allows(&self, _principal: u64, _action: Action, tenant: u64) -> bool {
        tenant == 1
    }
}

#[test]
fn test_every_action_a_tenant_takes_passes_this_stage() {
    let gate = TenantGate::new(EveryAuthenticated);
    for action in [Action::Read, Action::Write, Action::Declare, Action::Drop] {
        assert!(gate.check(7, action.number(), GateScope::Tenant(0)).is_ok());
    }
}

#[test]
fn test_what_the_gate_does_not_know_is_denied_naming_the_action() -> TestResult {
    let gate = TenantGate::new(EveryAuthenticated);
    for action in [Action::Admin.number(), Action::Operate.number(), 0, 99] {
        let denied = gate
            .check(7, action, GateScope::Tenant(0))
            .err()
            .context("a refusal")?;
        assert_eq!(
            (denied.code(), denied.detail()),
            (ErrorCode::PermissionDenied, action)
        );
    }
    let denied = gate
        .check(7, Action::Read.number(), GateScope::Cluster)
        .err()
        .context("a refusal")?;
    assert_eq!(denied.code(), ErrorCode::PermissionDenied);
    Ok(())
}

#[test]
fn test_a_refusal_says_nothing_of_the_tenant() -> TestResult {
    let gate = TenantGate::new(OnlyTenantOne);
    assert!(
        gate.check(7, Action::Read.number(), GateScope::Tenant(1))
            .is_ok()
    );
    let held = gate
        .check(7, Action::Read.number(), GateScope::Tenant(2))
        .err()
        .context("a refusal")?;
    let other = gate
        .check(7, Action::Read.number(), GateScope::Tenant(9))
        .err()
        .context("a refusal")?;
    assert_eq!(held, other);
    Ok(())
}
