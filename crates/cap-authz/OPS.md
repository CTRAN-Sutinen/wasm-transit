# authz

Authorization: may this identity perform this action on this pointer?

Authentication belongs to the identity provider. The host verifies the JWT and reads the roles claim at the configured path. The verification rules are:

- the key comes from a JWKS or a pinned key;
- only RS256 and ES256 are accepted;
- `iss`, `aud`, and `exp` are checked.

This module owns only the decisions. `authorize` evaluates the policy document that the host passes on each call. The policy is the store's policy pointer. It is seeded from the `-policy` source at first boot, and it changes by audited CAS commits.

`authorize` answers fail-closed. No policy, no subject, or no matching rule is a deny, never an allow. The verdict never depends on rule order.

`effective` projects the policy onto one identity: the caller's roles and merged capabilities. It never shows other roles or rules. This is all that a client may learn (`GET /permissions`). The policy blob itself is never served to clients.

transit-authz is the domain library. The module is stateless, like cap-plans.

Assemble this capability for any server whose writes must be role-gated.

## authorize

The one question: `{identity: {sub, roles}, action: "read"|"write"|"audit", pointer, policy}` gives `{decision: {allow, matched_rule, reason}}`.

A rule matches when both of these are true:

- The identity holds one of the rule's roles.
- The rule's pointers and actions match. `"*"` or empty means all. Empty roles means never.

The default policy shape is public read and authenticated write on the operator documents. The pointer log also needs read. It shows writer identities only with an audit rule.

## effective

The policy as it applies to this identity: `{identity, policy}` gives `{roles, capabilities: [{pointer, actions}]}`.

Matching rules are merged per pointer and sorted. Wildcards stay symbolic.

This is the client's view (`GET /permissions`). It contains nothing about other users or roles.

## validate

What a policy write must pass. The sync server asks this before it commits `POST /pointers/policy`.

- `issues` makes the policy wrong, and refuses the write: an action other than `read`, `write`, `audit`, or `*` (`unknown_action`); a rule with a blank name (`unnamed_rule`, by its place); two rules with one name (`duplicate_rule_name`).
- `warnings` are consequences a person should see, and never refuse: a policy that grants no role a write on the policy (`no_policy_writer`), which nobody could change again but by a new seed.
- With no issue, the answer is the canonical document and its `document_hash`. The server stores exactly these bytes.

A field the policy carries that this build does not know is kept, and named in `unknown_fields` (D-167). A policy in a newer version is refused, naming both versions.

