/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_hnsw_test.go (TestEdgePruningKeepsBestNeighbors). Changed for
 * ultragraphdb: translated to Rust; added: the dead list's parsing and update over system IDs as 32 hexadecimal
 * digits, the entry point's encoding and the vector decoding.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashMap;
use std::convert::Infallible;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_store_hnsw::{
    SimilarityType, add_dead_node, bytes_as_float_array, bytes_to_id, concat_strings,
    insert_uids_top_k_by_score, is_equal, parse_edges, worst_score,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::helpers::{id, ids, nums};

/// The score the tests give a node: its number.
fn number(u: SystemId) -> f64 {
    u.sequence().get() as f64
}

#[test]
fn test_edge_pruning_keeps_best_neighbors() -> TestResult {
    for (metric, expected) in [
        ("euclidean", vec![0.1, 0.2]),
        ("cosine", vec![0.5, 0.9]),
        ("dotproduct", vec![0.5, 0.9]),
    ] {
        let sim = SimilarityType::from_name(metric);
        let scores = [0.5, 0.1, 0.9, 0.2];
        let by_uid: HashMap<SystemId, f64> = scores
            .iter()
            .enumerate()
            .map(|(i, &s)| Ok((id(i as u64 + 1)?, s)))
            .collect::<TestResult<_>>()?;
        let uids = ids(&[1, 2, 3, 4])?;
        let kept = insert_uids_top_k_by_score(Vec::new(), &uids, 2, sim, |uid| {
            Ok::<_, Infallible>(by_uid[&uid])
        })?;
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
    Ok(())
}

/// No system ID stands for Go's `notAUid`, so of Go's case only the repeats remain to drop.
#[test]
fn test_insert_uids_top_k_drops_repeats() -> TestResult {
    let sim = SimilarityType::from_name("euclidean");
    let score = |u: SystemId| Ok::<_, Infallible>(number(u));
    let kept = insert_uids_top_k_by_score(ids(&[4, 4, 4, 2])?, &ids(&[3, 2, 1])?, 3, sim, score)?;
    // The edges are cut to `keep` first, [4, 4, 4] keeping 4 alone; 2 then counts as new.
    assert_eq!(nums(&kept), vec![1, 2, 3]);
    let by_score = insert_uids_top_k_by_score(ids(&[4, 9])?, &ids(&[5])?, 3, sim, score)?;
    assert_eq!(nums(&by_score), vec![4, 5, 9], "a new ID goes in by score");
    // Unsorted edges are bisected as Go's sort.Search bisects them.
    // A new ID the edges hold, or listed twice, goes in once.
    let once = insert_uids_top_k_by_score(ids(&[4, 9])?, &ids(&[9, 5, 5])?, 4, sim, score)?;
    assert_eq!(nums(&once), vec![4, 5, 9]);
    let bisected = insert_uids_top_k_by_score(ids(&[9, 4])?, &ids(&[5])?, 3, sim, score)?;
    assert_eq!(nums(&bisected), vec![9, 4, 5]);
    assert!(insert_uids_top_k_by_score(ids(&[1])?, &ids(&[2])?, 0, sim, score)?.is_empty());
    assert_eq!(
        insert_uids_top_k_by_score(ids(&[1])?, &ids(&[2])?, 3, sim, |_| Err::<f64, _>(
            "score failed"
        )),
        Err("score failed")
    );
    // A similarity keeps the highest first; a new ID that ties goes after the IDs it does not beat, and the list is
    // cut to `keep` after each insertion.
    let dot = SimilarityType::from_name("dotproduct");
    let by_similarity =
        insert_uids_top_k_by_score(ids(&[9, 5, 2])?, &ids(&[7, 6, 1])?, 4, dot, |u| {
            Ok::<_, Infallible>(match u.sequence().get() {
                7 => 5.0,
                6 => 9.5,
                other => other as f64,
            })
        })?;
    assert_eq!(nums(&by_similarity), vec![6, 9, 5, 7]);
    Ok(())
}

/// The dead list's text of `ns`, written by hand from its layout: a JSON array of strings, each the ID's 32 lowercase
/// hexadecimal digits, type 1 (`0001`), the sequence as 16 digits, bucket 0 and the reserved bits (`000000000000`).
fn dead_text(ns: &[u64]) -> String {
    let listed: Vec<String> = ns
        .iter()
        .map(|n| format!("\"0001{n:016x}000000000000\""))
        .collect();
    format!("[{}]", listed.join(","))
}

#[test]
fn test_parse_edges_reads_a_json_array_of_hexadecimal_ids() -> TestResult {
    for (input, expected) in [
        ("", vec![]),
        (" ", vec![]),
        ("\n\t", vec![]),
        ("[]", vec![]),
        ("[ ]", vec![]),
        ("[\"00010000000000000001000000000000\"]", vec![1]),
        (
            "[\"00010000000000000001000000000000\",\"00010000000000000002000000000000\"]",
            vec![1, 2],
        ),
        (
            "  [ \"00010000000000000004000000000000\" ,\n\t\"0001ffffffffffffffff000000000000\" ]  ",
            vec![4, u64::MAX],
        ),
    ] {
        assert_eq!(nums(&parse_edges(input)?), expected, "{input:?}");
    }
    assert_eq!(
        nums(&parse_edges(&dead_text(&[3, 15, 30]))?),
        vec![3, 15, 30]
    );
    // A bucket and the largest type are held too: type 0xffff, sequence 1, bucket 0xabcdef.
    let other = parse_edges("[\"ffff0000000000000001abcdef000000\"]")?;
    assert_eq!(
        other.first().map(|id| id.to_u128()),
        Some(0xffff_0000_0000_0000_0001_abcd_ef00_0000)
    );
    Ok(())
}

#[test]
fn test_parse_edges_refuses_what_is_no_array_of_valid_ids() -> TestResult {
    let one = "\"00010000000000000001000000000000\"";
    for input in [
        // Not the array: unbracketed, unclosed, nested, null, a bracket too many.
        one.to_string(),
        format!("[{one}"),
        format!("{one}]"),
        format!("[[{one}]]"),
        "null".to_string(),
        format!("[{one}]]"),
        // Not its elements: unquoted, decimal, empty, the commas wrong, separated by spaces.
        "[00010000000000000001000000000000]".to_string(),
        "[1]".to_string(),
        "[\"\"]".to_string(),
        format!("[{one},]"),
        format!("[,{one}]"),
        format!("[{one},,{one}]"),
        format!("[{one} {one}]"),
        // Not 32 lowercase hexadecimal digits: 31, 33 (twice each, once a valid ID's value with its leading zeros
        // cut or one added), uppercase, a sign, a non-digit.
        "[\"0001000000000000000100000000000\"]".to_string(),
        "[\"0010000000000000001000000000000\"]".to_string(),
        "[\"000100000000000000010000000000000\"]".to_string(),
        "[\"000010000000000000001000000000000\"]".to_string(),
        "[\"0001000000000000000A000000000000\"]".to_string(),
        "[\"+0010000000000000001000000000000\"]".to_string(),
        "[\"0001000000000000000g000000000000\"]".to_string(),
        // No valid system ID: type 0, sequence 0, a reserved bit set.
        "[\"00000000000000000001000000000000\"]".to_string(),
        "[\"00010000000000000000000000000000\"]".to_string(),
        "[\"00010000000000000001000000000001\"]".to_string(),
    ] {
        let refused = parse_edges(&input)
            .err()
            .context("the dead list is refused")?;
        assert_eq!(
            refused.to_string(),
            format!(
                "malformed dead list: expected a JSON array of 32-digit hexadecimal system IDs, found {}",
                input.trim()
            ),
            "{input:?}"
        );
    }
    Ok(())
}

#[test]
fn test_the_dead_list_is_a_json_array_of_hexadecimal_ids() -> TestResult {
    let one = add_dead_node(None, id(5)?)?;
    assert_eq!(String::from_utf8_lossy(&one), dead_text(&[5]));
    let two = add_dead_node(Some(&one), id(9)?)?;
    assert_eq!(String::from_utf8_lossy(&two), dead_text(&[5, 9]));
    assert_eq!(
        String::from_utf8_lossy(&add_dead_node(Some(&two), id(5)?)?),
        dead_text(&[5, 9, 5]),
        "an ID listed again, as Dgraph lists it"
    );
    assert_eq!(
        add_dead_node(Some(b"nope"), id(1)?)
            .err()
            .context("the error of add_dead_node(Some(b\"nope\"), ..)")?
            .to_string(),
        "malformed dead list: expected a JSON array of 32-digit hexadecimal system IDs, found nope"
    );
    assert_eq!(
        String::from_utf8_lossy(&add_dead_node(Some(b""), id(4)?)?),
        dead_text(&[4]),
        "blank is no IDs"
    );
    assert_eq!(
        nums(&parse_edges(&String::from_utf8_lossy(&two))?),
        vec![5, 9]
    );
    Ok(())
}

#[test]
fn test_the_small_helpers() -> TestResult {
    // The entry point names its node by the ID's 16 big-endian bytes: type 1, sequence 0x0102, bucket 0.
    let entry = [0, 1, 0, 0, 0, 0, 0, 0, 1, 2, 0, 0, 0, 0, 0, 0];
    assert_eq!(bytes_to_id(&entry), Some(id(0x0102)?));
    assert_eq!(
        bytes_to_id(&id(u64::MAX)?.to_be_bytes()),
        Some(id(u64::MAX)?)
    );
    for refused in [
        &entry[..15],
        &[0; 16][..],
        &[entry.as_slice(), &[0]].concat()[..],
    ] {
        assert_eq!(bytes_to_id(refused), None, "{refused:?}");
    }
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
    assert_eq!(bytes_as_float_array::<f32>(&[]), Some(vec![]));
    assert_eq!(
        bytes_as_float_array::<f32>(&1.5f32.to_le_bytes()),
        Some(vec![1.5])
    );
    assert_eq!(
        bytes_as_float_array::<f64>(&(-2.0f64).to_le_bytes()),
        Some(vec![-2.0])
    );
    Ok(())
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

/// Level `i` of `max_levels` is taken by a draw below a fifth to the power `max_levels - 1 - i`: a fifth of the
/// nodes reach each level above the one below, and a draw no threshold passes takes level 0.
#[test]
fn test_the_insert_layer_follows_the_powers_of_a_fifth() {
    assert_eq!(ultragraphdb_store_hnsw::insert_layer(1, 0.999), 0);
    for max_levels in [3_usize, 5, 16] {
        for level in 1..max_levels {
            let exponent = i32::try_from(max_levels - 1 - level).unwrap_or(i32::MAX);
            // The threshold of this level, and of the level above it.
            let (own, above) = (0.2_f64.powi(exponent), 0.2_f64.powi(exponent + 1));
            let within = f64::midpoint(own, above);
            assert_eq!(
                ultragraphdb_store_hnsw::insert_layer(max_levels, within),
                level,
                "{within} of {max_levels}"
            );
            assert_eq!(
                ultragraphdb_store_hnsw::insert_layer(max_levels, above),
                level,
                "{above} of {max_levels}"
            );
        }
        assert_eq!(ultragraphdb_store_hnsw::insert_layer(max_levels, 0.0), 0);
    }
    assert_eq!(ultragraphdb_store_hnsw::insert_layer(4, f64::NAN), 0);
}
