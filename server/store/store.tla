---------------------------- MODULE store ----------------------------
(* The sync store engine over object storage (docs/FORMATS.md,
   server/store/store.go), model-checked by `just spec` (TLC, explicit
   enumeration at small bounds — `just test` never requires Java).

   What the bucket guarantees, and all the model assumes of it (the
   backend contract, server/store/backend.go): a whole-object create
   that lands only if its key is absent, read and list after write,
   delete. Nothing is appended, renamed, overwritten, or locked.

   Model: one pointer, the writer processes in Procs sharing one bucket,
   and one offline `store gc` (D-117) running beside them. Each process
   caches the head it last read (the cache may be stale — another
   process commits under it). A commit:
     1. PutBlob  — create blobs/<h> if absent (an existing blob, even an
                   aged orphan, is reused: same key, same bytes);
     2. Record   — create record base+1 if absent: the conditional
                   create is the CAS; a present key is a conflict, and
                   the process re-reads the head;
     3. Check    — re-read its blob after the record, and re-create it
                   if missing (the process still holds the bytes).
   A process may crash between any two steps and restart with an empty
   cache.

   A GC pass:
     List    — snapshot the records' blobs and the aged, unreferenced
               blobs (the candidates; age past the grace is Tick);
     Trash   — per candidate: copy blobs/<h> to gc/<h>, delete blobs/<h>;
     Settle  — re-list the records, restore (create if absent) every
               trashed blob a record now names, then empty gc/.
   A GC pass may crash anywhere; the next pass begins with Settle.

   Why Settle: a commit can reuse an aged orphan after List snapshots
   the records, land its record, pass its Check while the orphan is
   still there — and then Trash deletes it. Neither the grace nor the
   commit's Check sees that; the re-list after the deletes does, and the
   trash still holds the bytes. With Restore = FALSE (no trash, no
   re-list) TLC finds exactly that trace against RefsDurable.

   The merge semilattice is NOT here: it is a pure function and
   proptest owns it (crates/transit-sync); this spec covers the
   engine's interleavings. *)

EXTENDS Naturals, FiniteSets, TLC

CONSTANTS
    Procs,          \* the server processes sharing the bucket
    Hashes,         \* the finite set of blob hashes
    MaxSeq,         \* bound on the pointer's records
    Restore         \* TRUE: GC trashes and settles; FALSE: GC deletes outright

NoRec == [from |-> "-", to |-> "-"]

(* Hashes and processes are interchangeable — every action and
   invariant treats them uniformly — so TLC exploits symmetry (cfg:
   SYMMETRY Symmetry). *)
Symmetry == Permutations(Hashes) \cup Permutations(Procs)

VARIABLES
    blobs,      \* the hashes present under blobs/
    aged,       \* the blobs past the grace period
    trash,      \* the hashes present under gc/
    recs,       \* seq -> record (NoRec = absent); records are immutable
    cache,      \* proc -> the head seq it last read
    pc,         \* proc -> "idle" | "blob" | "rec" | "check"
    want,       \* proc -> the hash it is committing
    base,       \* proc -> the seq its commit builds on
    gcpc,       \* "idle" | "listed"
    cand,       \* the GC pass's remaining candidates
    broken      \* hashes whose commit crashed between Record and Check

vars == <<blobs, aged, trash, recs, cache, pc, want, base, gcpc, cand, broken>>

Seqs == 1..MaxSeq
top == Cardinality({i \in Seqs : recs[i] /= NoRec})
HashAt(s) == IF s = 0 THEN "" ELSE recs[s].to
named == {recs[i].to : i \in {j \in Seqs : recs[j] /= NoRec}}

TypeOK ==
    /\ blobs \subseteq Hashes /\ aged \subseteq blobs /\ trash \subseteq Hashes
    /\ recs \in [Seqs -> [from : Hashes \cup {"", "-"}, to : Hashes \cup {"-"}]]
    /\ cache \in [Procs -> 0..MaxSeq]
    /\ pc \in [Procs -> {"idle", "blob", "rec", "check"}]
    /\ want \in [Procs -> Hashes \cup {"-"}]
    /\ base \in [Procs -> 0..MaxSeq]
    /\ gcpc \in {"idle", "listed"}
    /\ cand \subseteq Hashes
    /\ broken \subseteq Hashes

Init ==
    /\ blobs = {} /\ aged = {} /\ trash = {}
    /\ recs = [i \in Seqs |-> NoRec]
    /\ cache = [p \in Procs |-> 0]
    /\ pc = [p \in Procs |-> "idle"]
    /\ want = [p \in Procs |-> "-"]
    /\ base = [p \in Procs |-> 0]
    /\ gcpc = "idle" /\ cand = {}
    /\ broken = {}

---------------------------------------------------------------------------
(* A process *)

(* Read the head: list the pointer's records. *)
Refresh(p) ==
    /\ pc[p] = "idle"
    /\ cache' = [cache EXCEPT ![p] = top]
    /\ UNCHANGED <<blobs, aged, trash, recs, pc, want, base, gcpc, cand, broken>>

(* Begin a commit of h from the cached head. Committing the head's own
   bytes is the idempotent redelivery: no record, so not a step here. *)
Begin(p, h) ==
    /\ pc[p] = "idle"
    /\ cache[p] < MaxSeq
    /\ h /= HashAt(cache[p])
    /\ pc' = [pc EXCEPT ![p] = "blob"]
    /\ want' = [want EXCEPT ![p] = h]
    /\ base' = [base EXCEPT ![p] = cache[p]]
    /\ UNCHANGED <<blobs, aged, trash, recs, cache, gcpc, cand, broken>>

(* 1. Create the blob if absent; a present one (fresh or an aged
   orphan) is reused untouched. *)
PutBlob(p) ==
    /\ pc[p] = "blob"
    /\ IF want[p] \in blobs
         THEN UNCHANGED <<blobs, aged>>
         ELSE /\ blobs' = blobs \cup {want[p]}
              /\ UNCHANGED aged
    /\ pc' = [pc EXCEPT ![p] = "rec"]
    /\ UNCHANGED <<trash, recs, cache, want, base, gcpc, cand, broken>>

(* 2. Create record base+1 if absent — the CAS. A present key is a
   conflict: the head moved; re-read it and return the conflict. *)
Record(p) ==
    /\ pc[p] = "rec"
    /\ LET s == base[p] + 1 IN
       IF recs[s] = NoRec
         THEN /\ recs' = [recs EXCEPT ![s] = [from |-> HashAt(base[p]), to |-> want[p]]]
              /\ cache' = [cache EXCEPT ![p] = s]
              /\ pc' = [pc EXCEPT ![p] = "check"]
         ELSE /\ cache' = [cache EXCEPT ![p] = top]
              /\ pc' = [pc EXCEPT ![p] = "idle"]
              /\ UNCHANGED recs
    /\ UNCHANGED <<blobs, aged, trash, want, base, gcpc, cand, broken>>

(* 3. Re-read the blob after the record; re-create it if missing. *)
Check(p) ==
    /\ pc[p] = "check"
    /\ IF want[p] \in blobs
         THEN UNCHANGED blobs
         ELSE blobs' = blobs \cup {want[p]}
    /\ pc' = [pc EXCEPT ![p] = "idle"]
    /\ UNCHANGED <<aged, trash, recs, cache, want, base, gcpc, cand, broken>>

(* A crash: the process restarts with nothing cached. A crash after its
   record and before its Check leaves that record's blob to
   `store verify` (broken). *)
Crash(p) ==
    /\ pc[p] /= "idle"
    /\ broken' = IF pc[p] = "check" THEN broken \cup {want[p]} ELSE broken
    /\ pc' = [pc EXCEPT ![p] = "idle"]
    /\ cache' = [cache EXCEPT ![p] = 0]
    /\ UNCHANGED <<blobs, aged, trash, recs, want, base, gcpc, cand>>

---------------------------------------------------------------------------
(* Time and the offline GC *)

(* A blob outlives the grace period. *)
Tick(h) ==
    /\ h \in blobs \ aged
    /\ aged' = aged \cup {h}
    /\ UNCHANGED <<blobs, trash, recs, cache, pc, want, base, gcpc, cand, broken>>

(* List: the records, then the blobs with their ages. *)
List ==
    /\ gcpc = "idle"
    /\ (Restore => trash = {})
    /\ gcpc' = "listed"
    /\ cand' = aged \ named
    /\ UNCHANGED <<blobs, aged, trash, recs, cache, pc, want, base, broken>>

(* Trash one candidate: copy it to gc/, delete it from blobs/ (with
   Restore = FALSE: delete it outright). *)
Trash(h) ==
    /\ gcpc = "listed"
    /\ h \in cand
    /\ trash' = IF Restore /\ h \in blobs THEN trash \cup {h} ELSE trash
    /\ blobs' = blobs \ {h}
    /\ aged' = aged \ {h}
    /\ cand' = cand \ {h}
    /\ UNCHANGED <<recs, cache, pc, want, base, gcpc, broken>>

(* Settle: re-list the records, restore every trashed blob a record now
   names, empty gc/. Ends a pass; also begins one after a GC crash. *)
Settle ==
    /\ Restore
    /\ \/ gcpc = "listed" /\ cand = {}
       \/ gcpc = "idle" /\ trash /= {}
    /\ blobs' = blobs \cup (trash \cap named)
    /\ trash' = {}
    /\ gcpc' = "idle"
    /\ UNCHANGED <<aged, recs, cache, pc, want, base, cand, broken>>

(* Without Restore a pass ends when its candidates are gone. *)
Done ==
    /\ ~Restore
    /\ gcpc = "listed" /\ cand = {}
    /\ gcpc' = "idle"
    /\ UNCHANGED <<blobs, aged, trash, recs, cache, pc, want, base, cand, broken>>

GCCrash ==
    /\ gcpc = "listed"
    /\ gcpc' = "idle"
    /\ cand' = {}
    /\ UNCHANGED <<blobs, aged, trash, recs, cache, pc, want, base, broken>>

Next ==
    \/ \E p \in Procs :
        \/ Refresh(p) \/ PutBlob(p) \/ Record(p) \/ Check(p) \/ Crash(p)
        \/ \E h \in Hashes : Begin(p, h)
    \/ \E h \in Hashes : Tick(h) \/ Trash(h)
    \/ List \/ Settle \/ Done \/ GCCrash

Spec == Init /\ [][Next]_vars

---------------------------------------------------------------------------
(* Invariants *)

(* The records are 1..top with no gap, and each one's from is the prior
   one's to: the log chains. *)
LogChained ==
    /\ \A i \in Seqs : recs[i] /= NoRec <=> i <= top
    /\ \A i \in 1..top : recs[i].from = HashAt(i - 1)

(* Every record names a durable blob — under blobs/, or under gc/ until
   the pass settles — except a commit between its record and its Check,
   and one that crashed there (`store verify` reports it). *)
RefsDurable ==
    \A i \in 1..top :
        \/ recs[i].to \in blobs \cup trash
        \/ \E p \in Procs : pc[p] = "check" /\ want[p] = recs[i].to
        \/ recs[i].to \in broken

(* At rest — no GC pass open, no commit mid-Check — every named blob is
   under blobs/ (bar a crash `store verify` reports). *)
AtRest ==
    (gcpc = "idle" /\ trash = {} /\ \A p \in Procs : pc[p] /= "check")
        => named \subseteq blobs \cup broken

(* A process's cache never runs ahead of the bucket. *)
CacheBehind == \A p \in Procs : cache[p] <= top

---------------------------------------------------------------------------
(* Action properties *)

(* A record, once created, never changes — so a failed CAS leaves
   authority untouched, and two creates of one seq cannot both land. *)
RecordsImmutable ==
    [][\A i \in Seqs : recs[i] /= NoRec => recs'[i] = recs[i]]_vars

(* The head only moves forward, one record at a time. *)
HeadMonotone == [][top' = top \/ top' = top + 1]_vars

===============================================================================
