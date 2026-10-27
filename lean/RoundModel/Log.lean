/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import RoundModel.Round

/-!
# The log, the store and recovery

The log is a list of rounds; a round's position is its index from 1. A round holds the commits it admitted, with their
resolved writes: the log records decisions, so replaying a round re-decides nothing (D2).

The store is derived state (D1): the position of the last round it applied, and the multiversion record of the commits
it applied, read at a version through `snapshot`. Applying a round appends its commits and advances the position;
the round carries its position, and a round at or below the store's position is skipped, which is what makes a replay
idempotent (spec `write-ahead-log`, "Recovery replays the log above the store").

Recovery reads the store's position and applies every later round of the log in order. The power loss before it may
leave the store at any prefix of the rounds it had applied, as fjall recovers a prefix of its batches.

**Simplifications.** The store's multiversion record is the list of the commits it applied, not a keyed structure;
rounds are not framed, checksummed or segmented, so the scan, torn tails, interior corruption, the superblock,
checkpoints and trimming are not modelled; the log's position is the round's index. A recovery that finds a gap or a
store ahead of the log refuses to start; the model never produces either state, so the refusal is not modelled.
-/

namespace RoundModel

/-- A round as the log holds it: the commits it admitted, in round order. -/
structure Round where
  commits : List Commit
  deriving Repr

/-- The commits of a list of rounds, in commit order. -/
def commitsOf : List Round → List Commit
  | [] => []
  | r :: rs => r.commits ++ commitsOf rs

theorem commitsOf_append (A B : List Round) : commitsOf (A ++ B) = commitsOf A ++ commitsOf B := by
  induction A with
  | nil => rfl
  | cons r rs ih =>
    show r.commits ++ commitsOf (rs ++ B) = (r.commits ++ commitsOf rs) ++ commitsOf B
    rw [ih, List.append_assoc]

/-- The store: the position of the last round applied, and the commits applied. -/
structure Store where
  applied : Nat
  data : List Commit
  deriving Repr

/-- The store holding the rounds `rs` applied. -/
def Store.ofRounds (rs : List Round) : Store := { applied := rs.length, data := commitsOf rs }

/-- An empty store. -/
def Store.empty : Store := Store.ofRounds []

/-- A read at version `v` of key `k`. -/
def Store.read (st : Store) (v : Nat) (k : Key) : Option Value := snapshot st.data v k

/-- Apply the round at the next position. -/
def Store.applyNext (st : Store) (r : Round) : Store :=
  { applied := st.applied + 1, data := st.data ++ r.commits }

/-- Apply the round at position `pos`: only when it is the next position, so a round already applied is skipped. -/
def Store.applyAt (st : Store) (pos : Nat) (r : Round) : Store :=
  if pos = st.applied + 1 then st.applyNext r else st

/-- Apply rounds at the positions following the store's. -/
def replay (st : Store) : List Round → Store
  | [] => st
  | r :: rs => replay (st.applyNext r) rs

/-- Apply rounds starting at position `pos`, each through its position. -/
def replayAt (st : Store) (pos : Nat) : List Round → Store
  | [] => st
  | r :: rs => replayAt (st.applyAt pos r) (pos + 1) rs

/-- Recovery: apply every round of the log above the store's position. -/
def recover (st : Store) (log : List Round) : Store := replay st (log.drop st.applied)

/-! ## Idempotent replay -/

/-- Replaying a round already applied leaves the store unchanged. -/
theorem applyAt_applied (st : Store) (pos : Nat) (r : Round) (h : pos ≤ st.applied) : st.applyAt pos r = st := by
  unfold Store.applyAt
  rw [if_neg (by omega)]

theorem applyNext_ofRounds (P : List Round) (r : Round) :
    (Store.ofRounds P).applyNext r = Store.ofRounds (P ++ [r]) := by
  simp [Store.applyNext, Store.ofRounds, commitsOf_append, commitsOf]

/-- Replaying rounds onto the store of the rounds before them is the store of all of them. -/
theorem replay_ofRounds (P Q : List Round) : replay (Store.ofRounds P) Q = Store.ofRounds (P ++ Q) := by
  induction Q generalizing P with
  | nil => simp [replay]
  | cons q qs ih =>
    simp only [replay, applyNext_ofRounds, ih, List.append_assoc, List.singleton_append]

