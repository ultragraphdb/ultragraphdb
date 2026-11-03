/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_index::{
    CacheType, KeyValue, OptionalIndexSupport, OptionalSearchOptions, SearchFilter,
    SearchPathResult, Txn, VectorIndex, VectorIndexOptions, accept_all,
};

use crate::utils::helpers::id;

/// The IDs numbered `ns`, in their order.
fn ids(ns: &[u64]) -> TestResult<Vec<SystemId>> {
    ns.iter().map(|&n| id(n)).collect()
}

/// A cache the brute-force index never reads.
struct NoCache;

impl CacheType for NoCache {
    type Error = String;

    fn get(&self, _: &[u8]) -> Result<Vec<u8>, Self::Error> {
        Err("empty".to_string())
    }

    fn ts(&self) -> u64 {
        0
    }

    fn find(&self, _: &[u8], _: impl Fn(&[u8]) -> bool) -> Result<Option<SystemId>, Self::Error> {
        Ok(None)
    }
}

/// A transaction the brute-force index writes nothing through.
struct NoTxn;

impl Txn for NoTxn {
    type Error = String;

    fn start_ts(&self) -> u64 {
        0
    }

    fn get(&self, _: &[u8]) -> Result<Vec<u8>, Self::Error> {
        Err("empty".to_string())
    }

    fn get_with_lock_held(&self, _: &[u8]) -> Result<Vec<u8>, Self::Error> {
        Err("empty".to_string())
    }

    fn find(&self, _: &[u8], _: impl Fn(&[u8]) -> bool) -> Result<Option<SystemId>, Self::Error> {
        Ok(None)
    }

    fn add_mutation(&self, _: &[u8], _: &KeyValue) -> Result<(), Self::Error> {
        Ok(())
    }

    fn add_mutation_with_lock_held(&self, _: &[u8], _: &KeyValue) -> Result<(), Self::Error> {
        Ok(())
    }

    fn lock_key(&self, _: &[u8]) {}

    fn unlock_key(&self, _: &[u8]) {}
}

/// No draw is ever made: the brute-force index has no levels.
fn no_draw() -> f64 {
    0.0
}

/// An index that searches every vector, nearest by euclidean distance.
#[derive(Default)]
struct BruteForce(RefCell<Vec<(SystemId, Vec<f32>)>>);

impl BruteForce {
    fn nearest(
        &self,
        q: &[f32],
        max: usize,
        filter: SearchFilter<f32>,
        threshold: Option<f64>,
    ) -> Vec<SystemId> {
        let dist = |v: &[f32]| -> f64 {
            q.iter()
                .zip(v)
                .map(|(a, b)| f64::from(a - b).powi(2))
                .sum::<f64>()
                .sqrt()
        };
        let mut all: Vec<(f64, SystemId)> = self
            .0
            .borrow()
            .iter()
            .filter(|(uid, v)| filter(q, v, *uid))
            .map(|(uid, v)| (dist(v), *uid))
            .filter(|(d, _)| threshold.is_none_or(|t| *d <= t))
            .collect();
        all.sort_by(|a, b| a.0.total_cmp(&b.0));
        all.into_iter().take(max).map(|(_, uid)| uid).collect()
    }

    fn vector_of(&self, uid: SystemId) -> Result<Vec<f32>, String> {
        self.0
            .borrow()
            .iter()
            .find(|(u, _)| *u == uid)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| format!("no vector for {}", uid.sequence().get()))
    }
}

impl OptionalIndexSupport<f32> for BruteForce {
    type Error = String;

    fn search_with_path<C: CacheType<Error = Self::Error>>(
        &self,
        _: &C,
        query: &[f32],
        max_results: usize,
        filter: SearchFilter<f32>,
    ) -> Result<SearchPathResult, Self::Error> {
        let mut r = SearchPathResult::new();
        *r.neighbors_mut() = self.nearest(query, max_results, filter, None);
        let first = r.neighbors().first().copied();
        r.path_mut().extend(first);
        let _previous: Option<u64> = r
            .metrics_mut()
            .insert("visited".into(), self.0.borrow().len() as u64);
        Ok(r)
    }
}

