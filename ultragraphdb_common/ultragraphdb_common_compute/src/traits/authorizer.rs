/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Action;

/// Whether a principal holds an action in a tenant. Stage 7's grants implement it; the gate alone asks it, and only of what it
/// knows, so an implementation answers for a known action in a tenant and nothing else.
pub trait Authorizer {
    /// Whether `principal` may do `action` in `tenant`.
    fn allows(&self, principal: u64, action: Action, tenant: u64) -> bool;
}
