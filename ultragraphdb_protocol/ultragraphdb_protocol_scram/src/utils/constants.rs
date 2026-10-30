/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The mechanism's SASL name, as a Welcome lists it.
pub const MECHANISM: &str = "SCRAM-SHA-256";

/// The least iteration count of PBKDF2 a client takes (RFC 7677 §4).
pub const MIN_ITERATIONS: u32 = 4_096;

/// The iteration count a server derives a new verifier with (the OWASP Password Storage Cheat Sheet, `SECURITY.md`).
pub const DEFAULT_ITERATIONS: u32 = 600_000;

/// The bytes of a new verifier's salt.
pub const SALT_SIZE: usize = 16;