impl VectorIndex<f32> for BruteForce {
    fn search<C: CacheType<Error = Self::Error>>(
        &self,
        _: &C,
        query: &[f32],
        max_results: usize,
        filter: SearchFilter<f32>,
    ) -> Result<Vec<SystemId>, Self::Error> {
        Ok(self.nearest(query, max_results, filter, None))
    }

    fn search_with_uid<C: CacheType<Error = Self::Error>>(
        &self,
        c: &C,
        query_uid: SystemId,
        max_results: usize,
        filter: SearchFilter<f32>,
    ) -> Result<Vec<SystemId>, Self::Error> {
        self.search(c, &self.vector_of(query_uid)?, max_results, filter)
    }

    fn insert<X: Txn<Error = Self::Error>>(
        &self,
        _: &X,
        uuid: SystemId,
        vec: &[f32],
        _: &mut impl FnMut() -> f64,
    ) -> Result<Vec<KeyValue>, Self::Error> {
        if self.vector_of(uuid).is_ok() {
            return Err(format!("{} exists", uuid.sequence().get()));
        }
        self.0.borrow_mut().push((uuid, vec.to_vec()));
        let bytes = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
        Ok(vec![KeyValue::new(uuid, "vec", bytes)])
    }
}

impl OptionalSearchOptions<f32> for BruteForce {
    fn search_with_options<C: CacheType<Error = Self::Error>>(
        &self,
        _: &C,
        query: &[f32],
        max_results: usize,
        opts: &VectorIndexOptions<f32>,
    ) -> Result<Vec<SystemId>, Self::Error> {
        Ok(self.nearest(query, max_results, opts.filter(), opts.distance_threshold()))
    }

    fn search_with_uid_and_options<C: CacheType<Error = Self::Error>>(
        &self,
        c: &C,
        query_uid: SystemId,
        max_results: usize,
        opts: &VectorIndexOptions<f32>,
    ) -> Result<Vec<SystemId>, Self::Error> {
        self.search_with_options(c, &self.vector_of(query_uid)?, max_results, opts)
    }
}

/// Keeps the nodes of odd numbers.
fn odd_only(_: &[f32], _: &[f32], uid: SystemId) -> bool {
    uid.sequence().get() % 2 == 1
}

fn index() -> TestResult<BruteForce> {
    let ix = BruteForce::default();
    for (uid, v) in [
        (1, [0.0, 0.0]),
        (2, [1.0, 0.0]),
        (3, [3.0, 0.0]),
        (4, [10.0, 0.0]),
    ] {
        let _inserted: Vec<KeyValue> = ix
            .insert(&NoTxn, id(uid)?, &v, &mut no_draw)
            .context("ix.insert(..)")?;
    }
    Ok(ix)
}

#[test]
fn test_a_vector_index_inserts_and_searches() -> TestResult {
    let ix = index()?;
    let kv = ix
        .insert(&NoTxn, id(5)?, &[1.0, 1.0], &mut no_draw)
        .context("ix.insert(..)")?;
    // 1.0f32 is 0x3F80_0000 in IEEE 754, little-endian 00 00 80 3F.
    assert_eq!(
        kv,
        vec![KeyValue::new(
            id(5)?,
            "vec",
            vec![0, 0, 0x80, 0x3f, 0, 0, 0x80, 0x3f]
        )]
    );
    assert_eq!(
        ix.insert(&NoTxn, id(5)?, &[0.0, 0.0], &mut no_draw),
        Err("5 exists".to_string())
    );
    assert_eq!(
        ix.search(&NoCache, &[0.9, 0.0], 2, accept_all::<f32>),
        Ok(ids(&[2, 1])?)
    );
    assert_eq!(
        ix.search(&NoCache, &[0.9, 0.0], 2, odd_only),
        Ok(ids(&[1, 5])?)
    );
    assert_eq!(
        ix.search_with_uid(&NoCache, id(4)?, 1, accept_all::<f32>),
        Ok(ids(&[4])?)
    );
    assert_eq!(
        ix.search_with_uid(&NoCache, id(9)?, 1, accept_all::<f32>),
        Err("no vector for 9".to_string())
    );
    Ok(())
}

