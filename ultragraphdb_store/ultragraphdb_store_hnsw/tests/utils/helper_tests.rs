/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_hnsw_test.go (TestEdgePruningKeepsBestNeighbors). Changed for
 * ultragraphdb: translated to Rust; added: ParseEdges against Go's results, the dead list's update, the uid
 * encodings and the vector decoding.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashMap;
use std::convert::Infallible;

use serde::Deserialize;
use ultragraphdb_store_hnsw::{
    SimilarityType, add_dead_node, bytes_as_float_array, bytes_to_uint64, concat_strings,
    insert_uids_top_k_by_score, is_equal, parse_edges, uint64_to_bytes, worst_score,
};
use ultragraphdb_tool_bdd::{load_golden, testdata_dir};

#[test]
fn test_edge_pruning_keeps_best_neighbors() {
    for (metric, expected) in [
        ("euclidean", vec![0.1, 0.2]),
        ("cosine", vec![0.5, 0.9]),
        ("dotproduct", vec![0.5, 0.9]),
    ] {
        let sim = SimilarityType::from_name(metric);
        let scores = [0.5, 0.1, 0.9, 0.2];
        let by_uid: HashMap<u64, f64> = scores
            .iter()
            .enumerate()
            .map(|(i, &s)| (i as u64 + 1, s))
            .collect();
        let uids: Vec<u64> = (1..=4).collect();
        let kept = insert_uids_top_k_by_score(Vec::new(), &uids, 2, sim, |uid| {
            Ok::<_, Infallible>(by_uid[&uid])
        })
        .unwrap();
        let mut kept_scores: Vec<f64> = kept.iter().map(|u| by_uid[u]).collect();
        let best_first = kept_scores.clone();
        kept_scores.sort_by(f64::total_cmp);
        assert_eq!(kept_scores, expected, "{metric}");
        // Best first, by hand: a distance's lowest scores, a similarity's highest.
        let mut by_rank = expected.clone();
        if metric != "euclidean" {
            by_rank.reverse();
        }
        assert_eq!(best_first, by_rank, "{metric}");
    }
}

#[test]
fn test_insert_uids_top_k_drops_not_a_uid_and_repeats() {
    let sim = SimilarityType::from_name("euclidean");
    let kept = insert_uids_top_k_by_score(vec![4, 0, 4, 2], &[0, 3, 2, 1], 3, sim, |u| {
        Ok::<_, Infallible>(u as f64)
    })
    .unwrap();
    // The edges are cut to `keep` first, [4, 0, 4] keeping 4 alone; 2 then counts as new.
    assert_eq!(kept, vec![1, 2, 3]);
    let kept =
        insert_uids_top_k_by_score(vec![4, 9], &[5], 3, sim, |u| Ok::<_, Infallible>(u as f64))
            .unwrap();
    assert_eq!(kept, vec![4, 5, 9], "a new uid goes in by score");
    // Unsorted edges are bisected as Go's sort.Search bisects them.
    let kept =
        insert_uids_top_k_by_score(vec![9, 4], &[5], 3, sim, |u| Ok::<_, Infallible>(u as f64))
            .unwrap();
    assert_eq!(kept, vec![9, 4, 5]);
    assert!(
        insert_uids_top_k_by_score(vec![1], &[2], 0, sim, |u| Ok::<_, Infallible>(u as f64))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        insert_uids_top_k_by_score(vec![1], &[2], 3, sim, |_| Err::<f64, _>("score failed")),
        Err("score failed")
    );
    // A similarity keeps the highest first; a new uid that ties goes after the uids it does not beat, and the list is
    // cut to `keep` after each insertion.
    let dot = SimilarityType::from_name("dotproduct");
    let kept = insert_uids_top_k_by_score(vec![9, 5, 2], &[7, 6, 1], 4, dot, |u| {
        Ok::<_, Infallible>(match u {
            7 => 5.0,
            6 => 9.5,
            other => other as f64,
        })
    })
    .unwrap();
    assert_eq!(kept, vec![6, 9, 5, 7]);
}

#[derive(Debug, Deserialize)]
struct ParseEdgesCase {
    input: String,
    uids: Option<Vec<u64>>,
    error: String,
}

#[test]
fn test_parse_edges_equals_gos() {
    let golden =
        load_golden::<ParseEdgesCase>(&testdata_dir().join("golden/hnsw_parse_edges.json"));
    assert_eq!(golden.cases().len(), 32);
    for c in golden.cases() {
        match parse_edges(&c.input) {
            Ok(uids) => {
                assert!(
                    c.error.is_empty(),
                    "{:?}: Go refused with {}",
                    c.input,
                    c.error
                );
                assert_eq!(uids, c.uids.clone().unwrap_or_default(), "{:?}", c.input);
            }
            Err(e) => assert_eq!(e.to_string(), c.error, "{:?}", c.input),
        }
    }
}

#[test]
fn test_the_dead_list_is_gos_json() {
    let one = add_dead_node(None, 5).unwrap();
    assert_eq!(one, b"[5]");
    let two = add_dead_node(Some(&one), 9).unwrap();
    assert_eq!(two, b"[5,9]");
    assert_eq!(
        add_dead_node(Some(&two), 5).unwrap(),
        b"[5,9,5]",
        "a uid listed again, as Dgraph lists it"
    );
    assert_eq!(
        add_dead_node(Some(b"nope"), 1).unwrap_err().to_string(),
        "Cannot convert nope to uint slice"
    );
    assert_eq!(
        add_dead_node(Some(b""), 4).unwrap(),
        b"[4]",
        "blank is no uids"
    );
    assert_eq!(
        parse_edges(&String::from_utf8_lossy(&two)).unwrap(),
        vec![5, 9]
    );
}

#[test]
fn test_the_small_helpers() {
    assert_eq!(uint64_to_bytes(0x0102), vec![0, 0, 0, 0, 0, 0, 1, 2]);
    assert_eq!(bytes_to_uint64(&uint64_to_bytes(u64::MAX)), Some(u64::MAX));
    assert_eq!(concat_strings(&["0-a", "__vector_"]), "0-a__vector_");
    assert!(is_equal(&[1.0, 2.0], &[1.0, 2.0]) && !is_equal(&[1.0], &[1.0, 2.0]));
    assert_eq!(
        worst_score::<f32>(SimilarityType::from_name("euclidean")),
        f32::INFINITY
    );
    assert_eq!(
        worst_score::<f64>(SimilarityType::from_name("cosine")),
        f64::NEG_INFINITY
    );
    assert_eq!(
        worst_score::<f32>(SimilarityType::from_name("dotproduct")),
        f32::NEG_INFINITY
    );
    assert_eq!(bytes_to_uint64(&[0, 0, 0, 0, 0, 0, 1, 2]), Some(0x0102));
    assert_eq!(bytes_as_float_array::<f32>(&[]), Some(vec![]));
    assert_eq!(
        bytes_as_float_array::<f32>(&1.5f32.to_le_bytes()),
        Some(vec![1.5])
    );
    assert_eq!(
        bytes_as_float_array::<f64>(&(-2.0f64).to_le_bytes()),
        Some(vec![-2.0])
    );
}

#[test]
fn test_a_misaligned_vector_is_logged_as_dgraph_logs_it() {
    let capture = ultragraphdb_store_sim::LogCapture::new();
    let decoded = capture.capture(|| bytes_as_float_array::<f32>(&[1, 2, 3]));
    assert_eq!(decoded, None);
    let events = capture.events();
    assert_eq!(events.len(), 1);
    assert!(
        format!("{:?}", events[0]).contains("Invalid byte slice length"),
        "{events:?}"
    );
}
