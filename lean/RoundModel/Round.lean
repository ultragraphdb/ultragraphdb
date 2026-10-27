/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import RoundModel.History

/-!
# The commit step deciding a round

A round is a list of commit requests, decided in round order against the history `H` the log holds when the round is
closed (spec `commit-rounds`):

1. **Session.** A request whose number is its session's last committed one is a retry, answered with the stored reply
   and not committed again; a number other than one above the last is refused as out of order.
2. **Exact validation.** It conflicts when a commit of `H`, or of its round before it, with a version above its read
   version wrote a key its read set covers. An aborted transaction adds no writes, so later requests are checked only
   against the commits admitted before them.
3. **Commit-time operations.** Decided against the serial state of every commit before it; a compare-and-set that
   does not hold refuses the transaction, reporting the value found.
4. **Version.** An admitted transaction takes `max (previous + 2) (2 × clock)`, the previous being the highest commit
   version before it; the clock is the commit step's, in microseconds, read once per round.

The decisions are a function of the round's requests, their order, the history before it and the clock: two runs of
one round from one state decide alike by construction.

**Simplifications.** Validation holds the whole history; the pruned floor and the too-old abort only refuse more and
are not modelled. The busy, too-large, past-its-limit and refused aborts come from bounds and the catalog and are not
modelled. Session registration and eviction are not modelled: a session is a number, and the table is unbounded.
The "previous" version is the highest *commit* version, so an aborted transaction takes no version.
-/

namespace RoundModel

/-- Whether a commit of `H` above `t`'s read version wrote a key `t`'s read set covers. -/
def conflicts (H : List Commit) (t : TxnRequest) : Bool :=
  H.any (fun c => decide (t.readVersion < c.version) && c.writes.any (fun w => t.reads.covers w.1))

/-- The next commit version after `H` at the commit step's clock (µs): `max (previous + 2) (2 × clock)`. -/
def nextVersion (H : List Commit) (clock : Nat) : Nat := max (maxVersion H + 2) (2 * clock)

/-- The commit step's answer to one request. -/
inductive Outcome where
  /-- Committed at the version, with the operations' results. -/
  | committed (version : Nat) (results : List OpResult)
  /-- A retry of the session's last committed request: its stored reply, not committed again. -/
  | duplicate (version : Nat) (results : List OpResult)
  /-- The request number is not one above the session's last; the last is named. -/
  | outOfOrder (last : Nat)
  /-- Aborted: a commit above its read version wrote a key it read. -/
  | conflict
  /-- Refused: a compare-and-set did not hold; the value found is named. -/
  | operationFailed (found : Option Value)
  deriving Repr

/-- Decide one request after the history `H`: its outcome, and its commit when it commits. -/
def decideTxn (H : List Commit) (clock : Nat) (t : TxnRequest) : Outcome × Option Commit :=
  if t.reqNo = maxReq t.session H ∧ maxReq t.session H ≠ 0 then
    match lastCommitOf t.session H with
    | some c => (.duplicate c.version c.results, none)
    | none => (.outOfOrder (maxReq t.session H), none)
  else if t.reqNo ≠ maxReq t.session H + 1 then (.outOfOrder (maxReq t.session H), none)
  else if conflicts H t then (.conflict, none)
  else
    match resolve (serial H) t with
    | .error f => (.operationFailed f, none)
    | .ok (ws, rs) =>
      (.committed (nextVersion H clock) rs,
        some { txn := t, version := nextVersion H clock, writes := ws, results := rs })

/-- Decide a round's requests in order after `H`: the commits it admits, in round order, and every request's
outcome. -/
def runRound (H : List Commit) (clock : Nat) : List TxnRequest → List Commit × List Outcome
  | [] => ([], [])
  | t :: ts =>
    match decideTxn H clock t with
    | (o, some c) => ((c :: (runRound (H ++ [c]) clock ts).1), o :: (runRound (H ++ [c]) clock ts).2)
    | (o, none) => ((runRound H clock ts).1, o :: (runRound H clock ts).2)

/-- An aborted or refused request adds nothing to what the requests after it are checked against: the round goes on
from the same history. The spec's "A transaction is not aborted by another's aborted write". -/
theorem runRound_aborted (H : List Commit) (clock : Nat) (t : TxnRequest) (ts : List TxnRequest) (o : Outcome)
    (h : decideTxn H clock t = (o, none)) :
    (runRound H clock (t :: ts)).1 = (runRound H clock ts).1 := by
  simp only [runRound, h]

theorem conflicts_false {H : List Commit} {t : TxnRequest} (h : conflicts H t = false) :
    ∀ c' ∈ H, t.readVersion < c'.version → ∀ w ∈ c'.writes, t.reads.covers w.1 = false := by
  intro c' hc' hlt w hw
  have h1 := List.any_eq_false.mp h c' hc'
  simp only [Bool.and_eq_true, decide_eq_true_eq, not_and] at h1
  have h2 := h1 hlt
  simp only [List.any_eq_true, not_exists, not_and] at h2
  simpa using h2 w hw

