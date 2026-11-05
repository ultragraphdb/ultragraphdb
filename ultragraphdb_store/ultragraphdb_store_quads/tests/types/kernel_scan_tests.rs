/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroUsize;

use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_store_posting::PostingIdentity;
use ultragraphdb_store_quads::KernelScan;
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_a_scan_holds_its_request_and_ends_at_the_namespace_unless_bounded() -> TestResult {
    let limit = NonZeroUsize::new(5).context("5 is no zero")?;
    let scan = KernelScan::new(Keyspace::Reverse, 3, vec![0x11, 9], 40, limit);
    assert_eq!(scan.keyspace(), Keyspace::Reverse);
    assert_eq!(scan.namespace(), 3);
    assert_eq!(scan.from(), &[0x11, 9]);
    assert_eq!(scan.end(), None);
    assert_eq!(scan.version(), 40);
    assert_eq!(scan.limit(), limit);
    let bounded = scan.clone().to(vec![0x11, 10]);
    assert_eq!(bounded.end(), Some(&[0x11u8, 10][..]));
    assert_ne!(bounded, scan);
    Ok(())
}

#[test]
fn test_a_scan_resumes_after_the_posting_it_names() -> TestResult {
    let limit = NonZeroUsize::new(5).context("5 is no zero")?;
    let scan = KernelScan::new(Keyspace::Forward, 3, vec![0x10, 9], 40, limit);
    assert_eq!(scan.after(), None, "a scan begins at its first posting");
    let after = PostingIdentity::new(7, b"v".to_vec());
    assert_eq!(scan.resuming(Some(after.clone())).after(), Some(&after));
    Ok(())
}