theorem replayAt_append (st : Store) (pos : Nat) (A B : List Round) :
    replayAt st pos (A ++ B) = replayAt (replayAt st pos A) (pos + A.length) B := by
  induction A generalizing st pos with
  | nil => rfl
  | cons a as ih =>
    show replayAt (st.applyAt pos a) (pos + 1) (as ++ B) = _
    rw [ih]
    simp only [replayAt, List.length_cons]
    rw [Nat.add_assoc, Nat.add_comm 1 as.length]

/-- Rounds whose positions are all at or below the store's are all skipped. -/
theorem replayAt_applied (st : Store) (pos : Nat) (rs : List Round) (h : pos + rs.length ≤ st.applied + 1) :
    replayAt st pos rs = st := by
  induction rs generalizing pos with
  | nil => rfl
  | cons r rs ih =>
    simp only [List.length_cons] at h
    simp only [replayAt]
    rw [applyAt_applied st pos r (by omega)]
    exact ih (pos + 1) (by omega)

/-- Rounds at the positions following the store's are each applied once. -/
theorem replayAt_ofRounds (P Q : List Round) :
    replayAt (Store.ofRounds P) (P.length + 1) Q = Store.ofRounds (P ++ Q) := by
  induction Q generalizing P with
  | nil => simp [replayAt]
  | cons q qs ih =>
    simp only [replayAt]
    have : (Store.ofRounds P).applyAt (P.length + 1) q = Store.ofRounds (P ++ [q]) := by
      unfold Store.applyAt
      rw [if_pos (by simp [Store.ofRounds]), applyNext_ofRounds]
    rw [this]
    have hl : P.length + 1 + 1 = (P ++ [q]).length + 1 := by simp
    rw [hl, ih, List.append_assoc, List.singleton_append]

/-- Replaying the whole log, every round through its position, onto a store holding any prefix of it gives the store
of the log applied once. -/
theorem replay_whole_log (log : List Round) (j : Nat) (hj : j ≤ log.length) :
    replayAt (Store.ofRounds (log.take j)) 1 log = Store.ofRounds log := by
  have hlen : (log.take j).length = j := by rw [List.length_take]; omega
  have hsplit := List.take_append_drop j log
  calc replayAt (Store.ofRounds (log.take j)) 1 log
      = replayAt (Store.ofRounds (log.take j)) 1 (log.take j ++ log.drop j) := by rw [hsplit]
    _ = replayAt (Store.ofRounds (log.take j)) ((log.take j).length + 1) (log.drop j) := by
        rw [replayAt_append, replayAt_applied _ 1 (log.take j) (by simp [Store.ofRounds, hlen]; omega),
          Nat.add_comm]
    _ = Store.ofRounds log := by rw [replayAt_ofRounds, hsplit]

/-- Recovery from a store holding any prefix of a log gives the store of the log applied once. -/
theorem recover_prefix (log : List Round) (j : Nat) (hj : j ≤ log.length) :
    recover (Store.ofRounds (log.take j)) log = Store.ofRounds log := by
  have hlen : (log.take j).length = j := by rw [List.length_take]; omega
  unfold recover
  simp only [Store.ofRounds] at *
  have := replay_ofRounds (log.take j) (log.drop j)
  simp only [Store.ofRounds, hlen, List.take_append_drop] at this
  rw [hlen]
  exact this

/-- Recovering twice is recovering once. -/
theorem recover_idem (log : List Round) (j : Nat) (hj : j ≤ log.length) :
    recover (recover (Store.ofRounds (log.take j)) log) log = recover (Store.ofRounds (log.take j)) log := by
  rw [recover_prefix log j hj]
  have := recover_prefix log log.length (Nat.le_refl _)
  rwa [List.take_length] at this

/-- Applying a round's commits twice in a row to a state is applying them once: the log holds resolved writes. -/
theorem round_apply_idem (s : State) (r : Round) : serialFrom (serialFrom s r.commits) r.commits = serialFrom s r.commits :=
  serialFrom_idem s r.commits

end RoundModel
