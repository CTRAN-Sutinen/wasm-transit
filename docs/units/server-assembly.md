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
     notes, and log documents, each registered once. A request
     registers nothing it already holds. It evicts every document no
     head names (the kernel's evict, op 257).
   - **A refresh runs beside the instance, never in a request's path.**
     When the long-lived instance passes its memory ceiling, the host
     starts a fresh instance of the same compiled assembly and
     registers the current heads' documents in it. A write that lands
     during the refresh moves a head, so before the swap the host
     registers each head that moved since the refresh began. It then
     swaps the fresh instance in for the next request. The old instance finishes
     the call it holds, and then closes. A trap replaces the instance
     the same way.
   - The cost of a request does not depend on traffic or on how often
     the host refreshes. A refresh is started by memory growth, not by
     the number of requests.
3. **The pin.** The manifest's `wasm_hash` names the sync module
   today. No page or tool reads it. One Go test
   (`server/store_mode_test.go`) checks only that the manifest has it. With one assembly, it names
   the assembly, and it changes when any linked capability changes.
   Nothing a page refuses changes with it.
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
- A Go test: requests sent during a refresh are answered with no
  delay from the refresh, and none is answered by the old instance
  after the swap.
- The per-capability flags are gone from the server and every
  settings file.
