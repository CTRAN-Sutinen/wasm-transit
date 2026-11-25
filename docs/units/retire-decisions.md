# Retire built decisions

A decision file holds an open question, or an answer that is not built
yet (`docs/DECISIONS.md`, "The rules"). The decided entries left are
answers that are not built. About 80 provisional entries predate that
rule.

## The work

1. **Review the provisional entries with the user.** A confirmed
   answer becomes Decided. Once its pins exist, it retires: its
   substance moves to its homes, and the entry, its summary row, and
   every citation of its id go (`docs/DECISIONS.md`, "A built decision
   leaves this record"). A corrected answer rewrites the tests that pin
   it.

## Done when

- `docs/decisions/` holds only unanswered entries and decided entries
  whose answer is not built.
- No code, test, or doc cites an id whose entry is gone.
