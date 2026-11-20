# A UI host's inputs, declared

A generated UI's host has three jobs. Two of them are declared and run
by shared code:

- the answer graph (`web/src/uis/answers.ts`, D-176) runs its op answers;
- the outbox (`web/src/outbox.ts`, D-189) runs its writes.

The third job is still written by hand in each host: its inputs. An
input is a document or feed from outside the page. Examples are the
served network, the edit document, a log's document, the live feed, and
the deployment's manifest. Each host writes its own `boot`, `poll`,
`release`, and readers (`readEdits`, `readDocuments`, `fetchLive`,
`followDeployment`, `obtainServed`). It chooses for itself how often to
read, what to keep when a read fails, and what to say. The hosts are 530
to 1,384 lines each (2026-11-20).

Every row of a UI's failure table (`docs/architecture/generated-uis.md`)
is an input that fails. So the inputs are where a page's resilience is
decided, and today they are decided six times. This unit makes the
inputs declared and run by the kit, like the answers and the writes. A
new UI then gets the same behavior through an outage without writing
it.

## Scope

1. **Inputs are declarations.** A host names each input with these
   properties:
   - where it comes from (the server's path, the deployment's feed, a
     URL the rider gave);
   - how often it is read;
   - what the page keeps when a read fails;
   - the age past which the input is old.

   Where that age is a matter of policy, the agency names it, as in
   QAF-129's frozen live feed. The kit runs every input with one
   poll, while the page is visible.
2. **Every read has a time limit.** No request in `web/src/` has a
   time limit today. A request that never answers holds the poll, and
   the page keeps its last answers with no label. The kit stops a read
   at its limit and counts it as failed.
3. **A read has one of three outcomes.** A read answers a document,
   "none kept", or "failed since <time>". It never answers null for
   both "none kept" and "failed". This closes QAF-131. The input's
   state reaches the answer graph, so a frame that reads a failed
   input says so, as a starved frame does now. This closes QAF-130.
4. **Reads come back gently after an outage.** When the server answers
   again, the kit follows its `Retry-After` and spreads the reads by a
   random delay, so every open page does not read at the same moment.
5. **Inputs feed the answer graph directly.** An input names the graph
   inputs it changes. The host no longer touches the graph by hand
   when a document changes.
6. **The server's words, and the server's age.** An input's states use
   the words of the server's degraded mode
   (`docs/architecture/hosts.md`, "The rules every host keeps"). A
   page that reads a stale answer from the server shows the server's
   as-of time, not the time of its own read.
7. **A host contract suite.** One set of scenarios runs against the
   server and the browser:
   - the upstream does not answer;
   - it answers slowly;
   - it answers again;
   - a head moves while the host holds a copy.

   Each host must give the same states and labels. The suite holds
   the browser to the rules in `hosts.md`, as `just store-contract`
   holds a bucket to the store's.
8. **The failure table follows the declarations.** A gate in
   `just test` fails when a UI declares an input that its failure
   table has no row for.
9. **The docs.** `docs/architecture/generated-uis.md` (the recipe's
   step for inputs), `docs/architecture/web.md`,
   `docs/architecture/hosts.md` ("Where a host does not keep these
   rules"), and each UI's failure table, with the "page knows by"
   column naming the input's state.

The declarations follow `docs/CONVENTIONS.md`, "Traceable":
- the inputs are one plain list at the top of each host;
- an input's state names the input;
- the journal records which input changed and which answers ran
  again because of it.

The engine's per-kind readers (`servedStops`, `servedNotes`, and the
others) become one reader in [kind-registration.md](kind-registration.md).
This unit can start before that one lands, over the readers as they
are.

## Done when

- The six hosts declare their inputs. A web gate in `just test` finds
  no `fetch(` and no `whileVisible` in `web/src/uis/*/host.svelte.ts`.
- An e2e spec: a server that never answers a read turns the page's
  label to stale within the input's time limit.
- An e2e spec: a page whose stops read fails says the read failed. It
  does not say "publishes no stops yet".
- QAF-130 and QAF-131 are deleted from `docs/QA-FINDINGS.md`.
- The failure-table gate runs in `just test`.
- The host contract suite passes against the server and the browser.
