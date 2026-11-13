/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashSet;

use ultragraphdb_error_options::{OptionsError, OptionsErrorEnum};

#[test]
fn test_constructors_build_their_variant() {
    let m = || "m".to_string();
    assert_eq!(
        OptionsError::not_allowed(m()).0,
        OptionsErrorEnum::NotAllowed(m())
    );
    assert_eq!(OptionsError::parse(m()).0, OptionsErrorEnum::Parse(m()));
    assert_eq!(OptionsError::invalid(m()).0, OptionsErrorEnum::Invalid(m()));
    assert_eq!(
        OptionsError::type_mismatch(m()).0,
        OptionsErrorEnum::Type(m())
    );
    // `invalid` takes a `&str` as well as a `String`.
    assert_eq!(OptionsError::invalid("m").0, OptionsErrorEnum::Invalid(m()));
}

#[test]
fn test_message_is_the_variants_text() {
    for e in [
        OptionsError::not_allowed("option x is not allowed!".into()),
        OptionsError::parse("invalid digit found in string".into()),
        OptionsError::invalid("Can't create a vector index for l1"),
        OptionsError::type_mismatch("unable to assign a value of type int to type string".into()),
    ] {
        assert_eq!(e.message(), e.to_string());
    }
}

#[test]
fn test_derives_compare_clone_and_hash() {
    let e = OptionsError::parse("p".into());
    assert_eq!(e.clone(), e);
    assert_ne!(e, OptionsError::invalid("p"));
    let set: HashSet<OptionsError> = [e.clone(), e].into_iter().collect();
    assert_eq!(set.len(), 1);
    assert!(format!("{:?}", OptionsError::type_mismatch("t".into())).contains("Type"));
}
