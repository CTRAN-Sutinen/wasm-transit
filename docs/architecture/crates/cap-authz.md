# cap-authz

The authorization capability: may this identity perform this action on this pointer?
- `authorize` answers the question, and `effective` gives the policy as it applies to one identity. It is stateless, like cap-plans.
- `validate` checks a policy write: an unknown action, a blank rule name, or a name used twice refuses it, and a policy that grants no role a write on the policy is accepted with a warning. It answers the canonical bytes the server stores. The kind table names it as the policy's check (D-202).
- The host verifies the IdP's JWT and pushes the policy document on each call.
- The verdict is fail-closed and never depends on rule order. transit-authz is the library, and it is property-tested.
- The policy is a store document (kind `policy`, D-61).
