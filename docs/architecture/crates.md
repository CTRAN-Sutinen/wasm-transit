# The crates

Each crate's home is its own `//!` crate comment (`src/lib.rs`): what
it answers, the rules other code depends on, and why, with the decision
ids. A capability's ops are in its `OPS.md`
(`node tools/caps-run.mjs tools <id>`). `just docs` draws which crate
depends on which (`architecture.html`).

## Every module

Every module is the raw rustc output of an assembly crate. It has the
same four flat exports, with no wasm-bindgen and no host glue
([bus.md](bus.md), "The kernel: cap-abi").
