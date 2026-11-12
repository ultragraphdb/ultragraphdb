/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Oracle.Check

/-!
# `apg_oracle`

`lake exe apg_oracle <dir>` writes, for each schema of the corpus, two files per instance set (`SCHEMA-APG.md` section
19, `openspec/notes/archive/prepare-apg-lean.md` section 1), and `<dir>/MANIFEST` naming the Lean commit that wrote them
and every file:

- `<dir>/<schema>/<n>.write.nq`, the set's write: `INSERT DATA` of its quads, the engine minting every blank node's IRI,
  creates the set's elements. It is `R(G)` without the triples the view derives or the engine writes: an element's
  `rdf:type` of its concrete type alone, no `asserted` or `live`, and a `@symmetric` triple once (`exportWrite`). It
  creates every element live.
- `<dir>/<schema>/<n>.retract`, for a set that retracts any element: the IRIs the client retracts after the write, one
  per line in the order applied, each by an Operation of kind 6 in one later transaction
  (`openspec/changes/stage-6-6-ugs-data-definition/design.md`, decision 4). An element the engine retracts by itself, by
  a participant's `@on_retract(cascade)` or `restrict open`, is not listed (`clientRetractions`).
- `<dir>/<schema>/<n>.nq`, the set's view `R(G)`, which the engine must return after the write and the retractions,
  the instants of `asserted` aside.

Every set passes `Corpus.failures` before its files are written; a failure writes nothing and exits 1.
-/

namespace ApgModel.Oracle

/-- The instance sets written per schema. -/
def setsPerSchema : Nat := 4

/-- The Lean commit, `-dirty` when the working tree differs from it. -/
def leanCommit : IO String := do
  let head ← IO.Process.output { cmd := "git", args := #["rev-parse", "HEAD"] }
  let status ← IO.Process.output { cmd := "git", args := #["status", "--porcelain", "--", "."] }
  pure (head.stdout.trim ++ (if status.stdout.trim.isEmpty then "" else "-dirty"))

/-- Check every set, then write every file. -/
def run (dir : System.FilePath) : IO UInt32 := do
  for C in corpora do
    for n in List.range setsPerSchema do
      let fs := C.failures n
      unless fs.isEmpty do
        IO.eprintln s!"{C.name} set {n}: {fs}"
        return 1
  let commit ← leanCommit
  let mut files : List String := []
  for C in corpora do
    IO.FS.createDirAll (dir / C.name)
    for n in List.range setsPerSchema do
      let G := C.instances n
      let view := s!"{C.name}/{n}.nq"
      let write := s!"{C.name}/{n}.write.nq"
      IO.FS.writeFile (dir / view) (String.join ((exportView C.catalog C.view C.bases G).map (· ++ "\n")))
      IO.FS.writeFile (dir / write) (String.join ((exportWrite C.catalog C.view C.bases G).map (· ++ "\n")))
      files := files ++ [view, write]
      let retracted := (clientRetractions C.catalog G).filterMap (naming C.catalog C.bases G)
      unless retracted.isEmpty do
        let retract := s!"{C.name}/{n}.retract"
        IO.FS.writeFile (dir / retract) (String.join (retracted.map (· ++ "\n")))
        files := files ++ [retract]
  IO.FS.writeFile (dir / "MANIFEST")
    (String.join ((s!"lean-commit {commit}" :: files).map (· ++ "\n")))
  return 0

end ApgModel.Oracle

/-- `apg_oracle <dir>`. -/
def main (args : List String) : IO UInt32 := do
  match args with
  | [dir] => ApgModel.Oracle.run dir
  | _ =>
    IO.eprintln "usage: apg_oracle <dir>"
    return 2
