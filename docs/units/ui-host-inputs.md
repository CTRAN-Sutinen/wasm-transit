# A UI host's inputs, declared

A generated UI's host has three jobs, and shared code declares and
runs each of them:

- the answer graph (`web/src/uis/answers.ts`, D-176) runs its op answers;
- the outbox (`web/src/outbox.ts`, D-189) runs its writes;
- the inputs (`web/src/uis/inputs.ts`) read the documents and feeds
  from outside the page.

Each UI declares its inputs in its `ui.json` (`inputs`), and its host
gives each one a reader. The kit reads every input with one poll while
the page is visible, and stops each read at its time limit. A read
answers a document, "none kept", or "failed since <time>". The design
is in `docs/architecture/generated-uis.md` (kit's "The inputs") and
`docs/architecture/web.md` ("A UI's inputs are declared as data").
Its provisional decisions are D-245, D-246, and D-247.

The code, the laws, the gates, the specs, and the docs are written.
The build and `just test` pass, with the host contract suite on both
sides. What is left is the browser gate, the findings, and the release
gates.

## Scope

3. **The browser gate.** `just e2e` passed 148 of 150 in its last full
   run. The two failures, `ui-disruptions` "an alert from a gap…" and
   `ui-stops` "a stop's life…", hit the 20 s test timeout while the
   machine was loaded, and each passed alone earlier. Run the full gate
   once more on a quiet machine.
4. **The findings.** Delete QAF-130 and QAF-131 from
   `docs/QA-FINDINGS.md`, with the pins in item 3. QAF-133 is answered
   in part: after a write is sent, a staff page reads the manifest at
   once. Pin that in `offline-staff`, then delete it, or reword it to
   what is left.
5. **The release gates** (D-236): `just test-release` and
   `just e2e-release`, once items 3 and 4 pass.

A live feed that answers with old data (QAF-129) is the finding's own
work. An input can name `old_after_s`, and its reader can answer the
feed's own time as `asOfMs`. No live feed does either yet, and the
agency has not named the age.

## Done when

- `just check` passes, and the release gates pass.
- QAF-130 and QAF-131 are deleted, and QAF-133 is deleted or reworded.
