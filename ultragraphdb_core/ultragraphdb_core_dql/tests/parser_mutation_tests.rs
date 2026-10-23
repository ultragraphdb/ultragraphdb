/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/upsert_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_dql::parse_dql;

use crate::test_helpers::{contains, no_err, split};

#[test]
fn test_invalid_block_err() {
    let query = r#"
query {
  me(func: eq(age, 34)) {
    uid
    friend {
      uid
      age
    }
  }
}"#;
    let err = parse_dql(query).err();
    no_err(&err);
}

#[test]
fn test_extra_right_curl_err() {
    let query = r#"
upsert {
  query {
    me(func: eq(age, 34)) {
      uid
      friend {
        uid
        age
      }
    }
  }
}
}
"#;
    let err = parse_dql(query).err();
    assert!(err.is_some());
    contains(&err, "Too many right curl");
}

#[test]
fn test_no_mutation_err() {
    let query = r#"
upsert {
  query {
    me(func: eq(age, 34)) {
      uid
      friend {
        uid age
      }
    }
  }
}
"#;
    let err = parse_dql(query).err();
    assert!(err.is_some());
    contains(&err, "Empty mutation block");
}

#[test]
fn test_multiple_query_err() {
    let query = r#"
upsert {
  query {
    me(func: eq(age, 34)) {
      uid
      friend {
        uid
        age
      }
    }
  }

  query {
    me2(func: eq(age, 34)) {
      uid
      friend {
        uid
        age
      }
    }
  }

  mutation {
    set {
      "_:user1" <age> "45" .
    }
  }
}
"#;
    let err = parse_dql(query).err();
    assert!(err.is_some());
    contains(&err, "Multiple query ops inside upsert block");
}

#[test]
fn test_empty_upsert_err() {
    let query = r#"upsert {}"#;
    let err = parse_dql(query).err();
    assert!(err.is_some());
    contains(&err, "Empty mutation block");
}

#[test]
fn test_no_right_curl_err() {
    let query = r#"upsert {"#;
    let err = parse_dql(query).err();
    contains(&err, "Unclosed upsert block");
}

