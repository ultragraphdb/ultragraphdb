/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_token_names_its_commit() -> TestResult {
    // transactions, "A token names its commit": partition 1, position 40, epoch 12.
    let token = SessionToken::new(1, 40, Epoch::new(12)?);
    assert_eq!(token.partition(), 1);
    assert_eq!(token.position(), 40);
    assert_eq!(token.epoch(), Epoch::new(12)?);
    Ok(())
}

#[test]
fn test_a_token_keeps_its_largest_fields() -> TestResult {
    let token = SessionToken::new(u32::MAX, u64::MAX, Epoch::MAX);
    assert_eq!(
        (token.partition(), token.position(), token.epoch()),
        (u32::MAX, u64::MAX, Epoch::MAX)
    );
    Ok(())
}

#[test]
fn test_tokens_differing_in_one_field_differ() -> TestResult {
    let token = SessionToken::new(1, 40, Epoch::new(12)?);
    assert_eq!(token.clone(), SessionToken::new(1, 40, Epoch::new(12)?));
    assert_ne!(token, SessionToken::new(2, 40, Epoch::new(12)?));
    assert_ne!(token, SessionToken::new(1, 41, Epoch::new(12)?));
    assert_ne!(token, SessionToken::new(1, 40, Epoch::new(13)?));
    Ok(())
}
