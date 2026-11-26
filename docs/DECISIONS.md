# Decisions

This file holds the open questions put to the user, and the answers
that are not built yet. When an agent hits a decision that the user's
intent, the GTFS spec, and the architecture rules leave open, it writes
the question down and does not build on an answer that it picked
(AGENTS.md, "Clarify, don't assume"). The entry names the work that it
blocks, so the rest of the work can go ahead.

Each entry is one file, `docs/decisions/D-N.md`. This file holds the
rules and the summary, which links to each entry. Read the summary
first, and open only the entries you need.

## The rules

- **One file per question**, `docs/decisions/D-N.md`, in this format:

  ```
  # D-N — the question

  Topic: the area it belongs to (for example, Service design).
  State: Unanswered | Encoded provisionally | Decided
  Answer: (Decided only) the user's answer, dated, in their words.
  Context: what the intent, the spec, and the code say, briefly.
  Options: each answer and what it costs. Mark the one the code
    encodes, if any.
  Pinned by: the property tests and specs that hold the answer.
  Blocks: the units, findings, or tests that cannot proceed until it
    is decided. "Nothing" is a valid answer.
  ```

- **State.** *Unanswered* means nothing is built on any answer, and
  the work it blocks waits. *Encoded provisionally* means an agent had
  to pick an answer to land a property test. The tests that pin that
  answer are named, and a different answer rewrites them. *Decided*
  records the user's answer, dated, in their words.
- **Blocked work names its decision.** A unit or finding that waits
  on a decision says `Blocked: D-N`. Work that names none is free to
  start.
- **A built decision leaves this record.** In the change that builds a
  decided answer, or in a later change that confirms it is built, its
  substance moves to its homes, and the entry is deleted with its
  summary row. Git keeps the entry.
  - The property goes in the test that pins it, stated in one line.
  - The rule, its reason, and each rejected alternative (one line
    each) go in the doc or code comment that owns the topic
    (`docs/CONVENTIONS.md`, "What to write").
  - Every citation of the id goes in the same change. Find them with
    `rg -n -w 'D-N'`.
- **Cite an id only while its entry exists.** A doc, comment, or test
  that states a rule says it in its own words, and cites the id only
  while the question is open or the answer is not built.
- **A reversed answer is a new entry.** It names the entry it
  replaces, and the replaced entry is deleted in the same change.
- **The summary has one row for each entry.** A new entry adds its
  row in the same change. A change of state or of what it blocks
  updates the row and the entry together.
- IDs are never reused. The next new decision is **D-255**.

## Summary

