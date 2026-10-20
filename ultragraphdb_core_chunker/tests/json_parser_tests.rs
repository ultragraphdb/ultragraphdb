/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/json_parser_test.go. Changed for ultragraphdb: translated to Rust; the cases
 * whose expectation needs no running cluster. Go's test marshals its structs to JSON; the JSON is written out here
 * as Go marshals it, with a fixed time in place of time.Now(). FastParse runs the same parser.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashMap;

use ultragraphdb_core_chunker::{ChunkerError, ChunkerErrorEnum, JsonOp, NQuadBuffer, parse_json};
use ultragraphdb_core_types::parse_time;
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::facet::ValType;
use ultragraphdb_proto_dgraph::api::value::Val;

/// Go's `Parse`: the N-Quads a buffer collected, also when parsing failed part way.
fn parse(b: &str, op: JsonOp) -> Result<Vec<api::NQuad>, ChunkerError> {
    let (mut buf, rx) = NQuadBuffer::new(1000);
    let res = buf.parse_json(b.as_bytes(), op);
    buf.flush();
    res.map(|()| rx.recv().unwrap_or_default())
}

fn make_nquad(sub: &str, pred: &str, val: Val) -> api::NQuad {
    api::NQuad {
        subject: sub.to_string(),
        predicate: pred.to_string(),
        object_value: Some(api::Value { val: Some(val) }),
        ..Default::default()
    }
}

fn make_nquad_edge(sub: &str, pred: &str, obj: &str) -> api::NQuad {
    api::NQuad {
        subject: sub.to_string(),
        predicate: pred.to_string(),
        object_id: obj.to_string(),
        ..Default::default()
    }
}

fn check_count(nq: &[api::NQuad], pred: &str, count: usize) {
    if let Some(n) = nq.iter().find(|n| n.predicate == pred) {
        assert_eq!(count, n.facets.len());
    }
}

fn check_facets(nq: &[api::NQuad], pred: &str, facets: &[api::Facet]) {
    for n in nq.iter().filter(|n| n.predicate == pred) {
        assert_eq!(
            facets.len(),
            n.facets.len(),
            "expected {} facets, got {}",
            facets.len(),
            n.facets.len()
        );
        let actual: HashMap<&str, &api::Facet> =
            n.facets.iter().map(|f| (f.key.as_str(), f)).collect();
        for f in facets {
            let actual_f = actual
                .get(f.key.as_str())
                .unwrap_or_else(|| panic!("facet for key {} not found", f.key));
            assert_eq!(&f, actual_f);
        }
    }
}

fn facet(key: &str, value: Vec<u8>, val_type: ValType, tokens: &[&str]) -> api::Facet {
    api::Facet {
        key: key.to_string(),
        value,
        val_type: val_type as i32,
        tokens: tokens.iter().map(|t| t.to_string()).collect(),
        alias: String::new(),
    }
}

#[test]
fn test_nquads_from_json1() {
    let b = r#"{"uid":"1","namespace":"0x2","name":"Alice","age":26,"married":true,"now":"2026-10-20T10:11:12.123456789Z","address":{"type":"Point","coordinates":[1.1,2]}}"#;
    let nq = parse(b, JsonOp::Set).unwrap();
    assert_eq!(5, nq.len());
    assert!(nq.iter().all(|n| n.subject == "1" && n.namespace == 2));
}

#[test]
fn test_nquads_from_json2() {
    let b = r#"{"name":"Alice","address":{},"friend":[{"name":"Charlie","married":false,"address":{}},{"uid":"1000","name":"Bob","address":{}}]}"#;
    let nq = parse(b, JsonOp::Set).unwrap();
    assert_eq!(6, nq.len());
}

#[test]
fn test_nquads_from_json3() {
    let b = r#"{"name":"Alice","address":{},"school":{"name":"Wellington Public School"}}"#;
    let nq = parse(b, JsonOp::Set).unwrap();
    assert_eq!(3, nq.len());
}

#[test]
fn test_nquads_from_json4() {
    let json =
        r#"[{"name":"Alice","mobile":"040123456","car":"MA0123", "age": 21, "weight": 58.7}]"#;
    let nq = parse(json, JsonOp::Set).unwrap();
    assert_eq!(5, nq.len());
}

#[test]
fn test_nquads_from_json_map() {
    let json = "{\"name\":\"Alice\",\n\"age\": 25,\n\"friends\": [{\n\"name\": \"Bob\"\n}]}";
    let nq = parse(json, JsonOp::Set).unwrap();
    assert_eq!(4, nq.len());
}

