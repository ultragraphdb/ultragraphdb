# Stage 5.8, group 1: the wire, `ultragraphdb_protocol_wire`

Base commit `fe0db0a4`. This file records task 1.1, the Ultra Style lift, which is a commit of its own, and why the
group stopped after it. Tasks 1.2 to 1.6 are not started: no surface, no test, no golden frame, no fuzz target and no
mutation run exists for them yet.

## Task 1.1: the audit against `openspec/protocols/ULTRA_STYLE.md`

Scope: the crate-level rules over all of `src/` (24,791 lines, 1,486 functions at the base), and by review the files
this group changes: `types/command`, `types/subscribe`, `types/subscribe_view`, `types/close_request`,
`types/close_request_view`, `types/catalog`, `types/catalog_view`, `types/catalog_frames`, `types/challenge`,
`types/challenge_view`, `types/verify`, `types/verify_view`, `traits/`, and the helpers the new bodies read through
(`utils/body.rs`, `utils/command_header.rs`, `utils/fields.rs`, `utils/le.rs`, `utils/continuation.rs`,
`utils/write_sections.rs`, `types/term_table_view`). Every `file:line` below is as at `fe0db0a4`: the lift's
insertions moved the lines after them.

How it was measured. Clippy ran under Bazel, through the aspect, since `bazel build` alone runs no lint:

```
bazel build --aspects=@rules_rust//rust:defs.bzl%rust_clippy_aspect --output_groups=clippy_checks \
  //ultragraphdb_protocol/ultragraphdb_protocol_wire:all
```

It takes the workspace's lint table (`aquery` shows each `--deny=clippy::…` of `[workspace.lints]` on the action) and
was shown to bite: a probe function holding an `unwrap` and an unchecked addition failed it, and was removed again. The
lints of Ultra Style's table that the workspace table does not hold were added for the audit with
`--@rules_rust//rust/settings:clippy_flag=-D…`: `clippy::indexing_slicing`, `clippy::cast_possible_truncation`, and
the two that measure a rule the table states in words, `clippy::must_use_candidate` (*Types*, "`#[must_use]`") and
`clippy::missing_errors_doc` (*Comments and documentation*, "`# Errors` for each error it returns"), with
`clippy::missing_panics_doc`.

One limit of the measure: the aspect reads rules_rust's own `.clippy.toml`, not the repository's, because the root
`BUILD.bazel` does not export `clippy.toml`. `too-many-lines-threshold = 70` therefore did not apply, and rule 4 was
measured by a script counting each function's body lines, comments and blank lines left out.

