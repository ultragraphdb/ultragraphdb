# Group 5: the endpoint executes reads

## Task 5.1: the lifts

`group-5-lift-compute.md` and `group-5-lift-frontend.md`. The two lifts were gated apart with unchanged counts and
landed together in commit `429acbfe`. The property test's race the frontend's lift found is fixed in `e714ddaa`.

## Task 5.2: the merge (2026-11-18, base `e714ddaa`)

`ultragraphdb_common_frontend`'s code is in `ultragraphdb_common_compute` (design D1).

- **Moved by `git mv`:** 123 files, the crate's sources, tests, property tests, features and test doubles, each to the
  same path under `ultragraphdb_common_compute`.
- **Merged by hand:** `src/lib.rs`, `src/types/mod.rs`, `src/utils/mod.rs`, `tests/mod.rs`, `tests/types/mod.rs` and
  `tests/utils/mod.rs`, the only paths both crates held. No module name and no exported name was held by both.
- **Manifest and build file:** the dependencies joined, without `ultragraphdb_traits_catalog`, which nothing used; the
  `test-utils` feature, the `property` target and the eleven BDD targets brought over under the crate's name.
  `Cargo.lock` changed its dependency lists alone.
- **Dependants:** `ultragraphdb_frontend` and `ultragraphdb_frontend_tests` name `ultragraphdb_common_compute`.
- **`ultragraphdb_common_frontend`** is an empty crate until task 6.5: a manifest, a build file and a `lib.rs` holding
  its documentation line. Its five module files that the merge left behind were removed with the user's approval.
- **`AGENTS.md`:** both crates' rows and the dependency rows that named the frontend's crate.

| Target | `common_compute` before | `common_frontend` before | Merged |
| --- | --- | --- | --- |
| Unit tests | 179 | 114 | 293 |
| Property tests | none | 12 | 12 |
| BDD, eleven targets | none | 146 scenarios, 292 steps | 146 scenarios, 292 steps |

The gate: `cargo fmt --check` clean, `make build`, the Clippy aspect over `//...` and `make test` all pass; the
integration phase ran 21 of its 22 targets fresh.

Mutation runs from here on set `MUTANTS_UNCHANGED_BY` to this task's commit.
