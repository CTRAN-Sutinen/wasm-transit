# A guide to wasm-transit

This guide explains what wasm-transit is, how it works, and why it is
built the way it is. It is written for people, in plain language.

## Who this guide is for

- **People at a transit agency** who want to know what the software
  does for riders and planners, and what it takes to run.
- **Developers who are new to the project** and want the ideas before
  they read the code.

You do not need to know Rust, WebAssembly, or the project's history.
You should know roughly what a bus schedule and a GTFS feed are. The
[glossary](glossary.md) explains every other term.

## What each page answers

Read the pages in this order. Each one builds on the one before.

| Page | The question it answers |
|---|---|
| [What it does](what-it-does.md) | What does the software do, for whom, and how is it deployed? |
| [The apps](apps.md) | What does each app do, and does it need a server? |
| [How it works](how-it-works.md) | What happens, step by step, when a stop display shows its departures? |
| [Why it is built this way](why-it-is-built-this-way.md) | Why these design choices, and what do they cost? |
| [Glossary](glossary.md) | What does each term mean, and what might an agency call it? |

## Where to go next

- **To deploy the rider apps** with no server of your own, read
  [DEPLOYMENTS.md](../../DEPLOYMENTS.md). It is a step-by-step guide
  for a system administrator.
- **To build and run the project** on your own computer, read the
  [README](../../README.md).
- **For the full technical detail** of any part, read the
  [architecture reference](../architecture/README.md). It is the
  authority: where this guide and the reference disagree, the
  reference is right and this guide needs fixing.
- **For the work still to be done**, read
  [docs/units/](../units/README.md).

## How this guide is written

The guide follows the plain-language standard ISO 24495-1. In
practice, that means:

- each page starts with what the reader most needs;
- sentences are short and in the active voice;
- everyday words replace technical ones where they can, and every
  technical term that stays is defined in the glossary;
- the guide explains ideas and points to the reference for detail,
  so it does not repeat the reference.

If a page is hard to follow, that is a defect in the page. Please
report it the way you would report a bug.
