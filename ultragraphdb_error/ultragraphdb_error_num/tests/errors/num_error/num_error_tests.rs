/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashSet;

use ultragraphdb_error_num::{NumError, NumErrorEnum};

#[test]
fn test_constructors_build_their_variant() {
    assert_eq!(
        NumError::Syntax("ParseInt", "\"x\"".to_string()).0,
        NumErrorEnum::Syntax {
            func: "ParseInt",
            num: "\"x\"".to_string()
        }
    );
    assert_eq!(
        NumError::Range("ParseUint", "\"99\"".to_string()).0,
        NumErrorEnum::Range {
            func: "ParseUint",
            num: "\"99\"".to_string()
        }
    );
}

#[test]
fn test_the_classification_is_the_public_field() {
    let e = NumError(NumErrorEnum::Range {
        func: "Atoi",
        num: "\"1\"".to_string(),
    });
    assert!(matches!(e.0, NumErrorEnum::Range { func: "Atoi", .. }));
}

#[test]
fn test_derives_compare_clone_and_hash() {
    let e = NumError::Syntax("ParseBool", "\"yes\"".to_string());
    assert_eq!(e.clone(), e);
    assert_ne!(e, NumError::Range("ParseBool", "\"yes\"".to_string()));
    let set: HashSet<NumError> = [e.clone(), e].into_iter().collect();
    assert_eq!(set.len(), 1);
    assert!(format!("{:?}", NumError::Syntax("ParseFloat", "\"\"".to_string())).contains("Syntax"));
}