#[test]
fn test_nquads_from_multiple_json_objects() {
    let json = r#"[{"name": "A","age": 25,"friends": [{"name": "A1","friends": [{"name": "A11"},{"name": "A12"}]},
        {"name": "A2","friends": [{"name": "A21"},{"name": "A22"}]}]},{"name": "B","age": 26,"friends": [{"name": "B1",
        "friends": [{"name": "B11"},{"name": "B12"}]},{"name": "B2","friends": [{"name": "B21"},{"name": "B22"}]}]}]"#;
    let nq = parse(json, JsonOp::Set).unwrap();
    assert_eq!(28, nq.len());
}

#[test]
fn test_json_number_parsing() {
    let tests: [(&str, Option<Val>); 7] = [
        (
            r#"{"uid": "1", "key": 9223372036854775299}"#,
            Some(Val::IntVal(9223372036854775299)),
        ),
        (
            r#"{"uid": "1", "key": 9223372036854775299.0}"#,
            Some(Val::DoubleVal(9223372036854775299.0)),
        ),
        (r#"{"uid": "1", "key": 27670116110564327426}"#, None),
        (
            r#"{"uid": "1", "key": "23452786"}"#,
            Some(Val::StrVal("23452786".into())),
        ),
        (
            r#"{"uid": "1", "key": "23452786.2378"}"#,
            Some(Val::StrVal("23452786.2378".into())),
        ),
        (
            r#"{"uid": "1", "key": -1e10}"#,
            Some(Val::DoubleVal(-1e+10)),
        ),
        (r#"{"uid": "1", "key": 0E-0}"#, Some(Val::DoubleVal(0.0))),
    ];
    for (input, out) in tests {
        let nqs = parse(input, JsonOp::Set);
        match out {
            Some(out) => assert_eq!(make_nquad("1", "key", out), nqs.unwrap()[0]),
            None => assert!(nqs.is_err(), "{input}"),
        }
    }
}

#[test]
fn test_nquads_from_json_uid_out_of_range_error() {
    let json = r#"{"uid":"0xa14222b693e4ba34123","name":"Name","following":[{"name":"Bob"}],"school":[{"uid":"","name@en":"Crown Public School"}]}"#;
    assert!(parse(json, JsonOp::Set).is_err());
}

#[test]
fn test_nquads_from_json_array() {
    let json = r#"[
		{
			"uid": "uid(Project10)",
			"Ticket.row": {
				"uid": "uid(x)"
			}
		},
		{
			"Project.columns": [
				{
					"uid": "uid(x)"
				}
			],
			"uid": "uid(Project3)"
		},
		{
			"Ticket.onColumn": {
				"uid": "uid(x)"
			},
			"uid": "uid(Ticket4)"
		}
	]"#;
    let nqs = parse(json, JsonOp::Set).unwrap();
    assert_eq!(3, nqs.len());
}

#[test]
fn test_nquads_from_json_negative_uid_error() {
    let json = r#"{"uid":"-100","name":"Name","following":[{"name":"Bob"}],"school":[{"uid":"","name@en":"Crown Public School"}]}"#;
    assert!(parse(json, JsonOp::Set).is_err());
}

#[test]
fn test_nquads_from_json_empty_uid() {
    let json = "{\"uid\":\"\",\"name\":\"Alice\",\"following\":[{\"name\":\"Bob\"}],\"school\":[{\"uid\":\"\",\n\"name\":\"Crown Public School\"}]}";
    assert_eq!(parse(json, JsonOp::Set).unwrap().len(), 5);
}

#[test]
fn test_nquads_from_json_blank_nodes() {
    let json = r#"{"uid":"_:alice","name":"Alice","following":[{"name":"Bob"}],"school":[{"uid":"_:school","name":"Crown Public School"}]}"#;
    let nq = parse(json, JsonOp::Set).unwrap();
    assert_eq!(nq.len(), 5);
    assert!(
        nq.iter()
            .any(|n| n.subject == "_:alice" && n.object_id == "_:school")
    );
}

#[test]
fn test_nquads_delete_edges() {
    let json = r#"[{"uid": "0x1","name":null,"mobile":null,"car":null}]"#;
    assert_eq!(3, parse(json, JsonOp::Delete).unwrap().len());
}

#[test]
fn test_nquads_from_json_facets1() {
    // test the 5 data types on facets, string, bool, int, float and datetime
    let operation = "READ WRITE";
    let operation_tokens = ["\u{1}read", "\u{1}write"];
    let time_str = "2006-01-02T15:04:05Z";
    let time_binary = parse_time(time_str).unwrap().marshal_binary().unwrap();
    let car_price = 30000.56f64;
    let price_bytes = car_price.to_bits().to_le_bytes().to_vec();
    let car_age = 3u64;
    let age_bytes = car_age.to_le_bytes().to_vec();

    // fmt.Sprintf with %d and %f, as Go's test writes it.
    let json = format!(
        "[{{\"name\":\"Alice\",\"mobile\":\"040123456\",\"car\":\"MA0123\",\"mobile|operation\": \"{operation}\",
         \"car|first\":true,
         \"car|age\": {car_age},
         \"car|price\": {car_price:.6},
         \"car|since\": \"{time_str}\"
}}]"
    );

    let nq = parse(&json, JsonOp::Set).unwrap();
    assert_eq!(3, nq.len());

    check_facets(
        &nq,
        "mobile",
        &[facet(
            "operation",
            operation.as_bytes().to_vec(),
            ValType::String,
            &operation_tokens,
        )],
    );
    check_facets(
        &nq,
        "car",
        &[
            facet("first", vec![1], ValType::Bool, &[]),
            facet("age", age_bytes, ValType::Int, &[]),
            facet("price", price_bytes, ValType::Float, &[]),
            facet("since", time_binary, ValType::Datetime, &[]),
        ],
    );
}