| Rule | Result | Evidence |
|---|---|---|
| 1, acyclic control flow | conforms | no `dyn`, no stored callback, no recursion: a triple term's nesting is computed over the table's earlier entries in one pass and refused past `MAX_TERM_DEPTH` (`types/term_table_view/mod.rs:77-113`, `types/term_table/mod.rs:71`) |
| 2, bounds | conforms | three `while` loops, each stating the progress that ends it: `types/object_records.rs:60`, `types/key_values/mod.rs:55` (8 bytes or more a turn), `types/statement_cache/mod.rs:105` (one statement a turn); `utils/continuation.rs:157` counts its frames against the answer's bytes; `CatalogFrames` and `DescribeFrames` stop at `CATALOG_BYTES_MAX`; `clippy::infinite_iter` and `maybe_infinite_iter` clean |
| 3, planes | **gap, closed in part** | no module stated a plane (0 of 293 files). `src/lib.rs` now states it, as `ultragraphdb_cluster_partition` does. Open: the read path allocates per frame (see *Left open*, 1) |
| 4, 70 lines | conforms | longest bodies 70 (`utils/changes.rs:358`, `check`) and 69 (`types/quad_pattern/mod.rs:104`, `read`); 0 of 1,486 above 70 |
| 5, invariants | conforms | no run-time `assert!` under `src/`; 42 compile-time checks (`const _: () = assert!(…)`) of sizes and constants. The crate holds no state whose violation it could detect: a view is a function of its frame |
| 6, smallest scope | conforms | `clippy::shadow_unrelated` clean |
| 7, results and panics | conforms | no `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` or `unreachable!` under `src/`; `clippy::indexing_slicing` clean (the 15 index expressions left are constant indices into fixed arrays or `as_chunks` pairs, checked at compile time); `#![deny(clippy::arithmetic_side_effects)]` at `src/lib.rs` clean. Two `.ok()` drop an error on purpose, each with its reason, known from stage 5.5: `types/term_table_view/mod.rs:318`, `types/error_reply/error_reply_body_builder.rs:69` |
| 7, arithmetic on peer input | conforms | one unchecked addition on a peer's value, `types/entries_sequence.rs:98` (`view.through() as usize + 1`), cannot overflow: `check_range`, called before it, has refused a `through` at or past `bounds.len()` (`:177`). Its comment states the cast and not this bound; the code is unchanged |
| 8, no macros, `cfg` | conforms | no `macro_rules!`, no `#[cfg]` |
| 9, ownership | conforms | no `unsafe`, `Arc`, `Rc`, `RefCell`, `Cell`, `Mutex` or atomic; 102 `Copy` derives, 38 types asserted at most 64 bytes at compile time (stage 5.5, owner decisions 43 to 46). Of this group's types: `Subscribe` 8 bytes, `SubscribeView` 8, `CloseRequest` 16, `CloseRequestView` 16, `CatalogView`, `ChallengeView` and `VerifyView` asserted |
| 10, zero warnings | conforms | the aspect is clean over the library, and with the workspace table over the unit tests and the BDD target; no `#[allow]`, no `#[expect]`, no `Exception (Ultra rule` |
| Types: explicit widths, casts | conforms | 198 `as` casts, none narrowing (`clippy::cast_possible_truncation` clean); the widenings between `u32`, `usize` and `u64` rest on `utils/constants.rs:8-11`, which makes the width the compiler's to check (`usize::BITS` 32 to 64) |
| Types: exhaustive matches | conforms | `clippy::wildcard_enum_match_arm` clean; by the lint, none of the 33 `_ =>` arms is over an enum |
| Types: `#[must_use]` | **gap, closed** | 374 public functions returning a value that is no `Result` carried none, in 82 files (list below) |
| Comments: `# Errors` | **gap, closed** | 20 public functions returning a `Result` had no `# Errors` section (list below) |
| Naming | gap, left open | see *Left open*, 2 |
| Division | gap, left open | see *Left open*, 3 |

### The lift

Large by the group's measure: 86 files, 492 lines added and 36 removed, none changing what the crate does. So the
group made the lift alone and stopped (tasks.md, 1.1: "in its own commit").

1. `src/lib.rs`: the crate's plane stated in its documentation.
2. `#[must_use]` on the 374 functions `clippy::must_use_candidate` names. Lines as at `fe0db0a4`:

- `errors/statement_error/mod.rs`: 121, 134
- `types/action.rs`: 48, 57
- `types/authenticate/mod.rs`: 49, 57, 62, 67, 72, 77, 82
- `types/authenticate_view/mod.rs`: 27, 32, 37, 42, 47, 52
- `types/begin/mod.rs`: 39, 72, 77, 82
- `types/begin_view/mod.rs`: 28, 33, 38
- `types/bind/mod.rs`: 83
- `types/bind_view/mod.rs`: 42
- `types/catalog/mod.rs`: 39, 51, 62
- `types/catalog_announcement.rs`: 19, 24
- `types/catalog_view/mod.rs`: 39
- `types/chunk_layout/mod.rs`: 26, 64, 69, 74, 83, 88
- `types/chunked_body/mod.rs`: 74, 79
- `types/column_type.rs`: 58, 68
- `types/command/mod.rs`: 162
- `types/commit/mod.rs`: 82, 87, 98
- `types/commit_view/mod.rs`: 35, 40, 51, 56
- `types/compute_read/mod.rs`: 181
- `types/compute_read_view/mod.rs`: 68
- `types/conditional_write/mod.rs`: 92
- `types/conditional_write_view/mod.rs`: 47
- `types/conflict_kind.rs`: 44, 53
- `types/credit/mod.rs`: 22, 27
- `types/credit_view/mod.rs`: 19
- `types/declared_entity_type.rs`: 25, 30, 35
- `types/declared_predicate.rs`: 34, 39, 44, 57, 62, 70, 87, 92, 97, 102, 107, 112, 117, 122, 127
- `types/declared_relation.rs`: 23, 28
- `types/default_entry/mod.rs`: 46
- `types/default_entry_view/mod.rs`: 25
- `types/describe/mod.rs`: 57, 72, 77, 82, 87, 92, 97, 102, 107, 112, 118, 124, 130, 136, 142, 148, 155
- `types/describe_view/mod.rs`: 34, 39, 44, 70
- `types/direct_write/mod.rs`: 43, 89
- `types/direct_write_view/mod.rs`: 42
- `types/drop_command/mod.rs`: 66, 71, 85, 91, 96, 101, 125, 130
- `types/drop_command_view/mod.rs`: 27, 32, 37, 54, 59
- `types/drop_kind.rs`: 36, 45, 52
- `types/error_code/mod.rs`: 154, 164, 174
- `types/error_reply/mod.rs`: 51, 57, 62, 68, 86, 97, 105, 111
- `types/error_reply_view/mod.rs`: 23, 28, 33, 39
- `types/execute/mod.rs`: 41, 60, 65, 70, 75, 80, 85
- `types/execute_view/mod.rs`: 23, 28, 33, 38, 43, 48
- `types/frame_view/mod.rs`: 70, 75, 80, 85
- `types/header/header_encode.rs`: 22
- `types/header/mod.rs`: 50, 66, 73, 79, 85, 91, 97, 103, 109, 115, 121, 126, 131, 136, 141, 146, 151, 156, 161, 166
- `types/header_view/mod.rs`: 62, 67, 72, 77, 82, 87, 92, 97, 102, 107, 112, 117, 122, 127
- `types/hello/mod.rs`: 49, 54, 59, 64, 69
- `types/hello_view/mod.rs`: 23, 28, 33, 38, 43
- `types/key_values/mod.rs`: 76, 81
- `types/link.rs`: 17, 26, 35, 40, 45
- `types/operation/mod.rs`: 33, 38, 43
- `types/operation_result.rs`: 33, 60
- `types/operation_view/mod.rs`: 23, 28, 33, 38, 43, 48
- `types/placement/mod.rs`: 106
- `types/placement_view/mod.rs`: 76
- `types/prepare/mod.rs`: 64, 130, 135, 140, 151, 174, 180
- `types/prepare_view/mod.rs`: 37, 42, 47, 58, 75, 81
- `types/quad_pattern/mod.rs`: 27, 42, 47, 52, 57, 62, 67, 75
- `types/read/mod.rs`: 41, 58, 63, 68, 73, 78
- `types/read_view/mod.rs`: 23, 28, 33, 38, 43
- `types/record_entry.rs`: 18, 23, 28
- `types/reference/mod.rs`: 109
- `types/reference_view/mod.rs`: 29
- `types/report/mod.rs`: 41, 76, 96
- `types/report_view/mod.rs`: 39
- `types/result_batch/mod.rs`: 85, 212, 218, 223, 228, 234, 246, 288
- `types/result_term.rs`: 41, 115, 121, 130, 135
- `types/result_term_view.rs`: 53, 60, 75, 80, 85
- `types/result_view/mod.rs`: 37, 42, 48, 60, 65, 95
- `types/round_body/mod.rs`: 88, 100, 105
- `types/round_header/mod.rs`: 47, 98, 117, 122, 127, 132, 137
- `types/round_view/mod.rs`: 30, 35, 40, 51, 63
- `types/statement_cache/mod.rs`: 43, 194, 199, 204
- `types/tenant_run.rs`: 21, 26, 37
- `types/term_table/mod.rs`: 62, 131, 136, 141, 146
- `types/term_table_view/mod.rs`: 268, 273, 278, 311
- `types/welcome/mod.rs`: 71, 77, 83, 89, 95, 111, 116, 121, 126, 131, 136, 141, 146, 151, 156
- `types/welcome_view/mod.rs`: 27, 32, 37, 42, 47, 52, 57, 62, 67, 72
- `types/write/mod.rs`: 36, 41, 46
- `types/write_set_record/mod.rs`: 85, 114, 122, 128, 222, 233, 244, 249, 284, 289, 294, 299, 304, 315
- `types/write_set_record_view/mod.rs`: 241, 252, 263, 274, 279, 326, 331, 336, 341, 346, 351
- `types/write_view/mod.rs`: 25, 30, 35, 40, 45
- `utils/checksum.rs`: 8, 13