#[test]
fn test_incomplete_block_err() {
    let query = r#"
upsert {
  mutation {
    set {
      "_:user1" <age> "45" .
    }
  }

  query {
    me(func: eq(age, "{
"#;
    let err = parse_dql(query).err();
    assert!(err.is_some());
    contains(&err, "Unexpected end of input");
}

#[test]
fn test_missing_query_err() {
    let query = r#"
upsert {
  mutation {
    set {
      "_:user1" <age> "45" .
    }
  }
}
"#;
    let err = parse_dql(query).err();
    assert!(err.is_some());
    contains(&err, "Query op not found in upsert block");
}

#[test]
fn test_upsert_with_fragment() {
    let query = r#"
upsert {
  query {
    me(func: eq(age, 34)) {
      ...fragmentA
      friend {
        ...fragmentA
        age
      }
    }
  }

  fragment fragmentA {
    uid
  }

  mutation {
    set {
      "_:user1" <age> "45" .
    }
  }
}
"#;
    let err = parse_dql(query).err();
    no_err(&err);
}

#[test]
fn test_upsert_ex1() {
    let query = r#"
upsert {
  query {
    me(func: eq(age, "{")) {
      uid
      friend {
        uid
        age
      }
    }
  }

  mutation {
    set {
      "_:user1" <age> "45" .
    }
  }
}
"#;
    let err = parse_dql(query).err();
    no_err(&err);
}

#[test]
fn test_upsert_with_spaces() {
    let query = r#"
upsert

{
  query

  {
    me(func: eq(age, "{")) {
      uid
      friend {
        uid
        age
      }
    }
  }

  mutation

  {
    set
    {
      "_:user1" <age> "45" .

      # This is a comment
      "_:user1" <name> "{vishesh" .
    }}
}
"#;
    let err = parse_dql(query).err();
    no_err(&err);
}

#[test]
fn test_upsert_with_blank_node() {
    let query = r#"
upsert {
  mutation {
    set {
      "_:user1" <age> "45" .
    }
  }

  query {
    me(func: eq(age, 34)) {
      uid
      friend {
        uid
        age
      }
    }
  }
}
"#;
    let err = parse_dql(query).err();
    no_err(&err);
}

#[test]
fn test_upsert_mutation_then_query() {
    let query = r#"
upsert {
  query {
    me(func: eq(age, 34)) {
      uid
      friend {
        uid
        age
      }
    }
  }

  mutation {
    set {
      "_:user1" <age> "45" .
    }
  }
}
"#;
    let err = parse_dql(query).err();
    no_err(&err);
}

#[test]
fn test_upsert_with_filter() {
    let query = r#"
upsert {
  query {
    me(func: eq(age, 34)) @filter(ge(name, "user")) {
      uid
      friend {
        uid
        age
      }
    }
  }

  mutation {
    set {
      uid(a) <age> "45"
      uid(b) <age> "45" .
    }
  }
}"#;
    let err = parse_dql(query).err();
    no_err(&err);
}

#[test]
fn test_conditional_upsert_with_newlines() {
    let query = r#"
upsert {
  query {
    me(func: eq(age, 34)) @filter(ge(name, "user")) {
      m as uid
      friend {
        f as uid
        age
      }
    }
  }

  mutation @if(eq(len(m), 1)
               AND
               gt(len(f), 0)) {
    set {
      uid(m) <age> "45" .
      uid(f) <age> "45" .
    }
  }
}
"#;
    let err = parse_dql(query).err();
    no_err(&err);
}

#[test]
fn test_conditional_upsert_func_tree() {
    let query = r#"
upsert {
  query {
    me(func: eq(age, 34)) @filter(ge(name, "user")) {
      uid
      friend {
        uid
        age
      }
    }
  }

  mutation @if( ( eq(len(m), 1)
                  OR
                  lt(90, len(h)))
                AND
                gt(len(f), 0)) {
    set {
      uid(m) <age> "45" .
      uid(f) <age> "45" .
    }
  }
}
"#;
    let err = parse_dql(query).err();
    no_err(&err);
}

#[test]
fn test_conditional_upsert_multiple_func_arg() {
    let query = r#"
upsert {
  query {
    me(func: eq(age, 34)) @filter(ge(name, "user")) {
      uid
      friend {
        uid
        age
      }
    }
  }

  mutation @if( ( eq(len(m), len(t))
                  OR
                  lt(90, len(h)))
                AND
                gt(len(f), 0)) {
    set {
      uid(m) <age> "45" .
      uid(f) <age> "45" .
    }
  }
}
"#;
    let err = parse_dql(query).err();
    no_err(&err);
}

#[test]
fn test_conditional_upsert_err_missing_right_round() {
    let query = r#"
upsert {
  query {
    me(func: eq(age, 34)) @filter(ge(name, "user")) {
      uid
      friend {
        uid
        age
      }
    }
  }

  mutation @if(eq(len(m, 1)
               AND
               gt(len(f), 0)) {
    set {
      uid(m) <age> "45" .
      uid(f) <age> "45" .
    }
  }
}
"#;
    let err = parse_dql(query).err();
    contains(&err, "Matching brackets not found");
}

#[test]
fn test_conditional_upsert_err_unclosed() {
    let query = r#"upsert {
  mutation @if(eq(len(m), 1) AND gt(len(f), 0))"#;
    let err = parse_dql(query).err();
    contains(&err, "Unclosed mutation action");
}

#[test]
fn test_conditional_upsert_err_invalid_if() {
    let query = r#"upsert {
  mutation @if"#;
    let err = parse_dql(query).err();
    contains(&err, "Matching brackets not found");
}

#[test]
fn test_conditional_upsert_err_wrong_if() {
    let query = r#"upsert {
  mutation @fi( ( eq(len(m), 1)
                  OR
                  lt(len(h), 90))
                AND
                gt(len(f), 0)) {
    set {
      uid(m) <age> "45" .
      uid(f) <age> "45" .
    }
  }

  query {
    me(func: eq(age, 34)) @filter(ge(name, "user")) {
      uid
      friend {
        uid
        age
      }
    }
  }
}
"#;
    let err = parse_dql(query).err();
    contains(&err, "Expected @if, found [@fi]");
}

#[test]
fn test_multiple_mutation() {
    let query = r#"
upsert {
  mutation @if(eq(len(m), 1)) {
    set {
      uid(m) <age> "45" .
    }
  }

  mutation @if(not(eq(len(m), 1))) {
    set {
      uid(f) <age> "45" .
    }
  }

  mutation {
    set {
      _:user <age> "45" .
    }
  }

  query {
    me(func: eq(age, 34)) @filter(ge(name, "user")) {
      uid
    }
  }
}"#;
    let (req, err) = split(parse_dql(query));
    no_err(&err);
    assert_eq!(3, req.mutations.len());
}

#[test]
fn test_multiple_mutation_different_order() {
    let query = r#"
upsert {
  mutation @if(eq(len(m), 1)) {
    set {
      uid(m) <age> "45" .
    }
  }

  query {
    me(func: eq(age, 34)) @filter(ge(name, "user")) {
      uid
    }
  }

  mutation @if(not(eq(len(m), 1))) {
    set {
      uid(f) <age> "45" .
    }
  }

  mutation {
    set {
      _:user <age> "45" .
    }
  }
}"#;
    let (req, err) = split(parse_dql(query));
    no_err(&err);
    assert_eq!(3, req.mutations.len());
}
