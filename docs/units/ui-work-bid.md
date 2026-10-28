# UI: bidding on work

A generated UI for operators to bid on work: see the runs a pick
offers, what each pays and when it reports, and bid in seniority
order. Bidding sits on the runs layer (`docs/ROADMAP.md` 7),
which `docs/units/ui-runcut.md`'s runs questions build first, and
on data the bus does not hold.

It follows the generated-UI procedure (`docs/ARCHITECTURE.md`,
"Generated UIs"). It is at step 1, and it cannot reach step 3: no
op answers any of its questions yet.

## Questions, in the order asked

1. Which runs does this pick offer?
2. For each run: when and where it reports and clears, its pieces,
   its platform time, and what it pays.
3. Which runs are still open when my turn comes?
4. Bid, and see the bid in seniority order.

## Op inventory (draft)

| # | state |
|---|---|
| 1 | gap, QAF-11 (no runs layer) |
| 2 | gap, QAF-14 (no run view) and QAF-12 (no work-rule document) |
| 3, 4 | gap: no workforce or seniority data, QAF-17; a bid is a new shared document (a sync-document change, as QAF-36 is) |

## Done when

- The runs layer exists on the bus.
- The session has run, and the inventory names an op or a finding
  for every question.
- The answer graph table is in this file.
- The UI lands per step 8 with `e2e/ui-work-bid.spec.mjs`, and this
  file is deleted, or edited down to its blocked questions.