#[test]
fn test_nquads_from_json_facets2() {
    // Dave has uid facets which should go on the edge between Alice and Dave
    let json = r#"[{"name":"Alice","friend":[{"name":"Dave","friend|close":"true"}]}]"#;
    let nq = parse(json, JsonOp::Set).unwrap();
    assert_eq!(3, nq.len());
    check_count(&nq, "friend", 1);
}

// Test valid facets json.
#[test]
fn test_nquads_from_json_facets3() {
    let json = r#"
	[
		{
			"name":"Alice",
			"friend": ["Joshua", "David", "Josh"],
			"friend|from": {
				"0": "school",
				"2": "college"
			},
			"friend|age": {
				"1": 20,
				"2": 21
			}
		}
	]"#;
    let nqs = parse(json, JsonOp::Set).unwrap();
    assert_eq!(4, nqs.len());
    for nq in &nqs {
        let pred_val = match nq.object_value.as_ref().and_then(|v| v.val.as_ref()) {
            Some(Val::StrVal(s)) => s.as_str(),
            _ => "",
        };
        match pred_val {
            "Alice" => assert_eq!(0, nq.facets.len()),
            "Joshua" => assert_eq!(1, nq.facets.len()),
            "David" => assert_eq!(1, nq.facets.len()),
            "Josh" => assert_eq!(2, nq.facets.len()),
            _ => {}
        }
    }
}

// Test invalid facet format with scalar list predicate.
#[test]
fn test_nquads_from_json_facets4() {
    let inputs: [(&str, bool, &str); 9] = [
        (
            "facets_should_be_map",
            true,
            r#"[{"name":"Alice","friend": ["Joshua", "David", "Josh"],"friend|age": 20}]"#,
        ),
        (
            "predicate_should_be_list",
            true,
            r#"[{"name":"Alice","friend": "Joshua","friend|age": {"0": 20}}]"#,
        ),
        (
            "only_scalar_values_in_facet_map",
            true,
            r#"[{"name":"Alice","friend": ["Joshua"],"friend|age": {"0": {"1": 20}}}]"#,
        ),
        (
            "invalid_key_in_facet_map",
            true,
            r#"[{"name":"Alice","friend": ["Joshua"],"friend|age": {"a": 20}}]"#,
        ),
        (
            // Facets will be ignored here.
            "predicate_is_null",
            false,
            r#"[{"name":"Alice","friend": null,"friend|age": {"0": 20}}]"#,
        ),
        (
            // Facets will be ignored here.
            "empty_scalar_list",
            false,
            r#"[{"name":"Alice","friend": [],"friend|age": {"0": 20}}]"#,
        ),
        (
            "facet_map_is_null",
            false,
            r#"[{"name":"Alice","friend": ["Joshua"],"friend|age": null}]"#,
        ),
        (
            "facet_vales_should_not_be_list",
            true,
            r#"[{"name":"Alice","friend": ["Joshua", "David", "Josh"],"friend|age": ["20"]}]"#,
        ),
        (
            // Facets with higher index will be ignored.
            "facet_map_with_index_greater_than_scalarlist_length",
            false,
            r#"[{"name":"Alice","friend": ["Joshua", "David", "Josh"],"friend|age": {"100": 30,"20": 28}}]"#,
        ),
    ];
    for (name, error_out, json) in inputs {
        let res = parse(json, JsonOp::Set);
        assert_eq!(
            res.is_err(),
            error_out,
            "TestNquadsFromJsonFacets4-{name}: {res:?}"
        );
    }
}

