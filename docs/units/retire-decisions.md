# Retire built decisions

A decision file holds an open question, or an answer that is not built
yet (`docs/DECISIONS.md`, "The rules"). About 150 decided entries,
most of them built, and about 80 provisional entries predate that rule.
About 1,200 citations of their ids sit in code, tests, and docs.

## The work

1. **Retire the decided, built entries, a topic at a time.** For each
   one, confirm that its pins exist and pass. Move its substance to its
   homes: the property to the test that pins it, and the rule, its
   reason, and each rejected alternative to the doc or comment that
   owns the topic. Then delete the entry, its summary row, and every
   citation of its id (`rg -n -w 'D-N'`).
2. **Delete the replaced entries**, with their citations.
3. **Review the provisional entries with the user.** A confirmed
   answer becomes Decided, and then retires by step 1. A corrected one
   rewrites the tests that pin it.

## Done when

- `docs/decisions/` holds only unanswered entries, provisional
  entries, and decided entries whose answer is not built.
- No code, test, or doc cites an id whose entry is gone.
