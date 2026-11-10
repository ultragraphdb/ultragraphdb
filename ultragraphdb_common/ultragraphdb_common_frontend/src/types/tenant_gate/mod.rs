/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Action, ErrorCode, ErrorReply};

use crate::traits::authorizer::Authorizer;
use crate::types::gate_scope::GateScope;

/// The one check on every request naming a tenant, before it reaches another role or any state (`PROTOCOL.md` §11.10): whether
/// the connection's principal may do the request's action in that tenant. It fails closed: an action or a scope it does not
/// know is denied, and a refusal is the same for a tenant that exists and one that does not.
#[derive(Debug, Clone, Copy, Default)]
pub struct TenantGate<A> {
    authorizer: A,
}

impl<A: Authorizer> TenantGate<A> {
    /// The gate asking `authorizer`.
    pub fn new(authorizer: A) -> Self {
        Self { authorizer }
    }

    /// Whether `principal` may do the action numbered `action` in `scope`.
    ///
    /// # Errors
    ///
    /// Code 21, `detail` the action, the message saying nothing of the tenant.
    pub fn check(&self, principal: u64, action: u64, scope: GateScope) -> Result<(), ErrorReply> {
        let allowed = match (Action::from_number(action), scope) {
            (
                Some(
                    known @ (Action::Read
                    | Action::Write
                    | Action::Declare
                    | Action::Drop
                    | Action::Purge),
                ),
                GateScope::Tenant(tenant),
            ) => self.authorizer.allows(principal, known, tenant),
            (Some(Action::Admin | Action::Operate) | None, GateScope::Tenant(_))
            | (Some(_) | None, GateScope::Cluster) => false,
        };
        if allowed {
            return Ok(());
        }
        Err(ErrorReply::new(
            ErrorCode::PermissionDenied,
            format!("the principal may not do action {action} here"),
        )
        .with_detail(action))
    }
}
