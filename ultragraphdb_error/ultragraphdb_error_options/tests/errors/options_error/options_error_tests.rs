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
        OptionsError::NotAllowed(m()).0,
        OptionsErrorEnum::NotAllowed(m())
    );
    assert_eq!(OptionsError::Parse(m()).0, OptionsErrorEnum::Parse(m()));
    assert_eq!(OptionsError::Invalid(m()).0, OptionsErrorEnum::Invalid(m()));
    assert_eq!(OptionsError::Type(m()).0, OptionsErrorEnum::Type(m()));
    assert_eq!(OptionsError::invalid("m"), OptionsError::Invalid(m()));
}

#[test]
fn test_message_is_the_variants_text() {
    for e in [
        OptionsError::NotAllowed("option x is not allowed!".into()),
        OptionsError::Parse("strconv.Atoi: parsing \"x\": invalid syntax".into()),
        OptionsError::Invalid("Can't create a vector index for l1".into()),
        OptionsError::Type("unable to assign a value of type int to type string".into()),
    ] {
        assert_eq!(e.message(), e.to_string());
    }
}

#[test]
fn test_derives_compare_clone_and_hash() {
    let e = OptionsError::Parse("p".into());
    assert_eq!(e.clone(), e);
    assert_ne!(e, OptionsError::Invalid("p".into()));
    let set: HashSet<OptionsError> = [e.clone(), e].into_iter().collect();
    assert_eq!(set.len(), 1);
    assert!(format!("{:?}", OptionsError::Type("t".into())).contains("Type"));
}