/// The edge inputs of each search: no results asked for, a filter passing nothing, an empty index, and a threshold
/// that only the query's own vector meets.
#[test]
fn test_searches_at_their_edge_inputs() -> TestResult {
    let ix = index()?;
    let none = |_: &[f32], _: &[f32], _: SystemId| false;
    assert_eq!(
        ix.search(&NoCache, &[0.0, 0.0], 0, accept_all::<f32>),
        Ok(vec![])
    );
    assert_eq!(ix.search(&NoCache, &[0.0, 0.0], 9, none), Ok(vec![]));
    assert_eq!(
        BruteForce::default().search(&NoCache, &[0.0, 0.0], 9, accept_all::<f32>),
        Ok(vec![])
    );
    assert_eq!(
        ix.search_with_uid(&NoCache, id(1)?, 0, accept_all::<f32>),
        Ok(vec![])
    );
    let exact = VectorIndexOptions::new(0, Some(0.0), accept_all::<f32>);
    assert_eq!(
        ix.search_with_uid_and_options(&NoCache, id(3)?, 9, &exact),
        Ok(ids(&[3])?)
    );
    assert_eq!(
        ix.search_with_uid_and_options(&NoCache, id(9)?, 9, &exact),
        Err("no vector for 9".to_string())
    );
    let r = ix
        .search_with_path(&NoCache, &[0.0, 0.0], 0, accept_all::<f32>)
        .context("search_with_path(..)")?;
    assert_eq!((r.neighbors(), r.path()), (&[][..], &[][..]));
    Ok(())
}

/// A transaction that records what is written through it, or refuses every write.
#[derive(Default)]
struct Recording {
    refuse: bool,
    written: RefCell<Vec<(Vec<u8>, KeyValue)>>,
}

impl Txn for Recording {
    type Error = String;

    fn start_ts(&self) -> u64 {
        7
    }

    fn get(&self, _: &[u8]) -> Result<Vec<u8>, Self::Error> {
        Err("empty".to_string())
    }

    fn get_with_lock_held(&self, _: &[u8]) -> Result<Vec<u8>, Self::Error> {
        Err("empty".to_string())
    }

    fn find(&self, _: &[u8], _: impl Fn(&[u8]) -> bool) -> Result<Option<SystemId>, Self::Error> {
        Ok(None)
    }

