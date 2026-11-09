/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

/// When a source's failed authentications start to wait and when they ban it, and for how long (design D15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginLimits {
    before_throttle: u32,
    throttled_before_ban: u32,
    ban: Duration,
}

impl Default for LoginLimits {
    fn default() -> Self {
        Self::new(
            Self::BEFORE_THROTTLE_DEFAULT,
            Self::THROTTLED_BEFORE_BAN_DEFAULT,
            Self::BAN_DEFAULT,
        )
    }
}

impl LoginLimits {
    /// `auth_failures_before_throttle` when the settings name none.
    pub const BEFORE_THROTTLE_DEFAULT: u32 = 50;
    /// `auth_throttled_failures_before_ban` when the settings name none.
    pub const THROTTLED_BEFORE_BAN_DEFAULT: u32 = 20;
    /// `auth_ban_ms` when the settings name none: 24 hours.
    pub const BAN_DEFAULT: Duration = Duration::from_secs(24 * 60 * 60);

    /// A source's attempts waiting past `before_throttle` consecutive failures, and the source banned for `ban` past
    /// `throttled_before_ban` more. The settings' check refuses 0 for each.
    #[must_use]
    pub const fn new(before_throttle: u32, throttled_before_ban: u32, ban: Duration) -> Self {
        Self {
            before_throttle,
            throttled_before_ban,
            ban,
        }
    }

    /// The failures a source makes with no wait.
    #[must_use]
    pub const fn before_throttle(&self) -> u32 {
        self.before_throttle
    }

    /// The failures past those at which the source is banned.
    #[must_use]
    pub const fn throttled_before_ban(&self) -> u32 {
        self.throttled_before_ban
    }

    /// How long a ban lasts.
    #[must_use]
    pub const fn ban(&self) -> Duration {
        self.ban
    }
}
