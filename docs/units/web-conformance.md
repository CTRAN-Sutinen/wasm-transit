# Web conformance

The generated UIs, held to WCAG 2.2 AA and to the platform's current
practice (the `modern-web-guidance` corpus: accessibility, motion,
tables, forms, performance, security) in both deployment shapes. The
rider UIs come first: they are the MVP, deployed serverless (D-131),
and riders are the public.

The rider UIs and the picker are held by a gate in `just e2e`
(`docs/TESTING.md`, expectation 7). What is left is below.

## Left to do

### The other generated UIs

The stops and service-change pages are not in `web/e2e/a11y.spec.mjs`,
and they send no Content-Security-Policy (D-183). Walk each under axe
and under a policy, then add them to the gate and to
`server/headers.go`'s `riderPages`. Service design is outside this unit.

### Route text color

A ride-now route badge shows white text on the route's color. The
directory does not carry `route_text_color`, so the badge cannot use the
feed's own pair (the stop display's does). An authored pair below 4.5:1
shows as given (D-184) and is a finding about the agency's data. Carry
`route_text_color` through the directory's route rows, then read it in
the badges.

### A person with a screen reader

A screen reader by hand: VoiceOver and TalkBack on ride-now on a phone,
the trip-planning and map-picking flows past boot, and the stop display
on its Pi 4 at the display's distance. No gate covers this.

## Done when

- `web/e2e/a11y.spec.mjs` covers every generated UI that remains, and
  `server/headers.go` sends each one's policy.
- Ride-now's route badges read the feed's `route_text_color`, pinned
  in the directory's dispatch test and the a11y spec.
- A dogfood session with a screen reader on ride-now on a phone, recorded
  here or in `docs/QA-FINDINGS.md`.