    fn add_mutation(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error> {
        if self.refuse {
            return Err("refused".to_string());
        }
        self.written.borrow_mut().push((key.to_vec(), t.clone()));
        Ok(())
    }

    fn add_mutation_with_lock_held(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error> {
        self.add_mutation(key, t)
    }

    fn lock_key(&self, _: &[u8]) {}

    fn unlock_key(&self, _: &[u8]) {}
}

/// An index that writes each insert through its transaction at a level drawn from the caller's source, and reads
/// each search through its cache: the shape the reshaped bounds serve, where the cache's and the transaction's
/// errors are the index's own, passed on with `?`.
struct Layered;

impl OptionalIndexSupport<f32> for Layered {
    type Error = String;

    fn search_with_path<C: CacheType<Error = Self::Error>>(
        &self,
        c: &C,
        _: &[f32],
        _: usize,
        _: SearchFilter<f32>,
    ) -> Result<SearchPathResult, Self::Error> {
        let _get: Vec<u8> = c.get(b"entry")?;
        Ok(SearchPathResult::new())
    }
}

impl VectorIndex<f32> for Layered {
    fn search<C: CacheType<Error = Self::Error>>(
        &self,
        c: &C,
        _: &[f32],
        _: usize,
        _: SearchFilter<f32>,
    ) -> Result<Vec<SystemId>, Self::Error> {
        Ok(c.find(b"entry", |_| true)?.into_iter().collect())
    }

    fn search_with_uid<C: CacheType<Error = Self::Error>>(
        &self,
        c: &C,
        query_uid: SystemId,
        _: usize,
        _: SearchFilter<f32>,
    ) -> Result<Vec<SystemId>, Self::Error> {
        let _get: Vec<u8> = c.get(&query_uid.to_be_bytes())?;
        Ok(vec![query_uid])
    }

    fn insert<X: Txn<Error = Self::Error>>(
        &self,
        txn: &X,
        uuid: SystemId,
        _: &[f32],
        draw: &mut impl FnMut() -> f64,
    ) -> Result<Vec<KeyValue>, Self::Error> {
        // The level of an HNSW node, floor(-ln(u) * ml), with ml = 1 / ln(2).
        let level = (-draw().ln() / std::f64::consts::LN_2).floor() as u8;
        let kv = KeyValue::new(uuid, "level", vec![level]);
        txn.add_mutation(&uuid.to_be_bytes(), &kv)?;
        Ok(vec![kv])
    }
}

#[test]
fn test_an_insert_writes_through_its_transaction_with_the_callers_draws() -> TestResult {
    let txn = Recording::default();
    // The caller's seeded source: 0.5, 0.25, 0.125 in turn, levels 1, 2 and 3 by -ln(u) / ln(2).
    let mut next = 0.5;
    let mut draw = || {
        let u = next;
        next /= 2.0;
        u
    };
    let mut levels: Vec<Vec<KeyValue>> = Vec::new();
    for n in 1..=3 {
        levels.push(
            Layered
                .insert(&txn, id(n)?, &[0.0], &mut draw)
                .context("Layered.insert(..)")?,
        );
    }
    assert_eq!(
        levels,
        vec![
            vec![KeyValue::new(id(1)?, "level", vec![1])],
            vec![KeyValue::new(id(2)?, "level", vec![2])],
            vec![KeyValue::new(id(3)?, "level", vec![3])],
        ]
    );
    assert_eq!(draw(), 0.0625, "three draws were taken, no more");
    // Each write is under the node's ID, 16 big-endian bytes whose sequence ends at byte 9.
    assert_eq!(
        txn.written
            .borrow()
            .iter()
            .map(|(k, kv)| (k.get(9).copied(), kv.entity()))
            .collect::<Vec<_>>(),
        vec![(Some(1), id(1)?), (Some(2), id(2)?), (Some(3), id(3)?)]
    );
    let refusing = Recording {
        refuse: true,
        ..Recording::default()
    };
    assert_eq!(
        Layered.insert(&refusing, id(4)?, &[0.0], &mut || 0.5),
        Err("refused".to_string())
    );
    assert!(refusing.written.borrow().is_empty());
    Ok(())
}

#[test]
fn test_a_search_passes_on_its_caches_error() -> TestResult {
    assert_eq!(
        Layered.search_with_uid(&NoCache, id(4)?, 1, accept_all::<f32>),
        Err("empty".to_string())
    );
    assert_eq!(
        Layered
            .search_with_path(&NoCache, &[0.0], 1, accept_all::<f32>)
            .map(|r| r.neighbors().to_vec()),
        Err("empty".to_string())
    );
    // The cache finds no entry: nothing to return.
    assert_eq!(
        Layered.search(&NoCache, &[0.0], 1, accept_all::<f32>),
        Ok(Vec::new())
    );
    Ok(())
}

#[test]
fn test_optional_supports_give_the_path_and_take_options() -> TestResult {
    let ix = index()?;
    let r = ix
        .search_with_path(&NoCache, &[2.9, 0.0], 2, accept_all::<f32>)
        .context("search_with_path(..)")?;
    assert_eq!(
        (r.neighbors(), r.path(), r.metrics()["visited"]),
        (&ids(&[3, 2])?[..], &ids(&[3])?[..], 4)
    );
    let opts = VectorIndexOptions::new(0, Some(1.5), accept_all::<f32>);
    assert_eq!(
        ix.search_with_options(&NoCache, &[0.0, 0.0], 9, &opts),
        Ok(ids(&[1, 2])?)
    );
    let odd_opts = VectorIndexOptions::new(0, None, odd_only);
    assert_eq!(
        ix.search_with_uid_and_options(&NoCache, id(2)?, 2, &odd_opts),
        Ok(ids(&[1, 3])?)
    );
    Ok(())
}
