/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::collections::BTreeMap;

use ultragraphdb_traits_index::{
    AllowedOptions, CacheType, IndexFactory, KeyValue, OptionalIndexSupport, Options, SearchFilter,
    SearchPathResult, Txn, VectorIndex,
};

/// An index that holds nothing and finds nothing, named by its factory.
#[derive(Debug, Clone, PartialEq)]
struct Empty {
    name: String,
    float_bits: usize,
}

impl OptionalIndexSupport<f64> for Empty {
    type Error = String;

    fn search_with_path<C: CacheType<Error = Self::Error>>(
        &self,
        _: &C,
        _: &[f64],
        _: usize,
        _: SearchFilter<f64>,
    ) -> Result<SearchPathResult, Self::Error> {
        Err("unsupported".to_string())
    }
}

impl VectorIndex<f64> for Empty {
    fn search<C: CacheType<Error = Self::Error>>(
        &self,
        _: &C,
        _: &[f64],
        _: usize,
        _: SearchFilter<f64>,
    ) -> Result<Vec<u64>, Self::Error> {
        Ok(Vec::new())
    }

    fn search_with_uid<C: CacheType<Error = Self::Error>>(
        &self,
        _: &C,
        _: u64,
        _: usize,
        _: SearchFilter<f64>,
    ) -> Result<Vec<u64>, Self::Error> {
        Ok(Vec::new())
    }

    fn insert<X: Txn<Error = Self::Error>>(
        &self,
        _: &X,
        _: u64,
        _: &[f64],
        _: &mut impl FnMut() -> f64,
    ) -> Result<Vec<KeyValue>, Self::Error> {
        Ok(Vec::new())
    }
}

/// A factory keeping its indexes by name.
#[derive(Default)]
struct Factory(RefCell<BTreeMap<String, Empty>>);

impl IndexFactory<f64> for Factory {
    type Index = Empty;
    type Name = str;
    type Error = String;

    fn name(&self) -> &str {
        "empty"
    }

    fn get_options(&self, o: &Options) -> String {
        if o.specifies("k") {
            "(k)".into()
        } else {
            String::new()
        }
    }

    fn allowed_options(&self) -> AllowedOptions {
        let mut ao = AllowedOptions::new();
        ao.add_int_option("k");
        ao
    }

    fn create(
        &self,
        name: &str,
        _: &Options,
        float_bits: usize,
    ) -> Result<Self::Index, Self::Error> {
        if self.0.borrow().contains_key(name) {
            return Err(format!("{name} exists"));
        }
        let ix = Empty {
            name: name.into(),
            float_bits,
        };
        self.0.borrow_mut().insert(name.into(), ix.clone());
        Ok(ix)
    }

    fn find(&self, name: &str) -> Result<Option<Self::Index>, Self::Error> {
        Ok(self.0.borrow().get(name).cloned())
    }

    fn remove(&self, name: &str) -> Result<(), Self::Error> {
        self.0.borrow_mut().remove(name);
        Ok(())
    }

    fn create_or_replace(
        &self,
        name: &str,
        o: &Options,
        float_bits: usize,
    ) -> Result<Self::Index, Self::Error> {
        self.remove(name)?;
        self.create(name, o, float_bits)
    }
}

#[test]
fn test_a_factory_creates_finds_and_removes_indexes() {
    let f = Factory::default();
    let o = Options::new();
    assert_eq!(f.name(), "empty");
    assert_eq!(f.get_options(&o), "");
    assert_eq!(f.allowed_options().get_option_list(), vec!["k"]);
    let ix = f.create("a", &o, 64).unwrap();
    assert_eq!((ix.name.as_str(), ix.float_bits), ("a", 64));
    assert!(f.create("a", &o, 64).is_err());
    assert_eq!(f.find("a"), Ok(Some(ix.clone())));
    assert_eq!(
        f.create_or_replace("a", &o, 32).map(|i| i.float_bits),
        Ok(32)
    );
    f.remove("a").unwrap();
    assert_eq!(f.find("a"), Ok(None));
    assert!(
        ix.search_with_path(&NoCache, &[], 1, |_, _, _| true)
            .is_err()
    );
    assert_eq!(ix.search(&NoCache, &[1.0], 1, |_, _, _| true), Ok(vec![]));
    assert_eq!(
        ix.search_with_uid(&NoCache, 1, 1, |_, _, _| true),
        Ok(vec![])
    );
    assert_eq!(ix.insert(&NoTxn, 1, &[1.0], &mut || 0.0), Ok(vec![]));
}

struct NoCache;

/// A transaction that holds nothing.
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

    fn find(&self, _: &[u8], _: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        Err("empty".to_string())
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
