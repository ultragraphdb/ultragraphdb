/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/

-- The repository's Lean proofs, built with `lake build` outside Bazel, as MGQL's are. The toolchain is pinned in
-- `lean-toolchain` to the release MGQL uses.

import Lake
open Lake DSL

package ultragraphdb where
  leanOptions := #[
    ⟨`autoImplicit, false⟩
  ]

-- MGQL, the mechanised GQL semantics: the APG model builds its GQL view as MGQL's `PropertyGraph` and checks it with
-- MGQL's `graphConformsSchema`. Pinned to one commit; an update is a deliberate change.
require mgql from git "https://github.com/marvin-hansen/mgql.git" @ "4ce4878ab928834b014fb117c1fc041805620505"

-- The stage-5 round protocol: rounds, exact validation, commit-time operations, the watermark, the log's sync, crash
-- and recovery, and sessions, with their properties proved.
@[default_target]
lean_lib RoundModel where
  roots := #[`RoundModel]

-- The stage-5.5 placement protocol (P24): partitions over bucket groups, the sequencer's closes, catalog announcements,
-- registrations, referrer sets, reverse reads, tombstones and reclamation, with rules 4, 5, 6, 9 and 11 proved.
@[default_target]
lean_lib PlacementModel where
  roots := #[`PlacementModel]

-- The algebraic property graphs of `specification/schema/SCHEMA-APG.md`: a binary core proving the paper's results, a
-- named layer over it, the catalog's schema, constraints, history, migration operations and the two views. Its sources
-- sit under `schema/`; `openspec/notes/prepare-apg-lean.md` plans it.
@[default_target]
lean_lib ApgModel where
  srcDir := "schema"
  roots := #[`ApgModel]
