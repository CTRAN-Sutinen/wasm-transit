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

### Checks not yet run on this work

- `web/e2e/deploy-builds.spec.mjs`. It serves a store-mode build, which
  now sends the headers and gzip. It needs port 8085, which was in use
  when the rest of `just e2e` ran.
- `web/e2e/published-picks.spec.mjs`, "a planner publishes a fork from
  service design", failed on its refusal text. It is a service design
  flow, and no change here touches it. Run it on a clean tree to learn
  whether it failed before.
- The whole `just test` gate. The Go suite, the new specs, and
  `tools/docs-site.test.mjs` pass. The Rust suites did not run, because
  no Rust changed.

### Delivery, on a real deployment

- Cloudflare compresses text types. Check on a real deployment that it
  also compresses `application/wasm`. If it does not, say so in
  `DEPLOYMENTS.md`, "Headers every host must send".
- Check each rider page's `Content-Security-Policy` against a real
  deployment's live feeds and tiles, in the browser console of a phone
  and of the Pi 4 display. A blocked host shows there as a violation.

### A person with a screen reader

A screen reader by hand: VoiceOver and TalkBack on ride-now on a phone,
the trip-planning and map-picking flows past boot, and the stop display
on its Pi 4 at the display's distance. No gate covers this.

## Done when

- `web/e2e/a11y.spec.mjs` covers every generated UI that remains, and
  `server/headers.go` sends each one's policy.
- A dogfood session with a screen reader on ride-now on a phone, recorded
  here or in `docs/QA-FINDINGS.md`.
- The real-deployment checks above, recorded in `DEPLOYMENTS.md`.
