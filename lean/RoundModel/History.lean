/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import RoundModel.Txn

/-!
# Histories, snapshots and what the commit step guarantees of each commit

A *history* is the list of committed transactions in commit order: the log's rounds, flattened. Two states are read
from it:

* `serial H` — the state the commits of `H` produce run one after the other, in order: the serial history.
* `snapshot H v` — the state at version `v`: the commits of `H` whose version is at most `v`, in order. This is the
  multiversion read a transaction reading at `v` gets.

`Admissible pre c` is what the commit step guarantees of a commit `c` decided after the commits `pre`, the
commit-step half of every theorem: its read version is at or below a version already committed (it began at the
watermark), its commit version is even and above every earlier one, no earlier commit above its read version wrote a
key its read set covers (exact validation), its logged writes are what it resolves to against the serial state of
`pre`, and its request number is one above its session's last. `Valid H` states it of every commit of `H` against
the commits before it.
-/

namespace RoundModel

/-- Run the commits of a history one after the other from `s`. -/
def serialFrom (s : State) : List Commit → State
  | [] => s
  | c :: cs => serialFrom (applyWrites s c.writes) cs

/-- The state the commits of `H` produce in commit order: the serial history. -/
def serial (H : List Commit) : State := serialFrom State.empty H

/-- The state at version `v`: the commits of `H` at or below `v`, in commit order. -/
def snapshot (H : List Commit) (v : Nat) : State :=
  serial (H.filter (fun c => decide (c.version ≤ v)))

/-- The highest commit version in `H`, `0` for the empty history. -/
def maxVersion : List Commit → Nat
  | [] => 0
  | c :: cs => max c.version (maxVersion cs)

/-- The highest request number session `s` committed in `H`, `0` when it committed none. -/
def maxReq (s : Nat) : List Commit → Nat
  | [] => 0
  | c :: cs => if c.txn.session = s then max c.txn.reqNo (maxReq s cs) else maxReq s cs

/-- The last commit of session `s` in `H`: the session table's stored reply. -/
def lastCommitOf (s : Nat) : List Commit → Option Commit
  | [] => none
  | c :: cs =>
    match lastCommitOf s cs with
    | some c' => some c'
    | none => if c.txn.session = s then some c else none

/-- What the commit step guarantees of a commit `c` decided after the commits `pre`. -/
structure Admissible (pre : List Commit) (c : Commit) : Prop where
  /-- It began at the watermark, so its snapshot is at or below a committed version. -/
  readBelow : c.txn.readVersion ≤ maxVersion pre
  /-- Its commit version is above every earlier commit version. -/
  versionAbove : maxVersion pre < c.version
  /-- Its commit version is even. -/
  versionEven : c.version % 2 = 0
  /-- Exact validation: no earlier commit above its read version wrote a key its read set covers. -/
  noConflict : ∀ c' ∈ pre, c.txn.readVersion < c'.version → ∀ w ∈ c'.writes, c.txn.reads.covers w.1 = false
  /-- Its logged writes and results are its resolution against the serial state of the earlier commits. -/
  resolved : resolve (serial pre) c.txn = .ok (c.writes, c.results)
  /-- Its request number is one above its session's last committed one. -/
  nextRequest : c.txn.reqNo = maxReq c.txn.session pre + 1

/-- Every commit of `cs` is admissible after `pre` and the commits of `cs` before it. -/
def ValidFrom (pre : List Commit) : List Commit → Prop
  | [] => True
  | c :: cs => Admissible pre c ∧ ValidFrom (pre ++ [c]) cs

/-- Every commit of `H` is admissible after the commits before it. -/
def Valid (H : List Commit) : Prop := ValidFrom [] H

/-! ## Serial states -/

theorem serialFrom_append (s : State) (A B : List Commit) :
    serialFrom s (A ++ B) = serialFrom (serialFrom s A) B := by
  induction A generalizing s with
  | nil => rfl
  | cons c cs ih => exact ih _

theorem serial_append_single (A : List Commit) (c : Commit) :
    serial (A ++ [c]) = applyWrites (serial A) c.writes := by
  simp [serial, serialFrom_append, serialFrom]