3. A `# Errors` section on 20 functions, the refusals a summary already listed moved under it and none invented:
   `traits/body_builder.rs:33` (`frame`); `traits/body_view.rs:24` (`from_frame`), `:28` (`to_body`);
   `types/chunk_layout/mod.rs:37` (`of_body`); `types/chunked_body/mod.rs:30` (`verify`); `types/command/mod.rs:214`
   (`from_number`); `types/frame_view/mod.rs:28` (`validate`), `:39` (`validate_client`);
   `types/header/header_encode.rs:51` (`stamp_sequence`), `:112` (`encode_frame`);
   `types/header_view/header_view_verify.rs:21` (`verify`), `:27` (`verify_client`); `types/header_view/mod.rs:37`
   (`new`); `types/prepare_view/mod.rs:93` (`check_request`), `:109` (`check_reply`); `types/result_batch/mod.rs:107`
   (`push_row`); `types/statement_cache/mod.rs:55` (`prepare`), `:125` (`execute`); `types/term_table/mod.rs:71`
   (`intern`); `types/welcome/mod.rs:163` (`answer`).

### Evidence

| | Before, at `fe0db0a4` | After the lift |
|---|---|---|
| `:ultragraphdb_protocol_wire_tests` | 829 passed | 829 passed |
| `:bdd` | 62 scenarios, 124 steps passed | 62 scenarios, 124 steps passed |
| Clippy aspect, the workspace table | clean | clean |
| `must_use_candidate`, `missing_errors_doc` | 374 and 20 | 0 and 0 |
| `indexing_slicing`, `cast_possible_truncation`, `missing_panics_doc` | 0, 0, 0 | 0, 0, 0 |
| `cargo fmt -p ultragraphdb_protocol_wire -- --check` | | unchanged |
| `make build` (`bazel build //...`) | | passes: no crate discards a value the 374 attributes now guard |

Not run for the lift: `bazel test //... --test_tag_filters=-integration_test`. The lift adds attributes and
documentation, and `make build` compiles every test target of the workspace against them.

### Left open

1. **Rule 3, memory allocated at startup.** The read path allocates per frame: `TermTableView::read`
   builds its depths and its duplicate index (`types/term_table_view/mod.rs:218`), `utils/body.rs:88` returns a
   section's u32s as a `Vec`, `utils/changes.rs` and `utils/commit_operation_layout.rs` collect records, and every
   builder returns a new `Vec` (`traits/body_builder.rs`, `body` and `frame`). Each is bounded by its frame, 1 MiB.
   Closing it means views and builders taking their buffers from the caller, which changes the two traits and every
   caller in the workspace. Stage 5.5 left the same item open (its `notes/group-12.md`, "the planes of rule 3").
2. **Naming, "units and qualifiers last".** Six public constants put the qualifier first:
   `utils/constants.rs:20` `MAX_FRAME_SIZE`, `:24` `MAX_FRAME_FLOOR`, `:27` `MAX_FRAME_SIZE_U32`, `:31`
   `MAX_FRAME_FLOOR_U32`, `:69` `MAX_EPOCH`, `:84` `MAX_TERM_DEPTH`. Other crates name them, so renaming is not this
   group's (design D2).
3. **Division states its rounding.** 19 divisions of a section's length by its record size do not say they are exact:
   `types/object_record.rs:38`, `types/closed_view/mod.rs:28`, `types/term_table_view/mod.rs:269`,
   `types/write_view/mod.rs:36`, `types/scan_view/mod.rs:75`, `types/scan_view/scan_view_body_view.rs:189`,
   `types/install_view/mod.rs:47`, `types/join_view/join_view_body_view.rs:56`, `types/commit_view/mod.rs:52`,
   `types/reference_view/mod.rs:42`, `types/write_set_record_view/mod.rs:304`, `:316`, `:347`,
   `types/warnings/mod.rs:70`, `types/describe_view/mod.rs:71`, `:77`,
   `utils/commit_operation_layout.rs:293`, `utils/write_sections.rs:59`. Each divides by a nonzero constant a length
   its view's constructor refused unless it was a whole multiple (`SectionLength`), so none rounds. No lint measures
   the rule; the sites are unchanged.