#[test]
fn test_nquads_from_json_facets5() {
    // Dave has uid facets which should go on the edge between Alice and Dave,
    // AND Emily has uid facets which should go on the edge between Dave and Emily
    let json = r#"[
		{
			"name": "Alice",
			"friend": [
				{
					"name": "Dave",
					"friend|close": true,
					"friend": [
						{
							"name": "Emily",
							"friend|close": true
						}
					]
				}
			]
		}
	]"#;
    let nq = parse(json, JsonOp::Set).unwrap();
    assert_eq!(5, nq.len());
    check_count(&nq, "friend", 1);
}

#[test]
fn test_nquads_from_json_error1() {
    let b = r#"{"name":"Alice","address":{},"school":{"name":"Wellington Public School"}}"#;
    let err = parse(b, JsonOp::Delete).unwrap_err();
    assert_eq!(err.kind(), &ChunkerErrorEnum::EmptyUid);
    assert_eq!(
        err.to_string(),
        "UID must be present and non-zero while deleting edges"
    );
}

#[test]
fn test_nquads_from_json_list() {
    let json = r#"{"address":["Riley Street","Redfern"],"phone_number":[123,9876],"points":[{"type":"Point", "coordinates":[1.1,2.0]},{"type":"Point", "coordinates":[2.0,1.1]}]}"#;
    assert_eq!(6, parse(json, JsonOp::Set).unwrap().len());
}

#[test]
fn test_nquads_from_json_delete() {
    let json = r#"{"uid":1000,"friend":[{"uid":1001}]}"#;
    let nq = parse(json, JsonOp::Delete).unwrap();
    assert_eq!(nq[0], make_nquad_edge("1000", "friend", "1001"));
}

#[test]
fn test_nquads_from_json_delete_star() {
    let json = r#"{"uid":1000,"name": null}"#;
    let nq = parse(json, JsonOp::Delete).unwrap();
    assert_eq!(
        make_nquad("1000", "name", Val::DefaultVal("_STAR_ALL".into())),
        nq[0]
    );
}

#[test]
fn test_val_in_upsert() {
    let json = r#"{"uid":1000, "name": "val(name)"}"#;
    let nq = parse(json, JsonOp::Set).unwrap();
    assert_eq!(make_nquad_edge("1000", "name", "val(name)"), nq[0]);
}

#[test]
fn test_nquads_from_json_delete_star_lang() {
    let json = r#"{"uid":1000,"name@es": null}"#;
    let nq = parse(json, JsonOp::Delete).unwrap();
    let mut expected = make_nquad("1000", "name", Val::DefaultVal("_STAR_ALL".into()));
    expected.lang = "es".to_string();
    assert_eq!(expected, nq[0]);
}

#[test]
fn test_set_nquad_nil_value() {
    let json = r#"{"uid":1000,"name": null}"#;
    assert_eq!(0, parse(json, JsonOp::Set).unwrap().len());
}

// See PR #7737 to understand why this test exists.
#[test]
fn test_nquads_from_json_empty_facet() {
    let json = r#"{"uid":1000,"doesnt|exist":null}"#;
    let (mut buf, rx) = NQuadBuffer::new(-1);
    buf.parse_json(json.as_bytes(), JsonOp::Delete).unwrap();
    buf.flush();
    // needs to be empty, otherwise node gets deleted
    assert_eq!(0, rx.recv().unwrap_or_default().len());
}

#[test]
fn test_nquads_empty_string_from_json() {
    let json = r#"[{"name":""}]"#;
    let nq = parse(json, JsonOp::Set).unwrap();
    // The string value should be empty.
    assert_eq!(
        nq[0].object_value,
        Some(api::Value {
            val: Some(Val::StrVal(String::new()))
        })
    );
}

#[test]
fn test_nquads_json_empty_string_vector_pred() {
    let b = r#"[{"uid":"1","name":"","description":"","description_v":""}]"#;
    assert_eq!(3, parse(b, JsonOp::Set).unwrap().len());
}

#[test]
fn test_nquads_json_empty_square_bracket_vector_pred() {
    let b = r#"{"name":"ipad","description":"","description_v":"[]"}"#;
    let nq = parse(b, JsonOp::Set).unwrap();
    assert_eq!(3, nq.len());
    let v = nq.iter().find(|n| n.predicate == "description_v").unwrap();
    assert_eq!(
        v.object_value,
        Some(api::Value {
            val: Some(Val::StrVal("[]".into()))
        })
    );
}

