/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/

/-!
# Keys, values, versions and states

The vocabulary every other module speaks. A key is one entry of the keyspace the commit step validates and the store
holds: a list key, a statement (a list key and an object), a dictionary entry, a catalog key and a unique index entry
are all keys here, because validation treats each as a point it compares by equality. Keys, values and versions are
natural numbers; a version is a `Nat` throughout, without an alias, so `omega` reads every comparison of versions.

A `State` is the latest value of every key after some serial sequence of writes, `none` where no write reached the key.
A write assigns a value to a key; writes apply in list order, the last write to a key winning.

**Simplifications.** The keyspace is unbounded (`Nat`), not finite, and values are `Nat`; a deletion is not a separate
operation and would be the write of a distinguished value, which no proof below depends on.
-/

namespace RoundModel

/-- A key of the keyspace: a list, a statement, a dictionary entry, a catalog key or a unique index entry. -/
abbrev Key := Nat

/-- A stored value. -/
abbrev Value := Nat

/-- One write: the key and the value it assigns. -/
abbrev Write := Key × Value

/-- The latest value of every key after a serial sequence of writes. -/
abbrev State := Key → Option Value

/-- The state before any write. -/
def State.empty : State := fun _ => none

/-- `s` with `k` assigned `v`. -/
def State.set (s : State) (k : Key) (v : Value) : State :=
  fun k' => if k' = k then some v else s k'

/-- Apply writes in order; the last write to a key wins. -/
def applyWrites (s : State) : List Write → State
  | [] => s
  | w :: ws => applyWrites (s.set w.1 w.2) ws

/-- The value the last write to `k` in `ws` assigns, if any write in `ws` reaches `k`. -/
def lookupWrite : List Write → Key → Option Value
  | [], _ => none
  | w :: ws, k =>
    match lookupWrite ws k with
    | some v => some v
    | none => if k = w.1 then some w.2 else none

theorem applyWrites_append (s : State) (ws₁ ws₂ : List Write) :
    applyWrites s (ws₁ ++ ws₂) = applyWrites (applyWrites s ws₁) ws₂ := by
  induction ws₁ generalizing s with
  | nil => rfl
  | cons w ws ih => exact ih (s.set w.1 w.2)

/-- The value of `k` after `ws` is the last write to `k`, or the value before when `ws` does not reach `k`. -/
theorem applyWrites_get (s : State) (ws : List Write) (k : Key) :
    applyWrites s ws k = (lookupWrite ws k).or (s k) := by
  induction ws generalizing s with
  | nil => rfl
  | cons w ws ih =>
    simp only [applyWrites, lookupWrite]
    rw [ih]
    cases h : lookupWrite ws k with
    | some v => rfl
    | none =>
      simp only [Option.or, State.set]
      by_cases hk : k = w.1 <;> simp [hk]

/-- Writes that do not reach `k` leave it as it was. -/
theorem applyWrites_get_of_not_writes (s : State) (ws : List Write) (k : Key)
    (h : ∀ w ∈ ws, w.1 ≠ k) : applyWrites s ws k = s k := by
  induction ws generalizing s with
  | nil => rfl
  | cons w ws ih =>
    simp only [applyWrites]
    rw [ih (s.set w.1 w.2) (fun w' hw' => h w' (List.mem_cons_of_mem _ hw'))]
    have hne : k ≠ w.1 := fun he => h w (List.mem_cons_self _ _) he.symm
    simp [State.set, hne]

/-- Two states that agree on `k` still agree on it after the same writes. -/
theorem applyWrites_congr (s₁ s₂ : State) (ws : List Write) (k : Key) (h : s₁ k = s₂ k) :
    applyWrites s₁ ws k = applyWrites s₂ ws k := by
  rw [applyWrites_get, applyWrites_get, h]

/-- Applying the same writes twice in a row is applying them once: every write is an assignment. -/
theorem applyWrites_idem (s : State) (ws : List Write) :
    applyWrites (applyWrites s ws) ws = applyWrites s ws := by
  funext k
  rw [applyWrites_get, applyWrites_get]
  cases lookupWrite ws k <;> rfl

end RoundModel