| id | question | state | blocks |
|---|---|---|---|
| [D-1](decisions/D-1.md) | Is a trip's start its first arrival or its first departure? | encoded | nothing |
| [D-2](decisions/D-2.md) | What does a running-time change touch besides fixed-start trips? | encoded | nothing |
| [D-3](decisions/D-3.md) | Which timing profile does the import give a pattern? | decided; not built | nothing |
| [D-5](decisions/D-5.md) | Does the import keep a block that breaks the spec? | encoded | nothing |
| [D-12](decisions/D-12.md) | What headsign does a new trip get? | encoded | nothing |
| [D-14](decisions/D-14.md) | Does the proposal comparison collect public responses? | decided | nothing |
| [D-16](decisions/D-16.md) | Where does the runs layer live? | encoded | nothing |
| [D-17](decisions/D-17.md) | What does "the last bus" mean with no deadline? | encoded | nothing |
| [D-18](decisions/D-18.md) | How do live delays meet a diverted trip? | encoded | nothing |
| [D-20](decisions/D-20.md) | Which language does an alert speak? | encoded | nothing |
| [D-21](decisions/D-21.md) | Where is a trip going when nothing signs it? | encoded | nothing |
| [D-22](decisions/D-22.md) | How is a route's rider variant named and ordered? | encoded | nothing |
| [D-23](decisions/D-23.md) | In what order does a stop display list its groups? | encoded | nothing |
| [D-24](decisions/D-24.md) | How does a departure's countdown round? | encoded | nothing |
| [D-28](decisions/D-28.md) | Where do stop and address results meet? | encoded | nothing |
| [D-30](decisions/D-30.md) | What does the streets import call a street, and what class is it? | encoded | nothing |
| [D-32](decisions/D-32.md) | How does the router walk with no walk table? | encoded | nothing |
| [D-33](decisions/D-33.md) | How does a trip from a place walk? | encoded | nothing |
| [D-34](decisions/D-34.md) | Where does a stop no walk reaches go in the nearby list? | encoded | nothing |
| [D-35](decisions/D-35.md) | Among plans arriving together, where does the rider board and get off? | encoded | nothing |
| [D-37](decisions/D-37.md) | What names a spot dropped on the map? | encoded | nothing |
| [D-43](decisions/D-43.md) | What does a coverage comparison report? | encoded | QAF-20's matching rule |
| [D-44](decisions/D-44.md) | Is a branch of a direction its own thing? | decided; not built | nothing |
| [D-52](decisions/D-52.md) | How is a route-hour's frequency judged and drawn? | encoded | nothing |
| [D-53](decisions/D-53.md) | What ridership data does the planner bring? | decided | nothing |
| [D-54](decisions/D-54.md) | What does a frequency change keep of the trips already there? | decided; not built | nothing |
| [D-56](decisions/D-56.md) | Whose clock is the LWW clock? | encoded | nothing |
| [D-57](decisions/D-57.md) | How long does a tombstone live? | encoded | nothing |
| [D-70](decisions/D-70.md) | How do the module runtime's bounds default? | encoded | nothing |
| [D-72](decisions/D-72.md) | Is the release profile tuned for size or for speed? | encoded | nothing |
| [D-74](decisions/D-74.md) | How is a pick published, and what runs before the first one? | encoded | nothing |
| [D-83](decisions/D-83.md) | What does each deployed device report, and to where? | decided | nothing |
| [D-84](decisions/D-84.md) | Who takes a build first, and what counts as healthy? | decided | nothing |
| [D-86](decisions/D-86.md) | Which op answers what an edit record amounts to on the network? | encoded | nothing |
| [D-87](decisions/D-87.md) | How does an export name the import numbering it writes back? | encoded | nothing |
| [D-88](decisions/D-88.md) | What does a page run when its linked assembly does not load? | encoded | nothing |
| [D-89](decisions/D-89.md) | May a host decline a document an op minted? | encoded | nothing |
| [D-90](decisions/D-90.md) | What may a rider's device remember to open the map where it will end up? | encoded | nothing |
| [D-91](decisions/D-91.md) | What does fuzzy stop search accept, and in what order? | encoded | nothing |
| [D-92](decisions/D-92.md) | When does a plan say "leave in N min"? | encoded | nothing |
| [D-94](decisions/D-94.md) | Which types make a combination, and how are its dates folded? | encoded | nothing |
| [D-95](decisions/D-95.md) | Which service type does a gesture change, and what does its warning name? | encoded | nothing |
| [D-97](decisions/D-97.md) | Where does a layer's weight field and label live? | encoded | nothing |
| [D-98](decisions/D-98.md) | How finely is a polygon's area sampled? | encoded | nothing |
| [D-99](decisions/D-99.md) | Who names the two sides of a coverage comparison? | encoded | nothing |
| [D-100](decisions/D-100.md) | How does a board show a trip the dispatcher cancelled? | encoded | nothing |
| [D-101](decisions/D-101.md) | What does a trip cancel name, and how does it enter the edit document? | encoded | nothing |
| [D-102](decisions/D-102.md) | Which capability authors a trip cancel? | encoded | nothing |
| [D-103](decisions/D-103.md) | Which street classes does the bus profile ask a planner to confirm? | encoded | nothing |
| [D-104](decisions/D-104.md) | Where does a fork's period end come from? | encoded | nothing |
| [D-106](decisions/D-106.md) | Does a recovery rule judge a trip that continues with no wait? | encoded | nothing |
| [D-107](decisions/D-107.md) | Which routes may interline when the rules do not say? | encoded | nothing |
| [D-108](decisions/D-108.md) | What is an hours budget? | unanswered | hours against a budget: `ui-service-design` question 2, `ui-service-change` (the proposal) |
| [D-109](decisions/D-109.md) | What does a suggested blocking change? | encoded | nothing |
| [D-110](decisions/D-110.md) | Which waits does a recovery requirement judge, and after which trip? | encoded | nothing |
| [D-111](decisions/D-111.md) | When does a dated service read as a weekly rule? | encoded | nothing |
| [D-112](decisions/D-112.md) | Which weekly service types does a fork carry? | encoded | nothing |
| [D-114](decisions/D-114.md) | What does the clock answer in the hour before a fall-back day's anchor? | encoded | nothing |
| [D-120](decisions/D-120.md) | Does compaction drop a record whose dates have passed? | unanswered | nothing |
| [D-123](decisions/D-123.md) | Do trip, block, and run ids survive from one pick to the next? | unanswered | nothing |
| [D-124](decisions/D-124.md) | Does this repo publish its cancels and detours as GTFS-rt? | decided; deferred | nothing |
| [D-125](decisions/D-125.md) | Does an edit record carry its source system's id? | unanswered | nothing |
| [D-126](decisions/D-126.md) | Is the runs layer shaped by TODS? | decided | nothing |
| [D-127](decisions/D-127.md) | Where does operations history come from? | decided | nothing |
| [D-129](decisions/D-129.md) | Where does the authored GTFS-rt reach riders? | unanswered | nothing (publishing is deferred, D-124) |
| [D-130](decisions/D-130.md) | Where does a display's identity live on the device? | decided | nothing |
| [D-133](decisions/D-133.md) | What counts as a just-missed connection? | encoded | nothing |
| [D-134](decisions/D-134.md) | How does the network rank its longest waits? | encoded | nothing |
| [D-136](decisions/D-136.md) | What does the worklist call weak? | encoded | nothing |
| [D-137](decisions/D-137.md) | How are a plan's service combinations shown as day types? | encoded | nothing |
| [D-142](decisions/D-142.md) | Does serving a stop act on the direction or on one pattern? | decided; not built | nothing |
| [D-143](decisions/D-143.md) | What does redrawing a line do to running time? | decided; not built | nothing |
| [D-161](decisions/D-161.md) | When a survey and the agency's other data disagree, which wins? | decided | nothing |
| [D-164](decisions/D-164.md) | Does the store log hide who wrote each change from callers without audit? | unanswered | nothing |
| [D-166](decisions/D-166.md) | What is the smallest part of the stops document two writers can change at once? | decided; partly built | the stops merge in `storage-model` |
| [D-177](decisions/D-177.md) | What does a journal keep, and where? | encoded provisionally | nothing |
| [D-178](decisions/D-178.md) | Is a page's journal on by default? | encoded provisionally | nothing |
| [D-179](decisions/D-179.md) | Which module does replay run a journal's call on? | encoded provisionally | nothing |
| [D-180](decisions/D-180.md) | What is service design for, beside service change? | decided; not built | nothing |
| [D-181](decisions/D-181.md) | What is the network review called? | unanswered | the review UI's title and id |
| [D-182](decisions/D-182.md) | Does the server compress its own static files? | encoded | nothing |
| [D-183](decisions/D-183.md) | What policy do the rider pages send, and who writes the export's `_headers`? | encoded | nothing |
| [D-184](decisions/D-184.md) | What color is the text on a route's badge? | encoded | nothing |
| [D-187](decisions/D-187.md) | Is performance held to fixed budgets? | decided; not built | nothing |
| [D-190](decisions/D-190.md) | How does the agency's record store the data people edit? | decided; the disruptions, stops, and plans logs built | nothing |
| [D-191](decisions/D-191.md) | Is a service-change draft part of the agency's record, and what does publishing mean? | decided; freezing and adopting built on the server | nothing |
| [D-192](decisions/D-192.md) | Are the edit document's existing names renamed to "disruptions"? | unanswered | nothing |
| [D-193](decisions/D-193.md) | What does a revert write back? | encoded | nothing |
| [D-194](decisions/D-194.md) | Who may read a log's entries? | encoded | nothing |
| [D-195](decisions/D-195.md) | How does the server keep and check a log's checkpoints? | encoded | nothing |
| [D-200](decisions/D-200.md) | How does a stored derived document name the inputs it was made from? | encoded | nothing |
| [D-201](decisions/D-201.md) | How does the server serve freezing and adopting? | encoded | nothing |
| [D-202](decisions/D-202.md) | Where does the kind table live, and what does it govern? | encoded | nothing |
| [D-203](decisions/D-203.md) | How does the store describe itself? | encoded | nothing |
| [D-205](decisions/D-205.md) | Who turns an alert's local times into GTFS-rt's Unix seconds? | decided; not built | nothing |
| [D-206](decisions/D-206.md) | Who writes service alerts? | decided; not built | nothing |
| [D-207](decisions/D-207.md) | How do dispatch and communications staff share the alerts workspace? | decided; not built | nothing |
| [D-209](decisions/D-209.md) | Are service alerts and disruptions one UI? | decided; not built | nothing |
| [D-211](decisions/D-211.md) | How is the service alerts document shaped? | encoded | nothing |
| [D-213](decisions/D-213.md) | Where does the disruptions UI read today's cancels? | decided; not built | nothing |
| [D-217](decisions/D-217.md) | How is the notes document shaped? | encoded | nothing |
| [D-219](decisions/D-219.md) | How is a day's disruptions answer shaped? | encoded | nothing |
| [D-220](decisions/D-220.md) | Which riders does an alert about a disruption inform? | encoded | nothing |
| [D-221](decisions/D-221.md) | Does a station's stop_id in an alert reach its bays? | encoded | nothing |
| [D-223](decisions/D-223.md) | Does every install download the staff pages' modules? | encoded | nothing |
| [D-224](decisions/D-224.md) | What does a server copy started while the bucket does not answer serve? | encoded | nothing |
| [D-229](decisions/D-229.md) | Does the record's integrity encode the domain? | decided | nothing |
| [D-230](decisions/D-230.md) | When an adoption breaks a live reference, does it refuse or warn? | decided | nothing |
| [D-232](decisions/D-232.md) | How are references declared, and which stage may name which? | encoded | nothing |
| [D-233](decisions/D-233.md) | When does the receive path refuse a write whose reference is broken? | encoded | nothing |
| [D-240](decisions/D-240.md) | While a log's boundary moves, do writes to its kinds pause? | unanswered | `log-boundaries.md` items 2 and 3 |
| [D-241](decisions/D-241.md) | What must hold before a split runs? | unanswered | `log-boundaries.md` item 3 (the split) |
| [D-242](decisions/D-242.md) | What runs a move of a log's boundary, and when? | unanswered | `log-boundaries.md` item 3 |
| [D-243](decisions/D-243.md) | In which order does the manifest list the capabilities and their ops? | encoded | nothing |
| [D-245](decisions/D-245.md) | Does a failed input stop the answers that read it? | encoded | nothing |
| [D-246](decisions/D-246.md) | When does a page read a failed input again? | encoded | nothing |
| [D-247](decisions/D-247.md) | Do the staff pages follow the deployment's network as it moves? | encoded | nothing |
| [D-249](decisions/D-249.md) | Where do the HTTP contract's wrapper shapes come from when it is generated? | decided | nothing |
