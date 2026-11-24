# Decisions

This is the durable record of the questions put to the user and
their answers. When an agent hits a decision that the user's intent,
the GTFS spec, and the architecture laws leave open, the agent writes
it down and does not build on an answer it picked (AGENTS.md,
"Clarify, don't assume"). The entry names the work the decision
blocks, so the rest of the work can go ahead while the decision is
reviewed.

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
  Context: (open entries) what the intent, the spec, and the code
    say, briefly.
  Options: (open entries) each answer and what it costs; mark the one
    the code encodes (if any).
  Pinned by: the laws, tests, and specs that hold the answer.
  Why: (decided entries) the topic file and the heading, under its
    "Why", that holds the reasoning and the alternatives considered.
  Blocks: the units, findings, or laws that cannot proceed or be
    deleted until it is decided. "Nothing" is a valid answer.
  ```

- **A decided entry is the record, not the reasoning.** When an entry
  is decided, its Context and Options move into the "Why" section of
  the architecture topic it shapes (`docs/architecture/<topic>.md`),
  rewritten in the present tense with the alternatives named, and the
  entry keeps its question, the answer in the user's words, its pins,
  what it blocks, and a `Why:` pointer. An open entry (unanswered or
  encoded provisionally) keeps its Context and Options: the question
  is still in front of the user. So does a decided entry whose work is
  not built yet: its reasoning moves to the topic's Why in the change
  that builds it.

- **State.** *Unanswered* means nothing is built on any answer, and
  the work it blocks waits. *Encoded provisionally* means an agent
  had to pick an answer to land a law. The laws that pin that
  answer are named, and a different decision rewrites them.
- **Blocked work names its decision.** A unit or finding that
  waits on a decision says `Blocked: D-N`. Work that does not name
  one is free to start.
- **A decided entry stays — decisions are a durable record, never
  deleted.** Its state becomes *Decided* and its Answer line records
  what the user said and when. In the same change, the answer goes
  into the law that pins it and the doc that owns its topic
  (`docs/architecture/`, `docs/units/`, `docs/QA-FINDINGS.md`),
  and the `Blocked: D-N` lines are removed. A later answer that
  reverses one is a new entry naming the entry it replaces; the old
  entry's state says so.
- **The summary has one row for each entry.** A new entry adds its
  row in the same change. A change of state or of what it blocks
  updates the row and the entry together.
- IDs are never reused; the next new decision is **D-252**.

## Summary

| id | question | state | blocks |
|---|---|---|---|
| [D-1](decisions/D-1.md) | Is a trip's start its first arrival or its first departure? | encoded | nothing |
| [D-2](decisions/D-2.md) | What does a running-time change touch besides fixed-start trips? | encoded | nothing |
| [D-3](decisions/D-3.md) | Which timing profile does the import give a pattern? | decided; not built | nothing |
| [D-5](decisions/D-5.md) | Does the import keep a block that breaks the spec? | encoded | nothing |
| [D-6](decisions/D-6.md) | Does "frequent" count terminal arrivals? | decided | nothing |
| [D-8](decisions/D-8.md) | Do timetable edits act on the whole service family? | decided | nothing |
| [D-9](decisions/D-9.md) | Where does required recovery time come from? | decided | nothing |
| [D-10](decisions/D-10.md) | Agency questions on C-TRAN's data | decided | nothing |
| [D-11](decisions/D-11.md) | Does editing a shared alignment change every pattern on it? | decided | nothing |
| [D-12](decisions/D-12.md) | What headsign does a new trip get? | encoded | nothing |
| [D-14](decisions/D-14.md) | Does the proposal comparison collect public responses? | decided | nothing |
| [D-15](decisions/D-15.md) | What makes a trip "covered"? | replaced by D-212 | nothing |
| [D-16](decisions/D-16.md) | Where does the runs layer live? | encoded | nothing |
| [D-17](decisions/D-17.md) | What does "the last bus" mean with no deadline? | encoded | nothing |
| [D-18](decisions/D-18.md) | How do live delays meet a diverted trip? | encoded | nothing |
| [D-19](decisions/D-19.md) | Does an alert for one direction reach both? | replaced by D-204 | nothing |
| [D-20](decisions/D-20.md) | Which language does an alert speak? | encoded | nothing |
| [D-21](decisions/D-21.md) | Where is a trip going when nothing signs it? | encoded | nothing |
| [D-22](decisions/D-22.md) | How is a route's rider variant named and ordered? | encoded | nothing |
| [D-23](decisions/D-23.md) | In what order does a stop display list its groups? | encoded | nothing |
| [D-24](decisions/D-24.md) | How does a departure's countdown round? | encoded | nothing |
| [D-25](decisions/D-25.md) | Which streets are the source of truth? | decided | nothing |
| [D-26](decisions/D-26.md) | Which streets can each mode use? | decided | nothing |
| [D-27](decisions/D-27.md) | What does place search find? | decided | nothing |
| [D-28](decisions/D-28.md) | Where do stop and address results meet? | encoded | nothing |
| [D-29](decisions/D-29.md) | What area does a geography release cover? | decided | nothing |
| [D-30](decisions/D-30.md) | What does the streets import call a street, and what class is it? | encoded | nothing |
| [D-31](decisions/D-31.md) | Can a plan add a street the data does not have? | decided | nothing |
| [D-32](decisions/D-32.md) | How does the router walk with no walk table? | encoded | nothing |
| [D-33](decisions/D-33.md) | How does a trip from a place walk? | encoded | nothing |
| [D-34](decisions/D-34.md) | Where does a stop no walk reaches go in the nearby list? | encoded | nothing |
| [D-35](decisions/D-35.md) | Among plans arriving together, where does the rider board and get off? | encoded | nothing |
| [D-36](decisions/D-36.md) | Is geocoding a document on the bus or a service the host calls? | decided | nothing |
| [D-37](decisions/D-37.md) | What names a spot dropped on the map? | encoded | nothing |
| [D-38](decisions/D-38.md) | What may a host do with an external service's answer? | decided | nothing |
| [D-39](decisions/D-39.md) | Do the live feeds move into the manifest's services? | decided | nothing |
| [D-40](decisions/D-40.md) | How long does a planned trip take, and from when? | decided | nothing |
| [D-41](decisions/D-41.md) | What carries a service type's calendar into a fork's period? | decided | nothing |
| [D-42](decisions/D-42.md) | When a route has stopped for the day, how far ahead does a stop look? | decided | nothing |
| [D-43](decisions/D-43.md) | What does a coverage comparison report? | encoded | QAF-20's matching rule |
| [D-44](decisions/D-44.md) | Is a branch of a direction its own thing? | decided; not built | nothing |
| [D-45](decisions/D-45.md) | Which dispatcher edits does the edit document carry beyond the diversion? | decided | nothing |
| [D-46](decisions/D-46.md) | How many more vehicles does a proposal need? | decided | the work-rule policy document (D-50's shape) |
| [D-47](decisions/D-47.md) | Which layers does coverage take? | decided | nothing |
| [D-48](decisions/D-48.md) | Does a stop answer to a name its sign does not carry? | decided | nothing |
| [D-49](decisions/D-49.md) | Where do the basemap's glyphs and sprites come from? | decided | nothing |
| [D-50](decisions/D-50.md) | What are the work rules a run is cut under? | decided | nothing |
| [D-51](decisions/D-51.md) | What does a revenue hour, a revenue mile, and a vehicle cost? | decided | nothing |
| [D-52](decisions/D-52.md) | How is a route-hour's frequency judged and drawn? | encoded | nothing |
| [D-53](decisions/D-53.md) | What ridership data does the planner bring? | decided | nothing |
| [D-54](decisions/D-54.md) | What does a frequency change keep of the trips already there? | decided; not built | nothing |
| [D-55](decisions/D-55.md) | Which members does a service group count on a day? | decided | nothing |
| [D-56](decisions/D-56.md) | Whose clock is the LWW clock? | encoded | nothing |
| [D-57](decisions/D-57.md) | How long does a tombstone live? | encoded | nothing |
| [D-58](decisions/D-58.md) | Which endpoints require a token by default? | decided | nothing |
| [D-59](decisions/D-59.md) | Which JWT claim carries the roles? | decided | nothing |
| [D-60](decisions/D-60.md) | How are JWTs verified (JWKS vs pinned key)? | decided | nothing |
| [D-61](decisions/D-61.md) | Where does the authz policy live and how is it changed? | decided | nothing |
| [D-62](decisions/D-62.md) | Where does a document's schema version live? | decided | nothing |
| [D-63](decisions/D-63.md) | What does an open page do when the deployment moves past it? | decided | nothing |
| [D-64](decisions/D-64.md) | Which devices must the rider apps run well on? | decided | nothing |
| [D-65](decisions/D-65.md) | Do ride-now and the stop display work offline? | decided | nothing |
| [D-66](decisions/D-66.md) | How long is a migration kept? | decided | nothing |
| [D-67](decisions/D-67.md) | Whose feeds does the server's proxy fetch? | decided | nothing |
| [D-68](decisions/D-68.md) | What are ride-now's and the stop display's performance budgets? | decided; replaced by D-187 | nothing |
| [D-69](decisions/D-69.md) | What happens to the original app when host calls move into the engine? | decided | nothing |
| [D-70](decisions/D-70.md) | How do the module runtime's bounds default? | encoded | nothing |
| [D-71](decisions/D-71.md) | May a module register a document under a pinned hash it did not compute? | decided | nothing |
| [D-72](decisions/D-72.md) | Is the release profile tuned for size or for speed? | encoded | nothing |
| [D-73](decisions/D-73.md) | What does a stop's board show for a trip the live feed cancels? | decided | nothing |
| [D-74](decisions/D-74.md) | How is a pick published, and what runs before the first one? | encoded | nothing |
| [D-75](decisions/D-75.md) | How does a published pick come back out? | decided | nothing |
| [D-76](decisions/D-76.md) | What does publishing refuse beyond an invalid chain? | decided | nothing |
| [D-77](decisions/D-77.md) | How do the capabilities a page uses share one network? | decided | nothing |
| [D-78](decisions/D-78.md) | Does the Go server host linked assemblies? | decided; replaced by D-227 | nothing |
| [D-79](decisions/D-79.md) | How is a document version rolled back? | decided | nothing |
| [D-80](decisions/D-80.md) | Where do retired build artifacts live? | decided | nothing |
| [D-81](decisions/D-81.md) | How are op and HTTP contracts versioned? | decided | nothing |
| [D-82](decisions/D-82.md) | Where do the store's backups go, and how often? | decided | nothing |
| [D-83](decisions/D-83.md) | What does each deployed device report, and to where? | decided | nothing |
| [D-84](decisions/D-84.md) | Who takes a build first, and what counts as healthy? | decided | nothing |
| [D-85](decisions/D-85.md) | Are the generated assembly crates committed? | decided | nothing |
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
| [D-105](decisions/D-105.md) | What does a fork carry when a feed's calendars are all dated? | decided | nothing |
| [D-106](decisions/D-106.md) | Does a recovery rule judge a trip that continues with no wait? | encoded | nothing |
| [D-107](decisions/D-107.md) | Which routes may interline when the rules do not say? | encoded | nothing |
| [D-108](decisions/D-108.md) | What is an hours budget? | unanswered | hours against a budget: `ui-service-design` question 2, `ui-service-change` (the proposal) |
| [D-109](decisions/D-109.md) | What does a suggested blocking change? | encoded | nothing |
| [D-110](decisions/D-110.md) | Which waits does a recovery requirement judge, and after which trip? | encoded | nothing |
| [D-111](decisions/D-111.md) | When does a dated service read as a weekly rule? | encoded | nothing |
| [D-112](decisions/D-112.md) | Which weekly service types does a fork carry? | encoded | nothing |
| [D-113](decisions/D-113.md) | Where does the sync store live in production? | decided | nothing |
| [D-114](decisions/D-114.md) | What does the clock answer in the hour before a fall-back day's anchor? | encoded | nothing |
| [D-115](decisions/D-115.md) | Must a document be readable without the app? | decided | nothing |
| [D-116](decisions/D-116.md) | Which managed object store does a deployment run on? | decided | nothing |
| [D-117](decisions/D-117.md) | Who deletes from the store, and when? | decided | nothing |
| [D-118](decisions/D-118.md) | Where does a server read its inputs at boot? | decided | nothing |
| [D-119](decisions/D-119.md) | What does the server serve when the store is unreachable? | decided | nothing |
| [D-120](decisions/D-120.md) | Does compaction drop a record whose dates have passed? | unanswered | nothing |
| [D-121](decisions/D-121.md) | Does the store keep the edit document whole per commit, or record by record? | decided by D-190 | nothing |
| [D-122](decisions/D-122.md) | Is the dispatcher's day in scope? | decided | nothing |
| [D-123](decisions/D-123.md) | Do trip, block, and run ids survive from one pick to the next? | unanswered | nothing |
| [D-124](decisions/D-124.md) | Does this repo publish its cancels and detours as GTFS-rt? | decided; deferred | nothing |
| [D-125](decisions/D-125.md) | Does an edit record carry its source system's id? | unanswered | nothing |
| [D-126](decisions/D-126.md) | Is the runs layer shaped by TODS? | decided | nothing |
| [D-127](decisions/D-127.md) | Where does operations history come from? | decided | nothing |
| [D-128](decisions/D-128.md) | Does this repo produce TIDES? | decided | nothing |
| [D-129](decisions/D-129.md) | Where does the authored GTFS-rt reach riders? | unanswered | nothing (publishing is deferred, D-124) |
| [D-130](decisions/D-130.md) | Where does a display's identity live on the device? | decided | nothing |
| [D-131](decisions/D-131.md) | Does the MVP deployment need a server? | decided | nothing |
| [D-132](decisions/D-132.md) | Can a serverless deployment follow the agency's GTFS URL? | decided | nothing |
| [D-133](decisions/D-133.md) | What counts as a just-missed connection? | encoded | nothing |
| [D-134](decisions/D-134.md) | How does the network rank its longest waits? | encoded | nothing |
| [D-135](decisions/D-135.md) | Which browsers must the generated UIs support? | decided | nothing |
| [D-136](decisions/D-136.md) | What does the worklist call weak? | encoded | nothing |
| [D-137](decisions/D-137.md) | How are a plan's service combinations shown as day types? | encoded | nothing |
| [D-138](decisions/D-138.md) | Does a fork keep the dates outside its period? | decided | nothing |
| [D-139](decisions/D-139.md) | Is there a base network the plans fork from? | decided | nothing |
| [D-140](decisions/D-140.md) | What date does an imported feed pick on? | decided | nothing |
| [D-141](decisions/D-141.md) | What does an open plan compare against by default? | decided | nothing |
| [D-142](decisions/D-142.md) | Does serving a stop act on the direction or on one pattern? | decided; not built | nothing |
| [D-143](decisions/D-143.md) | What does redrawing a line do to running time? | decided; not built | nothing |
| [D-144](decisions/D-144.md) | What does closing a stop do to a plan? | decided; replaced by D-157 | nothing |
| [D-145](decisions/D-145.md) | What makes a stop the same stop over time? | decided | nothing |
| [D-146](decisions/D-146.md) | Does this repo own the stop data? | decided | nothing |
| [D-147](decisions/D-147.md) | Do stops live inside each plan or beside the plans? | decided; its picks' part replaced by D-157 | nothing |
| [D-148](decisions/D-148.md) | How fine are a stop's dates? | decided | nothing |
| [D-149](decisions/D-149.md) | What format is the stops document? | decided; replaced by D-153 | nothing |
| [D-150](decisions/D-150.md) | What does importing a feed do to a stops timeline that exists? | decided | nothing |
| [D-151](decisions/D-151.md) | What does the export write when a stop changes inside its window? | decided | nothing |
| [D-152](decisions/D-152.md) | Who owns the data, and how strict is a write? | decided | nothing |
| [D-153](decisions/D-153.md) | How is the stops document stored? | decided; its event types replaced by D-157 | nothing |
| [D-154](decisions/D-154.md) | What does the stops document say about amenities? | decided | nothing |
| [D-155](decisions/D-155.md) | What happens when a closure leaves a trip short? | decided; replaced by D-157 | nothing |
| [D-156](decisions/D-156.md) | Does the network document change for the stops? | decided; replaced by D-157 | nothing |
| [D-157](decisions/D-157.md) | Whose lifecycle is a stop's? | decided | nothing |
| [D-158](decisions/D-158.md) | Where does a stop's site survey live? | decided | nothing |
| [D-159](decisions/D-159.md) | What is a stop request, and how does it reach the work order system? | decided | nothing |
| [D-160](decisions/D-160.md) | Are a stop's signs, posts, and shelters records of their own? | decided | nothing |
| [D-161](decisions/D-161.md) | When a survey and the agency's other data disagree, which wins? | decided | nothing |
| [D-162](decisions/D-162.md) | Where do photos and drawings live, and how does a record name one? | decided | nothing |
| [D-163](decisions/D-163.md) | Who is a survey's surveyor? | decided | nothing |
| [D-164](decisions/D-164.md) | Does the store log hide who wrote each change from callers without audit? | unanswered | nothing |
| [D-165](decisions/D-165.md) | How does a request reach the work order system before its API is wired? | decided | nothing |
| [D-166](decisions/D-166.md) | What is the smallest part of the stops document two writers can change at once? | decided; partly built | the stops merge in `storage-model` |
| [D-167](decisions/D-167.md) | Do authored documents keep the fields they do not know? | decided | nothing |
| [D-168](decisions/D-168.md) | Where does personal data live, and how is it removed when the law requires? | decided | nothing |
| [D-169](decisions/D-169.md) | Is a document's canonical form serde's field order, or RFC 8785? | decided | nothing |
| [D-170](decisions/D-170.md) | Are the migration steps from before the first deployment kept? | decided | nothing |
| [D-171](decisions/D-171.md) | How does a UI at its own URL reach the server, and how is it rolled back? | decided | nothing |
| [D-172](decisions/D-172.md) | Where do the server's settings live? | decided | nothing |
| [D-173](decisions/D-173.md) | How does a stop the stops app made reach a plan? | decided | nothing |
| [D-174](decisions/D-174.md) | How are code comments and the developer documents written? | decided | nothing |
| [D-175](decisions/D-175.md) | Whose clock does the runner read for $TODAY and $NOW_SECS? | decided | nothing |
| [D-176](decisions/D-176.md) | Does a user's own selection assemble capabilities at wire time? | decided | nothing |
| [D-177](decisions/D-177.md) | What does a journal keep, and where? | encoded provisionally | nothing |
| [D-178](decisions/D-178.md) | Is a page's journal on by default? | encoded provisionally | nothing |
| [D-179](decisions/D-179.md) | Which module does replay run a journal's call on? | encoded provisionally | nothing |
| [D-180](decisions/D-180.md) | What is service design for, beside service change? | decided; not built | nothing |
| [D-181](decisions/D-181.md) | What is the network review called? | unanswered | the review UI's title and id |
| [D-182](decisions/D-182.md) | Does the server compress its own static files? | encoded | nothing |
| [D-183](decisions/D-183.md) | What policy do the rider pages send, and who writes the export's `_headers`? | encoded | nothing |
| [D-184](decisions/D-184.md) | What color is the text on a route's badge? | encoded | nothing |
| [D-185](decisions/D-185.md) | Are the Go host's own documents (`build`, `manifest`) typed and hashed by a module? | decided | nothing |
| [D-186](decisions/D-186.md) | What does a build that reads one version ahead write over a document in that version? | decided; built | nothing |
| [D-187](decisions/D-187.md) | Is performance held to fixed budgets? | decided; not built | nothing |
| [D-188](decisions/D-188.md) | Is the migration module cut by use? | decided | nothing |
| [D-189](decisions/D-189.md) | Do the staff pages work offline, and queue their writes? | decided; built and dogfooded on C-TRAN | nothing |
| [D-190](decisions/D-190.md) | How does the agency's record store the data people edit? | decided; the disruptions, stops, and plans logs built | nothing |
| [D-191](decisions/D-191.md) | Is a service-change draft part of the agency's record, and what does publishing mean? | decided; freezing and adopting built on the server | nothing |
| [D-192](decisions/D-192.md) | Are the edit document's existing names renamed to "disruptions"? | unanswered | nothing |
| [D-193](decisions/D-193.md) | What does a revert write back? | encoded | nothing |
| [D-194](decisions/D-194.md) | Who may read a log's entries? | encoded | nothing |
| [D-195](decisions/D-195.md) | How does the server keep and check a log's checkpoints? | encoded | nothing |
| [D-196](decisions/D-196.md) | When is work dogfooded? | decided | nothing |
| [D-197](decisions/D-197.md) | How is a log built for kinds that two capabilities own? | decided; built | nothing |
| [D-198](decisions/D-198.md) | What does a revert do to a record the reverted entry created? | decided | nothing |
| [D-199](decisions/D-199.md) | Does the record store the documents the server derives from its inputs? | decided; built | nothing |
| [D-200](decisions/D-200.md) | How does a stored derived document name the inputs it was made from? | encoded | nothing |
| [D-201](decisions/D-201.md) | How does the server serve freezing and adopting? | encoded | nothing |
| [D-202](decisions/D-202.md) | Where does the kind table live, and what does it govern? | encoded | nothing |
| [D-203](decisions/D-203.md) | How does the store describe itself? | encoded | nothing |
| [D-204](decisions/D-204.md) | Does an alert's direction_id scope it to one direction? | decided; built | nothing |
| [D-205](decisions/D-205.md) | Who turns an alert's local times into GTFS-rt's Unix seconds? | decided; not built | nothing |
| [D-206](decisions/D-206.md) | Who writes service alerts? | decided; not built | nothing |
| [D-207](decisions/D-207.md) | How do dispatch and communications staff share the alerts workspace? | decided; not built | nothing |
| [D-208](decisions/D-208.md) | Which alert fields does authoring carry first? | decided; built | nothing |
| [D-209](decisions/D-209.md) | Are service alerts and disruptions one UI? | decided; not built | nothing |
| [D-210](decisions/D-210.md) | What does an alert template fill in? | decided; built | nothing |
| [D-211](decisions/D-211.md) | How is the service alerts document shaped? | encoded | nothing |
| [D-212](decisions/D-212.md) | Is a trip's coverage this repo's answer? | decided; built | nothing |
| [D-213](decisions/D-213.md) | Where does the disruptions UI read today's cancels? | decided; not built | nothing |
| [D-214](decisions/D-214.md) | Can dispatch keep internal notes here? | decided; built on the bus and the server | nothing |
| [D-215](decisions/D-215.md) | Where does the disruptions summary for a day live? | replaced by D-218 | nothing |
| [D-216](decisions/D-216.md) | How does an alert explain a trip the live feed cancels? | decided; built | nothing |
| [D-217](decisions/D-217.md) | How is the notes document shaped? | encoded | nothing |
| [D-218](decisions/D-218.md) | Which capability answers what is disrupted on a day? | decided; built | nothing |
| [D-219](decisions/D-219.md) | How is a day's disruptions answer shaped? | encoded | nothing |
| [D-220](decisions/D-220.md) | Which riders does an alert about a disruption inform? | encoded | nothing |
| [D-221](decisions/D-221.md) | Does a station's stop_id in an alert reach its bays? | encoded | nothing |
| [D-222](decisions/D-222.md) | Which time does a write sent late carry? | decided; built | nothing |
| [D-223](decisions/D-223.md) | Does every install download the staff pages' modules? | encoded | nothing |
| [D-224](decisions/D-224.md) | What does a server copy started while the bucket does not answer serve? | encoded | nothing |
| [D-225](decisions/D-225.md) | Which object stores does the server talk to, and through what code? | decided | nothing |
| [D-226](decisions/D-226.md) | What is a capability? | decided | nothing |
| [D-227](decisions/D-227.md) | Does the server link an assembly, as a UI does? | decided | nothing |
| [D-228](decisions/D-228.md) | What may a host do with an op's answer? | decided | nothing |
| [D-229](decisions/D-229.md) | Does the record's integrity encode the domain? | decided | nothing |
| [D-230](decisions/D-230.md) | When an adoption breaks a live reference, does it refuse or warn? | decided | nothing |
| [D-231](decisions/D-231.md) | Does compaction drop a retired record that a live reference names? | decided | nothing |
| [D-232](decisions/D-232.md) | How are references declared, and which stage may name which? | encoded | nothing |
| [D-233](decisions/D-233.md) | When does the receive path refuse a write whose reference is broken? | encoded | nothing |
| [D-234](decisions/D-234.md) | Over which service dates does an undated record's reference resolve? | decided | nothing |
| [D-235](decisions/D-235.md) | Where do the worked examples' fixtures live, and what may depend on them? | decided | nothing |
| [D-236](decisions/D-236.md) | Which modules do the test runners build? | decided; built | nothing |
| [D-237](decisions/D-237.md) | Where do a pick's checks run? | decided; built | nothing |
| [D-238](decisions/D-238.md) | On which date must an open stop request's stop not be retired? | decided | nothing |
| [D-239](decisions/D-239.md) | How does the kernel load a kind without naming its library? | decided; built | nothing |
| [D-240](decisions/D-240.md) | While a log's boundary moves, do writes to its kinds pause? | unanswered | `log-boundaries.md` items 2 and 3 |
| [D-241](decisions/D-241.md) | What must hold before a split runs? | unanswered | `log-boundaries.md` item 3 (the split) |
| [D-242](decisions/D-242.md) | What runs a move of a log's boundary, and when? | unanswered | `log-boundaries.md` item 3 |
| [D-243](decisions/D-243.md) | In which order does the manifest list the capabilities and their ops? | encoded | nothing |
| [D-244](decisions/D-244.md) | What does the kind-name gate match, and do the per-capability module flags retire? | unanswered | `kind-registration.md` item 4 |
| [D-245](decisions/D-245.md) | Does a failed input stop the answers that read it? | encoded | nothing |
| [D-246](decisions/D-246.md) | When does a page read a failed input again? | encoded | nothing |
| [D-247](decisions/D-247.md) | Do the staff pages follow the deployment's network as it moves? | encoded | nothing |
| [D-248](decisions/D-248.md) | D-239 costs each module 2 to 5 percent: keep it? | unanswered | `kind-registration.md` (the release gate at the unit's end) |
| [D-249](decisions/D-249.md) | Where do the HTTP contract's wrapper shapes come from when it is generated? | unanswered | `http-contract-generated.md` item 4 |
| [D-250](decisions/D-250.md) | Which distance do the libraries share? | decided | nothing |
| [D-251](decisions/D-251.md) | May transit-network depend on a library that holds no kind? | unanswered | `shared-dates-and-distance.md` |
