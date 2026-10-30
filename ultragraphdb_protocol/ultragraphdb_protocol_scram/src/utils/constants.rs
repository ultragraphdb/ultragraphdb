/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The mechanism's SASL name, as a Welcome lists it.
pub const MECHANISM: &str = "SCRAM-SHA-256";

/// The least iteration count of PBKDF2 a client takes (RFC 7677 §4).
pub const MIN_ITERATIONS: u32 = 4_096;

/// The most iterations of PBKDF2 a client runs, and a verifier is derived with: RFC 5802 sets no ceiling, and a server
/// naming `u32::MAX` would hold a client for hours. 16 times [`DEFAULT_ITERATIONS`], room for the OWASP figure to rise
/// (`SECURITY.md`).
pub const MAX_ITERATIONS: u32 = 10_000_000;

/// The iteration count a server derives a new verifier with (the OWASP Password Storage Cheat Sheet, `SECURITY.md`).
pub const DEFAULT_ITERATIONS: u32 = 600_000;

/// The bytes of a new verifier's salt.
pub const SALT_SIZE: usize = 16;
