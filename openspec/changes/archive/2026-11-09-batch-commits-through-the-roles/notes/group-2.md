# Group 2: one ordered push channel

## 2.1 `Push`

`ultragraphdb_common_role` declares `Push` beside `Answer` in `traits/session.rs`, where the crate keeps the types a session's
hooks return: `Push::Frame(Vec<u8>)`, a frame, and `Push::End`, a stream's end with no frame. `Session::next_push` yields
`(u64, Push)`. `serve_connection` sends a pushed frame as before, dropping one for a stream no longer open, and ends a stream on
`Push::End` as on `Answer::Done`, through `Outbound::cancel`, which drops what the stream still holds queued for credit: the same
end `Answer::Done` gives today.

The fourteen implementors that override `next_push` yield `Push::Frame` where they yielded a frame: in `src/`, `StorageSession`,
`ComputeSession`, the frontend's `ClientSession` and `RoleSession`, and the frontend's `DoubleSession` test double; the rest are test
doubles in `ultragraphdb_common_role`, `ultragraphdb_common_compute`, `ultragraphdb_common_storage`, `ultragraphdb_compute_tests` and
`ultragraphdb_storage_tests` (whose played frontend has an `enum Push` of its own and imports the role's as `RolePush`). None yields
`End` yet; `StorageSession` does in group 3.

## 2.2 The test

`test_a_pushed_end_ends_its_stream_with_no_frame` (`ultragraphdb_common_role`, `tests/utils/serve_tests.rs`), over the
`Defaulted` double, which now takes a `Push` to push: a frame pushed to an open Scan stream leaves first; with credit for one more
frame granted and taken (a probe behind the Credit proves it), the stream's `End` is pushed and then a last Entries frame; the
answer read next is the probe's own, so nothing left on the stream after its end. Proven by a temporary edit making `serve` ignore
`Push::End`: the test failed, and passed with the edit undone. A first version passed under that edit, since the late frame had no
credit and stayed queued either way; the credit granted before the end closes that gap.

## Gate

`cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --all` applied. `bazel test //... --test_tag_filters=unit-test,bdd-test`:
115 of 116 targets pass; `ultragraphdb_common_storage_tests` fails with the seven scenarios of group 1 alone, which wait for
group 3.

A stale Cargo build of `ultragraphdb_protocol_wire` for the workspace's feature set reported `DETAIL_VALUE_CUT` and `value_cut`
missing in a workspace-wide check while the crate checked clean alone; touching `src/lib.rs` and `src/utils/constants.rs`, their
content unchanged, rebuilt it.
