/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::PostingIdentity;

#[test]
fn test_an_identity_holds_its_uid_and_value() {
    let identity = PostingIdentity::new(9, b"v".to_vec());
    assert_eq!((identity.uid(), identity.value()), (9, &b"v"[..]));
    assert_eq!(PostingIdentity::default().uid(), 0);
    assert!(PostingIdentity::default().value().is_empty());
}
