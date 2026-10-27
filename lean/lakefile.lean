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

-- The stage-5 round protocol: rounds, exact validation, commit-time operations, the watermark, the log's sync, crash
-- and recovery, and sessions, with their properties proved.
@[default_target]
lean_lib RoundModel where
  roots := #[`RoundModel]
