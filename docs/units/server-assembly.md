# The server links an assembly

The server links its capabilities into one assembly, as each UI does
(D-227, which replaces D-78). Today it runs single-capability modules,
one flag each (`-wasm`, `-plans-wasm`, `-authz-wasm`, and the others).
A flow that needs two capabilities is composed in Go between two
modules. With one assembly, the server is a host like the others
(`docs/architecture/hosts.md`): one module, one registry, and a
document registered once.

## Scope

1. **The server's assembly.** `gen-assemblies` writes
   `module-server` from a list of the capabilities the server hosts.
   The list is one file, read by gen-assemblies and by the server
   (`docs/CONVENTIONS.md`, "Traceable"). `just caps` builds it, and
   `independence.mjs` checks it like a UI's assembly.
2. **Instances, kept as they are.** D-78's reasons stay, as instances
   of the one assembly:
   - authz runs in an instance of its own, so a trap in another
     capability never takes the fail-closed check with it;
   - an import or a publish runs in a fresh instance, so the memory it
     used goes with it;
   - the long-lived instance holds the sync, plans, stops, alerts,
     notes, and log documents, each registered once. It keeps the bus
     worker's memory rule (`docs/architecture/web.md`, "The bus
     worker"): it evicts every document no head names, and an instance
     past its ceiling is replaced at the next op and registers its
     documents again.
3. **The pin.** The manifest's `wasm_hash` names the sync module
   today, and a page checks it. With one assembly, it names the
   assembly, and it changes when any linked capability changes. If
   that changes what a page refuses, a proposal goes to the user
   first.
4. **One flag.** `-wasm` names the assembly. The per-capability flags
   go, with their entries in `server/dev.toml`, `local.example.toml`,
   the e2e servers, and the image.
5. **The docs.** `docs/architecture/server.md` (the module list, and
   the Why "Single-capability modules, fresh per import"),
   `docs/architecture/bus.md` (the two kinds of module),
   `AGENTS.md` (the server's hosted capabilities, and `just caps`),
   and `DEPLOYMENTS-SERVER.md` (the flags).

## Done when

- The server boots on `module-server` alone, and `go test -race`
  passes.
- A Go test: a trap in a capability during a write leaves authz
  answering.
- A Go test: an import's instance is gone after the import, and the
  long-lived instance's memory does not grow with it.
- The per-capability flags are gone from the server and every
  settings file.
