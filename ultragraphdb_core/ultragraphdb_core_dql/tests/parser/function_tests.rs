/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Functions nested in functions count toward the nesting bound of 1,000 levels: each nested function one level
//! below the one holding it, the outermost at the level of its block or filter (ledger 32).

use crate::test_helpers::{MAX_DEPTH, TOO_DEEP, on_small_stack};
use ultragraphdb_core_dql::{Request, parse};

/// The root block's function, at level 1, holding `nested` functions inside one another.
fn nested_functions(nested: usize) -> String {
    format!(
        "{{ v as var(func: uid(1)) {{ a }} q(func: eq({}v{}, 1)) {{ a }} }}",
        "val(".repeat(nested),
        ")".repeat(nested)
    )
}

fn parse_on_small_stack(query: String) -> Result<(), String> {
    on_small_stack(move || {
        parse(&Request::new(query))
            .map(drop)
            .map_err(|e| e.to_string())
    })
}

#[test]
fn test_nested_functions_count_each_level() {
    assert_eq!(
        parse_on_small_stack(nested_functions(MAX_DEPTH - 1)),
        Ok(())
    );
    assert_eq!(
        parse_on_small_stack(nested_functions(MAX_DEPTH)),
        Err(TOO_DEEP.to_string())
    );
    assert_eq!(
        parse_on_small_stack(nested_functions(100 * MAX_DEPTH)),
        Err(TOO_DEEP.to_string())
    );
}

#[test]
fn test_nested_functions_in_a_filter_count_below_it() {
    // The filter's function sits at level 2, below the root block.
    let query = |nested: usize| {
        format!(
            "{{ v as var(func: uid(1)) {{ a }} q(func: uid(1)) @filter(eq({}v{}, 1)) {{ a }} }}",
            "val(".repeat(nested),
            ")".repeat(nested)
        )
    };
    assert_eq!(parse_on_small_stack(query(MAX_DEPTH - 2)), Ok(()));
    assert_eq!(
        parse_on_small_stack(query(MAX_DEPTH - 1)),
        Err(TOO_DEEP.to_string())
    );
}
