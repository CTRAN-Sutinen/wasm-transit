# Linked assembly — what is left

The capabilities a generated UI uses link into ONE module over ONE
kernel-owned registry (D-77; `docs/ARCHITECTURE.md`, "The capability
bus" and "Bus rules"): each UI's page loads its assembly and holds
each network version once, the four gates run in `just test`, no
capability's `Request` carries a load, and the live document reaches
the board and the router without crossing to the page and back. The
C-TRAN record is in `docs/units/runtime-cost-gate.md`. What is left
is the server.

## Scope

- **The server's modules** (Blocked: D-78). The server runs
  single-capability modules today (`module-sync`, `module-network`,
  …, copied to `web/pkg/`); whether it links the long-lived ones, or
  everything, is D-78's answer.

The Law 3 re-cuts of cap-analysis, cap-directory, and cap-streets
follow this unit (`docs/units/README.md`, queued).

## Done when

- D-78 is answered and the server runs what it says.
