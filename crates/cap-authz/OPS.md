# authz

Authorization: may this identity perform this action on this pointer? Authentication is the IdP's — the host verifies the JWT (JWKS or a pinned key, RS256/ES256 only, iss/aud/exp checked, D-60) and reads the roles claim at the configured path (D-59). This module owns only the decisions: authorize evaluates the POLICY document the host passes per call (the store's policy pointer, seeded from the -policy source at first boot, changed by audited CAS commits — D-61) and answers fail-closed — no policy, no subject, no matching rule is a deny, never an allow; the verdict never depends on rule order. effective projects the policy onto ONE identity — the caller's roles and merged capabilities, never other roles or rules — which is all a client may learn (GET /permissions). The policy blob itself is never served to clients. transit-authz is the domain library; the module is stateless, like cap-plans. Assemble for any server whose writes must be role-gated.

## authorize

The one question: {identity: {sub, roles}, action: "read"|"write"|"audit", pointer, policy} → {decision: {allow, matched_rule, reason}}. A rule matches when the identity holds one of its roles AND its pointers/actions match ("*" or empty = all; empty roles = never). Default policy shape: public read, authenticated write on the operator documents (D-58); the pointer log additionally needs read, and shows writer identities only with an audit rule.

## effective

The policy as it applies to THIS identity: {identity, policy} → {roles, capabilities: [{pointer, actions}]} — matching rules merged per pointer, sorted, wildcards symbolic. The client's view (GET /permissions); contains nothing about other users or roles.