/-- A request the commit step commits is admissible after the history it was decided against. -/
theorem decideTxn_admissible {H : List Commit} {clock : Nat} {t : TxnRequest} {o : Outcome} {c : Commit}
    (heven : maxVersion H % 2 = 0) (hrv : t.readVersion ≤ maxVersion H)
    (h : decideTxn H clock t = (o, some c)) : Admissible H c := by
  unfold decideTxn at h
  split at h
  · split at h <;> simp at h
  · split at h
    · simp at h
    · rename_i _ hreq
      split at h
      · simp at h
      · rename_i hconf
        split at h
        · simp at h
        · rename_i ws rs hres
          simp only [Prod.mk.injEq, Option.some.injEq] at h
          obtain ⟨-, rfl⟩ := h
          have hreq' : t.reqNo = maxReq t.session H + 1 := by simpa using hreq
          refine
            { readBelow := hrv
              versionAbove := ?_
              versionEven := ?_
              noConflict := conflicts_false (by simpa using hconf)
              resolved := hres
              nextRequest := hreq' }
          · show maxVersion H < max (maxVersion H + 2) (2 * clock)
            exact Nat.lt_of_lt_of_le (by omega) (Nat.le_max_left _ _)
          · show max (maxVersion H + 2) (2 * clock) % 2 = 0
            rw [Nat.max_def]
            split
            · exact Nat.mul_mod_right 2 clock
            · rw [Nat.add_mod_right]; exact heven

/-- Every commit a round admits is admissible after the history and the commits admitted before it in the round,
given that every request began at or below a committed version. -/
theorem runRound_valid (H : List Commit) (clock : Nat) (ts : List TxnRequest)
    (heven : ∀ c ∈ H, c.version % 2 = 0) (hrv : ∀ t ∈ ts, t.readVersion ≤ maxVersion H) :
    ValidFrom H (runRound H clock ts).1 := by
  induction ts generalizing H with
  | nil => trivial
  | cons t ts ih =>
    have hrv' : ∀ t' ∈ ts, t'.readVersion ≤ maxVersion H := fun t' ht' => hrv t' (List.mem_cons_of_mem _ ht')
    unfold runRound
    match hd : decideTxn H clock t with
    | (o, some c) =>
      have hadm := decideTxn_admissible (maxVersion_even heven) (hrv t (List.mem_cons_self _ _)) hd
      refine ⟨hadm, ih (H ++ [c]) ?_ ?_⟩
      · intro c' hc'
        cases List.mem_append.mp hc' with
        | inl h => exact heven c' h
        | inr h => rw [List.mem_singleton.mp h]; exact hadm.versionEven
      · intro t' ht'
        exact Nat.le_trans (hrv' t' ht') (maxVersion_le_append_left H [c])
    | (o, none) => exact ih H heven hrv'

/-! ## Retries through a session -/

theorem lastCommitOf_append (s : Nat) (A B : List Commit) :
    lastCommitOf s (A ++ B) =
      match lastCommitOf s B with
      | some c => some c
      | none => lastCommitOf s A := by
  induction A with
  | nil => simp only [List.nil_append]; cases lastCommitOf s B <;> rfl
  | cons a as ih =>
    show (match lastCommitOf s (as ++ B) with
      | some c' => some c'
      | none => if a.txn.session = s then some a else none) = _
    rw [ih]
    cases lastCommitOf s B <;> rfl

theorem lastCommitOf_none {s : Nat} {l : List Commit} (h : ∀ d ∈ l, d.txn.session ≠ s) : lastCommitOf s l = none := by
  induction l with
  | nil => rfl
  | cons d ds ih =>
    simp only [lastCommitOf, ih (fun d' hd' => h d' (List.mem_cons_of_mem _ hd'))]
    simp [h d (List.mem_cons_self _ _)]

theorem maxReq_none {s : Nat} {l : List Commit} (h : ∀ d ∈ l, d.txn.session ≠ s) : maxReq s l = 0 := by
  induction l with
  | nil => rfl
  | cons d ds ih =>
    simp only [maxReq, h d (List.mem_cons_self _ _), ite_false]
    exact ih (fun d' hd' => h d' (List.mem_cons_of_mem _ hd'))

/-- A retry of a session's last committed request is answered with that commit's stored reply, and commits nothing.
The spec's "A retried commit commits once". -/
theorem decideTxn_retry {pre post : List Commit} {c : Commit} (clock : Nat) (t : TxnRequest)
    (hv : Valid (pre ++ c :: post)) (hlast : ∀ d ∈ post, d.txn.session ≠ c.txn.session)
    (hs : t.session = c.txn.session) (hn : t.reqNo = c.txn.reqNo) :
    decideTxn (pre ++ c :: post) clock t = (.duplicate c.version c.results, none) := by
  have hadm := valid_split hv
  have hmax : maxReq t.session (pre ++ c :: post) = c.txn.reqNo := by
    rw [hs, maxReq_append]
    simp only [maxReq, ite_true, maxReq_none hlast]
    rw [hadm.nextRequest]
    omega
  have hlc : lastCommitOf t.session (pre ++ c :: post) = some c := by
    rw [hs, lastCommitOf_append]
    simp [lastCommitOf, lastCommitOf_none hlast]
  have hpos : c.txn.reqNo ≠ 0 := by rw [hadm.nextRequest]; omega
  unfold decideTxn
  rw [if_pos ⟨by rw [hmax, hn], by rw [hmax]; exact hpos⟩, hlc]

end RoundModel