/-- Filtering out commits that do not write `k` does not change the value of `k`. -/
theorem serialFrom_filter_get (p : Commit → Bool) (k : Key) (l : List Commit)
    (h : ∀ c ∈ l, p c = false → ∀ w ∈ c.writes, w.1 ≠ k) (s₁ s₂ : State) (hs : s₁ k = s₂ k) :
    serialFrom s₁ (l.filter p) k = serialFrom s₂ l k := by
  induction l generalizing s₁ s₂ with
  | nil => exact hs
  | cons c cs ih =>
    have h' : ∀ c' ∈ cs, p c' = false → ∀ w ∈ c'.writes, w.1 ≠ k :=
      fun c' hc' => h c' (List.mem_cons_of_mem _ hc')
    rw [List.filter_cons]
    by_cases hp : p c = true
    · simp only [hp, ite_true, serialFrom]
      exact ih h' _ _ (applyWrites_congr _ _ _ _ hs)
    · simp only [hp, serialFrom]
      have hpf : p c = false := by simpa using hp
      apply ih h' _ _
      rw [applyWrites_get_of_not_writes _ _ _ (h c (List.mem_cons_self _ _) hpf)]
      exact hs

/-- Applying a history's commits twice in a row from the same state is applying them once: each logged write is an
assignment of a resolved value. -/
theorem serialFrom_idem (s : State) (cs : List Commit) :
    serialFrom (serialFrom s cs) cs = serialFrom s cs := by
  have key : ∀ (t : State), serialFrom t cs = applyWrites t (cs.foldr (fun c ws => c.writes ++ ws) []) := by
    intro t
    induction cs generalizing t with
    | nil => rfl
    | cons c cs ih => simp only [serialFrom, List.foldr, applyWrites_append, ih]
  simp only [key, applyWrites_idem]

/-! ## Versions and request numbers -/

theorem maxVersion_append (A B : List Commit) : maxVersion (A ++ B) = max (maxVersion A) (maxVersion B) := by
  induction A with
  | nil => simp [maxVersion]
  | cons c cs ih =>
    show max c.version (maxVersion (cs ++ B)) = max (max c.version (maxVersion cs)) (maxVersion B)
    rw [ih, Nat.max_assoc]

theorem le_maxVersion {c : Commit} {H : List Commit} (h : c ∈ H) : c.version ≤ maxVersion H := by
  induction H with
  | nil => cases h
  | cons d ds ih =>
    simp only [maxVersion]
    cases List.mem_cons.mp h with
    | inl he => subst he; exact Nat.le_max_left _ _
    | inr ht => exact Nat.le_trans (ih ht) (Nat.le_max_right _ _)

theorem maxVersion_le_append_left (A B : List Commit) : maxVersion A ≤ maxVersion (A ++ B) := by
  rw [maxVersion_append]; exact Nat.le_max_left _ _

theorem maxVersion_even {H : List Commit} (h : ∀ c ∈ H, c.version % 2 = 0) : maxVersion H % 2 = 0 := by
  induction H with
  | nil => rfl
  | cons c cs ih =>
    simp only [maxVersion, Nat.max_def]
    split
    · exact ih (fun c' hc' => h c' (List.mem_cons_of_mem _ hc'))
    · exact h c (List.mem_cons_self _ _)

theorem maxReq_append (s : Nat) (A B : List Commit) : maxReq s (A ++ B) = max (maxReq s A) (maxReq s B) := by
  induction A with
  | nil => simp [maxReq]
  | cons c cs ih =>
    show maxReq s (c :: (cs ++ B)) = max (maxReq s (c :: cs)) (maxReq s B)
    simp only [maxReq]
    split <;> simp [ih, Nat.max_assoc]

theorem le_maxReq {c : Commit} {H : List Commit} (h : c ∈ H) : c.txn.reqNo ≤ maxReq c.txn.session H := by
  induction H with
  | nil => cases h
  | cons d ds ih =>
    simp only [maxReq]
    cases List.mem_cons.mp h with
    | inl he => subst he; simp [Nat.le_max_left]
    | inr ht =>
      split
      · exact Nat.le_trans (ih ht) (Nat.le_max_right _ _)
      · exact ih ht

/-! ## Valid histories -/

theorem validFrom_append (P A B : List Commit) :
    ValidFrom P (A ++ B) ↔ ValidFrom P A ∧ ValidFrom (P ++ A) B := by
  induction A generalizing P with
  | nil => simp [ValidFrom]
  | cons c cs ih =>
    show (Admissible P c ∧ ValidFrom (P ++ [c]) (cs ++ B)) ↔
      (Admissible P c ∧ ValidFrom (P ++ [c]) cs) ∧ ValidFrom (P ++ c :: cs) B
    rw [ih, and_assoc]
    simp [List.append_assoc]

