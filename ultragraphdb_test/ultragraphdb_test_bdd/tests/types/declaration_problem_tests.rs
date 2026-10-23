/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_test_bdd::DeclarationProblem;

#[test]
fn display_names_the_file_and_the_problem() {
    let p = DeclarationProblem::new("a/BUILD.bazel", "missing x");
    assert_eq!(p.to_string(), "a/BUILD.bazel: missing x");
}