4. **The repository's `clippy.toml` under Bazel.** The aspect cannot read it until the root `BUILD.bazel` exports it.
   Not this crate's to change.

## Why the group stopped after 1.1

Two reasons, either sufficient.

1. The lift is large, and the tasks give it a commit of its own.
2. **Phase 1 cannot land inside this crate alone.** The rows for 534 to 536, 538 and 539 are five new variants of
   `Command`. Three `match`es in two other crates name every variant with no wildcard, as
   `clippy::wildcard_enum_match_arm` has them do, and stop compiling the moment a variant is added:
   - `ultragraphdb_protocol/ultragraphdb_protocol_transport/src/types/server_connection/request_kind.rs:35`,
     `RequestKind::of`: the five join the arm ending `| Command::Held => Self::Open`, as Catalog and CloseRequest
     are there.
   - `ultragraphdb_protocol/ultragraphdb_protocol_transport/src/types/client_authentication/mod.rs:66`,
     `ClientAuthentication::receive`: the five join the arm that refuses with `unexpected_command`.
   - `ultragraphdb_client/ultragraphdb_client_core/src/utils_test/test_server/test_server_requests.rs:62`,
     `TestServer`'s requests: the five join the role commands' empty arm.

   Fifteen patterns, each in an arm that exists, and no other line. tasks.md 1.3 and design D2 have no caller change
   in this group, so this is the user's to decide before phase 1 starts.

## Found wanting

A weakness in a specification is fixed in the specification first. None is decided here; each is open, routed to the
user. Items 1 and 2 of `notes/corner-cases.md` stay open as they are.

1. **tasks.md 1.3 against the command table.** "which keep their signatures so no caller changes in this group"
   (tasks.md, 1.3; design D2, "The new settings arrive first and the old leave last") against wire-protocol, "The
   system group's commands are bodies of the protocol", and `PROTOCOL.md` §3: the five rows are new `Command`
   variants, and three exhaustive matches outside the crate must name them (above).
2. **A builder that refuses, against the builders in use.** wire-protocol, the same requirement: "a builder SHALL
   refuse to build what its decoder refuses", which law L1.23 states for the seven bodies, Challenge and Verify among
   them. `BodyBuilder`'s contract says the opposite (`src/traits/body_builder.rs:15-18`: "A builder is permissive …
   the receiving command's view, the one that validates, refuses it"), and tasks.md 1.3 keeps `Challenge::with_salt`
   and `Verify::with_verified` infallible. The one place left to refuse is `BodyBuilder::frame`, as `ErrorReply`'s
   does. Neither text says where a builder refuses.
3. **A Catalog request's `epoch` past 2⁴² − 1.** `PROTOCOL.md` §13.7, Order: "in a reply, `version` at least 1
   (`FieldValue` `"version"`) and `epoch` at most 2⁴² − 1 (`FieldValue` `"epoch"`)" bounds a reply's `epoch` alone.
   §1, *Epoch*: "a receiver refuses any other value as `FieldValue` naming the field". A request of `version` 0 with
   `epoch` 2⁴² is refused by §1, at a place in the order §13.7 does not give. Case C1.100 and law L1.13 draw no such
   request.
4. **A Defaults frame breaking the first's command header.** `PROTOCOL.md` §13.28: "The requester refuses a frame
   breaking the first's command header with `FieldValue` (`"defaults"`)" names no value, where §13.7's code names the
   frames taken, and does not say whether the frame's own refusal comes first: a second frame of `position` 0 holding
   an entry breaks both the first's header (`FieldValue` `"defaults"`) and a request's rule (`CountMismatch`
   `"defaults"`). Case C1.68 names the field alone.
