/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/dql_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_dql::parse_dql;

use crate::test_helpers::no_err;

#[test]
fn test_parse_dql() {
    let test_cases = [
        r#"{ set {_:a <name> "Alice" .}}"#,
        "schema{}",
        r#"upsert {
			query {
				ns(func: eq(dgraph.namespace.name, "ns1")) {
					q as var
					dgraph.namespace.id
				}
			}

			mutation {
				delete {
					uid(q) * * .
				}
			}
		}"#,
        r#"{
			q(func: eq(dgraph.namespace.name, "ns1")) {
				uid
				dgraph.type
				dgraph.namespace.id
			}
		}"#,
        r#" query test($a: int) {
			q(func: uid(0x1)) {
				x as count(uid)
				p : math(x + $a)
			}
		}"#,
        r#"{
			me(func: uid(1)) {
				Upvote {
					u as Author
				}
				count(val(u))
			}
		}"#,
        r#"{
			user(func: uid(0x0a)) {
				...fragmenta,...fragmentb
				friends {
					name
				}
				...fragmentc
				hobbies
				...fragmentd
			}
			me(func: uid(0x01)) {
				...fragmenta
				...fragmentb
			}
		}
		fragment fragmenta {
			name
		}
		fragment fragmentb {
			id
		}
		fragment fragmentc {
			name
		}
		fragment fragmentd {
			id
		}"#,
        r#"{
			set {
				<name> <is> <something> .
				<hometown> <is> <san/francisco> .
			}
			delete {
				<name> <is> <something-else> .
			}
		}"#,
        r#"{
			<user-query>(func: uid(0x1)) {
				name
			}
		}"#,
    ];
    for tc in test_cases {
        let err = parse_dql(tc).err();
        no_err(&err);
    }
}
