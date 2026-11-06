/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a request acts on, as the gate reads it: a tenant it knows, or a scope it does not, which it denies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateScope {
    /// A tenant.
    Tenant(u64),
    /// The cluster, reached by administration and no request the gate takes.
    Cluster,
}