#[test]
fn test_nquads_json_valid_vector() {
    let b = r#"{"name":"ipad","description":"","description_v":"[1.1, 2.2, 3.3]"}"#;
    let nq = parse(b, JsonOp::Set).unwrap();
    assert_eq!(3, nq.len());
    let v = nq.iter().find(|n| n.predicate == "description_v").unwrap();
    let bytes: Vec<u8> = [1.1f32, 2.2, 3.3]
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    assert_eq!(
        v.object_value,
        Some(api::Value {
            val: Some(Val::Vfloat32Val(bytes))
        })
    );
}

// Beyond Dgraph's tests.

#[test]
fn parse_json_returns_the_nquads_and_their_hints() {
    let (nqs, md) = parse_json(
        br#"{"uid": "0x1", "name": "a", "tags": ["x", "y"]}"#,
        JsonOp::Set,
    )
    .unwrap();
    assert_eq!(nqs.len(), 3);
    assert_eq!(md.pred_hints["name"], 1);
    assert_eq!(md.pred_hints["tags"], 2);
}

#[test]
fn blank_nodes_share_one_random_id_and_count_up() {
    let blank = |nqs: &[api::NQuad]| -> (u32, u64) {
        let (id, n) = nqs[0]
            .subject
            .strip_prefix("_:dg.")
            .unwrap()
            .split_once('.')
            .unwrap();
        (id.parse().unwrap(), n.parse().unwrap())
    };
    let (a, _) = parse_json(br#"{"name": "a"}"#, JsonOp::Set).unwrap();
    let (b, _) = parse_json(br#"{"name": "b"}"#, JsonOp::Set).unwrap();
    let (a, b) = (blank(&a), blank(&b));
    assert_eq!(a.0, b.0);
    // Other tests running at the same time draw from the same counter.
    assert!(a.1 >= 1 && b.1 > a.1);
}

#[test]
fn a_uid_edge_takes_the_facets_of_its_child_object() {
    let (nqs, _) = parse_json(
        br#"{"uid": "_:a", "friend": {"uid": "_:b", "friend|close": true}}"#,
        JsonOp::Set,
    )
    .unwrap();
    assert_eq!(nqs.len(), 1);
    assert_eq!(nqs[0].object_id, "_:b");
    assert_eq!(nqs[0].facets[0].key, "close");
}

#[test]
fn delete_of_a_bare_uid_deletes_every_predicate() {
    let (nqs, _) = parse_json(br#"{"uid": "0x1", "namespace": "0x2"}"#, JsonOp::Delete).unwrap();
    assert!(nqs.is_empty(), "namespace is a second key: {nqs:?}");
    let (nqs, _) = parse_json(br#"{"uid": "0x1"}"#, JsonOp::Delete).unwrap();
    assert_eq!(
        nqs,
        [make_nquad(
            "1",
            "_STAR_ALL",
            Val::DefaultVal("_STAR_ALL".into())
        )]
    );
}

#[test]
fn json_that_does_not_parse_is_refused() {
    for input in [
        "",
        "{",
        r#"{"a":}"#,
        "[1]",
        r#""abc""#,
        "{\"a\":\"b\"}\u{1}",
    ] {
        let res = parse_json(input.as_bytes(), JsonOp::Set);
        if input.ends_with('\u{1}') {
            // Go decodes the first value and ignores what follows it.
            assert!(res.is_ok(), "{input:?}: {res:?}");
        } else {
            assert!(res.is_err(), "{input:?}");
        }
    }
}

#[test]
fn a_lone_surrogate_becomes_a_replacement_character() {
    let (nqs, _) = parse_json(
        br#"{"uid": "0x1", "n\ud800": "a\udc00b\ud83d\ude00"}"#,
        JsonOp::Set,
    )
    .unwrap();
    assert_eq!(nqs[0].predicate, "n\u{fffd}");
    assert_eq!(
        nqs[0].object_value,
        Some(api::Value {
            val: Some(Val::StrVal("a\u{fffd}b\u{1f600}".into()))
        })
    );
}

#[test]
fn bytes_that_are_not_utf8_in_a_string_become_replacement_characters() {
    let (nqs, _) = parse_json(b"{\"uid\": \"0x1\", \"n\": \"a\xe2\x82b\"}", JsonOp::Set).unwrap();
    assert_eq!(
        nqs[0].object_value,
        Some(api::Value {
            val: Some(Val::StrVal("a\u{fffd}\u{fffd}b".into()))
        })
    );
}
