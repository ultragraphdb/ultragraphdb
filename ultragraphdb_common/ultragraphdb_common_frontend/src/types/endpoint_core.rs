/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::{Rc, Weak};

use crate::traits::authorizer::Authorizer;
use crate::types::auth_throttle::AuthThrottle;
use crate::types::endpoint_limits::EndpointLimits;
use crate::types::owner_handle::OwnerHandle;
use crate::types::role_links::RoleLinks;
use crate::types::tenant_gate::TenantGate;

/// What a client session shares on its endpoint core: the way to the owner, the connections to the other roles, the gate, the
/// limits and the throttle. The way to the owner is shared by an `Rc` on the core, so what outlives the core's sessions can hold it
/// weakly and keep no owner from stopping.
#[derive(Debug, Clone)]
pub struct EndpointCore<A> {
    owner: Rc<OwnerHandle>,
    links: RoleLinks,
    gate: TenantGate<A>,
    limits: EndpointLimits,
    throttle: AuthThrottle,
}

impl<A: Authorizer> EndpointCore<A> {
    /// The core asking `owner`, reaching the other roles through `links`, each request passing `gate`.
    #[must_use]
    pub fn new(
        owner: OwnerHandle,
        links: RoleLinks,
        gate: TenantGate<A>,
        limits: EndpointLimits,
    ) -> Self {
        Self {
            owner: Rc::new(owner),
            links,
            gate,
            limits,
            throttle: AuthThrottle::new(limits.refusal_interval()),
        }
    }
}

impl<A> EndpointCore<A> {
    /// The way to the owner.
    #[must_use]
    pub fn owner(&self) -> &OwnerHandle {
        self.owner.as_ref()
    }

    /// The way to the owner while the core lasts, for what outlives its sessions.
    pub(crate) fn owner_weak(&self) -> Weak<OwnerHandle> {
        Rc::downgrade(&self.owner)
    }

    /// The connections to the other roles.
    #[must_use]
    pub fn links(&self) -> &RoleLinks {
        &self.links
    }

    /// The gate.
    #[must_use]
    pub fn gate(&self) -> &TenantGate<A> {
        &self.gate
    }

    /// The limits.
    #[must_use]
    pub fn limits(&self) -> &EndpointLimits {
        &self.limits
    }

    /// The throttle of failed authentications.
    #[must_use]
    pub fn throttle(&self) -> &AuthThrottle {
        &self.throttle
    }
}
