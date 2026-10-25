/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::attr_in_root_namespace;
use ultragraphdb_proto_pb::pb::{directed_edge, posting};
use ultragraphdb_tool_bench::CommitShape;

#[test]
fn test_the_schema_is_the_dgraph_runs_ten_reverse_types_and_the_hub_name() {
    let schema = CommitShape::schema(10);
    let lines: Vec<&str> = schema.lines().collect();
    assert_eq!(lines.len(), 21);
    assert_eq!(lines[0], "rel_t0_org: uid @reverse .");
    assert_eq!(lines[1], "rel_t0_w: string .");
    assert_eq!(lines[19], "rel_t9_w: string .");
    assert_eq!(lines[20], "name: string @index(exact) .");
}

#[test]
fn test_the_hubs_carry_their_names() {
    let shape = CommitShape::new(1, 1_000, 10, 7);
    let m = shape.hubs(3);
    assert_eq!(m.start_ts, 3);
    assert_eq!(m.edges.len(), 1_000);
    assert_eq!(m.edges[0].entity, 1);
    assert_eq!(m.edges[0].attr, attr_in_root_namespace("name"));
    assert_eq!(m.edges[0].value, b"h0");
    assert_eq!(m.edges[999].entity, 1_000);
}

#[test]
fn test_a_commit_writes_new_records_of_one_type_each_linked_to_a_hub_with_a_string() {
    let mut shape = CommitShape::new(50, 1_000, 10, 7);
    let m = shape.next(12);
    assert_eq!(m.start_ts, 12);
    assert_eq!(m.edges.len(), 100);
    let org = &m.edges[0].attr;
    let ty = org
        .trim_start_matches(&attr_in_root_namespace("rel_t"))
        .trim_end_matches("_org")
        .to_string();
    for (i, pair) in m.edges.chunks(2).enumerate() {
        let (link, value) = (&pair[0], &pair[1]);
        assert_eq!(link.entity, 1_001 + i as u64);
        assert_eq!(link.attr, attr_in_root_namespace(&format!("rel_t{ty}_org")));
        assert_eq!(link.value_type, posting::ValType::Uid as i32);
        assert!((1..=1_000).contains(&link.value_id));
        assert_eq!(link.op, directed_edge::Op::Set as i32);
        assert_eq!(value.entity, link.entity);
        assert_eq!(value.attr, attr_in_root_namespace(&format!("rel_t{ty}_w")));
        assert_eq!(value.value, b"x");
    }
    // The next commit's records are new nodes.
    assert_eq!(shape.next(13).edges[0].entity, 1_051);
}

#[test]
fn test_a_seed_replays_its_commits_and_the_types_vary() {
    let mut a = CommitShape::new(1, 1_000, 10, 7);
    let mut b = CommitShape::new(1, 1_000, 10, 7);
    let attrs: Vec<String> = (0..50).map(|_| a.next(1).edges[0].attr.clone()).collect();
    let again: Vec<String> = (0..50).map(|_| b.next(1).edges[0].attr.clone()).collect();
    assert_eq!(attrs, again);
    let mut distinct = attrs.clone();
    distinct.sort();
    distinct.dedup();
    assert!(distinct.len() > 5, "{distinct:?}");
    let mut one = CommitShape::new(1, 10, 1, 7);
    assert!((0..50).all(|_| one.next(1).edges[0].attr == attr_in_root_namespace("rel_t0_org")));
}
