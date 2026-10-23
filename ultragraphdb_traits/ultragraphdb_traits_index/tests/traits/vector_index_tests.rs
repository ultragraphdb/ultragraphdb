/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;

use ultragraphdb_traits_index::{
    CacheType, KeyValue, OptionalIndexSupport, OptionalSearchOptions, SearchFilter,
    SearchPathResult, VectorIndex, VectorIndexOptions, accept_all,
};

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

    fn find(&self, _: &[u8], _: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        Err("empty".to_string())
    }
}

/// An index that searches every vector, nearest by euclidean distance.
#[derive(Default)]
struct BruteForce(RefCell<Vec<(u64, Vec<f32>)>>);

impl BruteForce {
    fn nearest(
        &self,
        q: &[f32],
        max: usize,
        filter: SearchFilter<f32>,
        threshold: Option<f64>,
    ) -> Vec<u64> {
        let dist = |v: &[f32]| -> f64 {
            q.iter()
                .zip(v)
                .map(|(a, b)| f64::from(a - b).powi(2))
                .sum::<f64>()
                .sqrt()
        };
        let mut all: Vec<(f64, u64)> = self
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

    fn vector_of(&self, uid: u64) -> Result<Vec<f32>, String> {
        self.0
            .borrow()
            .iter()
            .find(|(u, _)| *u == uid)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| format!("no vector for {uid}"))
    }
}

impl OptionalIndexSupport<f32> for BruteForce {
    type Error = String;

    fn search_with_path<C: CacheType>(
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
        r.metrics_mut()
            .insert("visited".into(), self.0.borrow().len() as u64);
        Ok(r)
    }
}

impl VectorIndex<f32> for BruteForce {
    fn search<C: CacheType>(
        &self,
        _: &C,
        query: &[f32],
        max_results: usize,
        filter: SearchFilter<f32>,
    ) -> Result<Vec<u64>, Self::Error> {
        Ok(self.nearest(query, max_results, filter, None))
    }

    fn search_with_uid<C: CacheType>(
        &self,
        c: &C,
        query_uid: u64,
        max_results: usize,
        filter: SearchFilter<f32>,
    ) -> Result<Vec<u64>, Self::Error> {
        self.search(c, &self.vector_of(query_uid)?, max_results, filter)
    }

    fn insert<C: CacheType>(
        &self,
        _: &C,
        uuid: u64,
        vec: &[f32],
    ) -> Result<Vec<KeyValue>, Self::Error> {
        if self.vector_of(uuid).is_ok() {
            return Err(format!("{uuid} exists"));
        }
        self.0.borrow_mut().push((uuid, vec.to_vec()));
        let bytes = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
        Ok(vec![KeyValue::new(uuid, "vec", bytes)])
    }
}

impl OptionalSearchOptions<f32> for BruteForce {
    fn search_with_options<C: CacheType>(
        &self,
        _: &C,
        query: &[f32],
        max_results: usize,
        opts: &VectorIndexOptions<f32>,
    ) -> Result<Vec<u64>, Self::Error> {
        Ok(self.nearest(query, max_results, opts.filter(), opts.distance_threshold()))
    }

    fn search_with_uid_and_options<C: CacheType>(
        &self,
        c: &C,
        query_uid: u64,
        max_results: usize,
        opts: &VectorIndexOptions<f32>,
    ) -> Result<Vec<u64>, Self::Error> {
        self.search_with_options(c, &self.vector_of(query_uid)?, max_results, opts)
    }
}

fn odd_only(_: &[f32], _: &[f32], uid: u64) -> bool {
    uid % 2 == 1
}

fn index() -> BruteForce {
    let ix = BruteForce::default();
    for (uid, v) in [
        (1, [0.0, 0.0]),
        (2, [1.0, 0.0]),
        (3, [3.0, 0.0]),
        (4, [10.0, 0.0]),
    ] {
        ix.insert(&NoCache, uid, &v).unwrap();
    }
    ix
}

#[test]
fn test_a_vector_index_inserts_and_searches() {
    let ix = index();
    let kv = ix.insert(&NoCache, 5, &[1.0, 1.0]).unwrap();
    assert_eq!(
        (kv[0].entity(), kv[0].attr(), kv[0].value().len()),
        (5, "vec", 8)
    );
    assert!(ix.insert(&NoCache, 5, &[0.0, 0.0]).is_err());
    assert_eq!(
        ix.search(&NoCache, &[0.9, 0.0], 2, accept_all::<f32>),
        Ok(vec![2, 1])
    );
    assert_eq!(
        ix.search(&NoCache, &[0.9, 0.0], 2, odd_only),
        Ok(vec![1, 5])
    );
    assert_eq!(
        ix.search_with_uid(&NoCache, 4, 1, accept_all::<f32>),
        Ok(vec![4])
    );
    assert!(
        ix.search_with_uid(&NoCache, 9, 1, accept_all::<f32>)
            .is_err()
    );
}

#[test]
fn test_optional_supports_give_the_path_and_take_options() {
    let ix = index();
    let r = ix
        .search_with_path(&NoCache, &[2.9, 0.0], 2, accept_all::<f32>)
        .unwrap();
    assert_eq!(
        (r.neighbors(), r.path(), r.metrics()["visited"]),
        (&[3, 2][..], &[3][..], 4)
    );
    let opts = VectorIndexOptions::new(0, Some(1.5), accept_all::<f32>);
    assert_eq!(
        ix.search_with_options(&NoCache, &[0.0, 0.0], 9, &opts),
        Ok(vec![1, 2])
    );
    let opts = VectorIndexOptions::new(0, None, odd_only);
    assert_eq!(
        ix.search_with_uid_and_options(&NoCache, 2, 2, &opts),
        Ok(vec![1, 3])
    );
}
