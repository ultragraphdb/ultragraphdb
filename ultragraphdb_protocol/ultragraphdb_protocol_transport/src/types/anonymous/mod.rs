/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod anonymous_client_mechanism;
mod anonymous_login;

/// The login of a client without credentials: it speaks no mechanism, so it connects only to a server whose Welcome
/// asks for no authentication, acting for the anonymous principal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Anonymous;