/-- A prefix of a valid history is valid: truncating the log keeps it valid. -/
theorem valid_prefix {A B : List Commit} (h : Valid (A ++ B)) : Valid A :=
  ((validFrom_append [] A B).mp h).1

/-- Every commit of a valid history is admissible after the commits before it. -/
theorem valid_split {pre post : List Commit} {c : Commit} (h : Valid (pre ++ c :: post)) : Admissible pre c := by
  have := ((validFrom_append [] pre (c :: post)).mp h).2
  simp only [List.nil_append, ValidFrom] at this
  exact this.1

/-- Commit versions strictly increase along a valid history, and lie above the history it continues. -/
theorem validFrom_versions {P Q : List Commit} (h : ValidFrom P Q) :
    Q.Pairwise (fun a b => a.version < b.version) ∧ ∀ d ∈ Q, maxVersion P < d.version := by
  induction Q generalizing P with
  | nil => exact ⟨List.Pairwise.nil, fun _ hd => by cases hd⟩
  | cons c cs ih =>
    obtain ⟨hc, hcs⟩ := h
    obtain ⟨hpw, hgt⟩ := ih hcs
    have hcle : c.version ≤ maxVersion (P ++ [c]) := le_maxVersion (List.mem_append_right _ (List.mem_singleton_self _))
    refine ⟨List.pairwise_cons.mpr ⟨fun d hd => Nat.lt_of_le_of_lt hcle (hgt d hd), hpw⟩, ?_⟩
    intro d hd
    cases List.mem_cons.mp hd with
    | inl he => subst he; exact hc.versionAbove
    | inr ht => exact Nat.lt_of_le_of_lt (maxVersion_le_append_left P [c]) (hgt d ht)

/-- In a valid history, commit order is version order. -/
theorem valid_versions {H : List Commit} (h : Valid H) : H.Pairwise (fun a b => a.version < b.version) :=
  (validFrom_versions h).1

/-- Every commit version of a valid history is even. -/
theorem valid_even {H : List Commit} (h : Valid H) : ∀ c ∈ H, c.version % 2 = 0 := by
  intro c hc
  obtain ⟨pre, post, rfl⟩ := List.append_of_mem hc
  exact (valid_split h).versionEven

/-- Commits after `c` in a valid history carry higher versions than `c`. -/
theorem valid_later_gt {pre post : List Commit} {c : Commit} (h : Valid (pre ++ c :: post)) :
    ∀ d ∈ post, c.version < d.version := by
  have := ((validFrom_append [] pre (c :: post)).mp h).2
  simp only [List.nil_append, ValidFrom] at this
  intro d hd
  exact Nat.lt_of_le_of_lt (le_maxVersion (List.mem_append_right _ (List.mem_singleton_self _)))
    ((validFrom_versions this.2).2 d hd)

/-- Within a session, request numbers strictly increase along a valid history. -/
theorem validFrom_sessions {P Q : List Commit} (h : ValidFrom P Q) :
    Q.Pairwise (fun a b => a.txn.session = b.txn.session → a.txn.reqNo < b.txn.reqNo) ∧
      ∀ d ∈ Q, maxReq d.txn.session P < d.txn.reqNo := by
  induction Q generalizing P with
  | nil => exact ⟨List.Pairwise.nil, fun _ hd => by cases hd⟩
  | cons c cs ih =>
    obtain ⟨hc, hcs⟩ := h
    obtain ⟨hpw, hgt⟩ := ih hcs
    refine ⟨List.pairwise_cons.mpr ⟨?_, hpw⟩, ?_⟩
    · intro d hd hs
      have h1 : c.txn.reqNo ≤ maxReq c.txn.session (P ++ [c]) :=
        le_maxReq (List.mem_append_right _ (List.mem_singleton_self _))
      have h2 := hgt d hd
      rw [← hs] at h2
      exact Nat.lt_of_le_of_lt h1 h2
    · intro d hd
      cases List.mem_cons.mp hd with
      | inl he => subst he; rw [hc.nextRequest]; exact Nat.lt_succ_self _
      | inr ht =>
        have h2 := hgt d ht
        rw [maxReq_append] at h2
        exact Nat.lt_of_le_of_lt (Nat.le_max_left _ _) h2

end RoundModel
