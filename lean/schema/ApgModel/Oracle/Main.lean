/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Oracle.Check

/-!
# `apg_oracle`

`lake exe apg_oracle <dir>` writes, for each schema of the corpus, its instance sets' `R(G)` as N-Quads, one file per
set at `<dir>/<schema>/<n>.nq`, and `<dir>/MANIFEST` naming the Lean commit that wrote them (`SCHEMA-APG.md` section 19,
`openspec/notes/archive/prepare-apg-lean.md` section 1). A file is at once the set's write, `INSERT DATA` of its quads
with the engine minting every blank node's IRI, and the view the engine must return. Every set passes `Corpus.failures`
before its file is written; a failure writes nothing and exits 1.
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
      let path := s!"{C.name}/{n}.nq"
      let lines := exportView C.catalog C.view C.bases (C.instances n)
      IO.FS.writeFile (dir / path) (String.join (lines.map (· ++ "\n")))
      files := files ++ [path]
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
