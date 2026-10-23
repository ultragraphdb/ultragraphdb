/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, worker/mutation_unit_test.go (TestReverseEdge, TestReverseEdgeSetDel) and
 * posting/index_test.go (TestTokensTable). Changed for ultragraphdb: translated to Rust; the edges are applied in a
 * request of the local commit, read back through it, where Dgraph's tests read the transaction's lists.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::parse;
use ultragraphdb_traits_store::{Scan, Store, StoreSnapshot};

use crate::utils::helpers::{
    attr, del_uid, dkey, index_prefix, new_applier, rkey, set_str, set_uid,
};

#[test]
fn test_reverse_edge() {
    let a = new_applier();
    a.schema()
        .parse_text("revc: [uid] @reverse @count .")
        .unwrap();
    let batch = a.batch();
    let mut request = batch.request(5);
    let edge = del_uid("revc", 1, 2);
    request.run_mutation(&edge).unwrap();
    request.run_mutation(&edge).unwrap();
    let list = request.request().get(&dkey("revc", 1)).unwrap();
    assert_eq!(list.get_length(5).unwrap(), 0);
}

#[test]
fn test_reverse_edge_set_del() {
    let a = new_applier();
    a.schema()
        .parse_text("revc: [uid] @reverse @count .")
        .unwrap();
    let batch = a.batch();
    let mut request = batch.request(5);
    for edge in [
        set_uid("revc", 1, 2),
        set_uid("revc", 3, 2),
        set_uid("revc", 4, 2),
        del_uid("revc", 3, 2),
    ] {
        request.run_mutation(&edge).unwrap();
    }
    let list = request.request().get(&rkey("revc", 2)).unwrap();
    assert_eq!(list.get_length(5).unwrap(), 2);
}

#[test]
fn test_tokens_table() {
    let a = new_applier();
    a.schema()
        .parse_text("name: string @index(term) .\nname2: string @index(term) .")
        .unwrap();
    let mut batch = a.batch();
    let mut request = batch.request(1);
    request
        .run_mutation(&set_str("name", 157, "david"))
        .unwrap();
    batch.push(request);
    let ts = batch.commit().unwrap()[0].clone().unwrap();
    let snapshot = a.lists().store().snapshot(ts + 1).unwrap();
    let tokens: Vec<Vec<u8>> = snapshot
        .scan(&Scan::prefix(index_prefix("name")))
        .map(|i| parse(i.unwrap().key()).unwrap().term().to_vec())
        .collect();
    assert_eq!(tokens, vec![b"\x01david".to_vec()]);
    assert_eq!(attr("name"), "0-name");
}

/// A proposal of more than 510 edges is applied a batch per predicate, each in a fork of the request on the
/// executor: the lists it commits equal those of the same edges applied one by one.
#[test]
fn test_edges_applied_apart_equal_edges_applied_in_turn() {
    let schema = "name: string @index(term) @count .\nfriend: [uid] @reverse @count .";
    let edges: Vec<_> = (1..=300u64)
        .map(|uid| {
            set_str(
                "name",
                uid,
                if uid % 2 == 0 { "big apple" } else { "red fig" },
            )
        })
        .chain((1..=300u64).map(|uid| set_uid("friend", uid, uid % 7 + 1)))
        .collect();
    let apart = new_applier();
    crate::utils::helpers::alter_now(&apart, schema);
    let m = ultragraphdb_proto_pb::pb::Mutations {
        start_ts: apart.lists().latest_ts(),
        edges: edges.clone(),
        ..Default::default()
    };
    let mut sorted = edges.clone();
    sorted.sort_by(|a, b| a.attr.cmp(&b.attr).then(a.entity.cmp(&b.entity)));
    assert_eq!(ultragraphdb_store_apply::edge_batches(&sorted).len(), 2);
    apart.apply_mutations(&m).unwrap();

    let in_turn = new_applier();
    crate::utils::helpers::alter_now(&in_turn, schema);
    let mut batch = in_turn.batch();
    let mut request = batch.request(in_turn.lists().latest_ts());
    for e in &sorted {
        request.run_mutation(e).unwrap();
    }
    batch.push(request);
    batch.commit().unwrap()[0].clone().unwrap();

    let keys = |a: &crate::utils::helpers::TestApplier| {
        let mut out = Vec::new();
        for name in ["name", "friend"] {
            let prefix = ultragraphdb_core_keys::predicate_prefix(&attr(name)).unwrap();
            for k in crate::utils::helpers::live_keys(a, &prefix) {
                let u = crate::utils::helpers::uids(a, &k);
                out.push((k, u));
            }
        }
        out
    };
    let (a, b) = (keys(&apart), keys(&in_turn));
    assert!(a.len() > 600, "{} lists", a.len());
    assert_eq!(a, b);
}
