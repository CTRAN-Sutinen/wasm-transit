/// The capability-bus demo's host side: load the manifest, assemble
/// the composition, and run it — the feed bytes into gtfs (identity),
/// the SAME bytes into the network capability (the document), the
/// SAME document into board (the day view), hashes asserted equal at
/// the host at every hop, then a plan through router over the feed.
/// Partial assemblies degrade per capability: with router unavailable
/// the feed/document/board half still runs, and the statuses say so.
/// CapabilityDemo.svelte only renders `demo` and the rows below.
import {
  caps,
  ensure,
  loadManifest,
  gtfsCap,
  routerCap,
  boardCap,
  boardModule,
  ensureBoardBoarding,
  ensureRouterBoarding,
  forgetCapability,
  restoreDocuments,
  putDocument,
  currentDocument,
  documentHistory,
  rollbackDocument,
  migrateWatchlistForward,
  type GtfsLoadFeed,
} from "./capabilities.svelte";
import type { Watchlist } from "./types/cap_board/Watchlist";

type Step = { label: string; detail: string; ok: boolean };

export const demo = $state({
  running: false,
  result: "",
  steps: [] as Step[],
  degraded: false,
  docSteps: [] as Step[],
  board: [] as { id: string; label: string; count: number }[],
  docMessage: "",
});

/// The manifest's capabilities with the resolver's status for each —
/// null until the manifest loads.
export function capabilityRows(): { id: string; hash: string; status: string; kib: string }[] | null {
  if (!caps.manifest) return null;
  return caps.manifest.capabilities.map((cap) => ({
    id: cap.id,
    hash: cap.hash,
    status: caps.statuses[cap.id],
    kib: (Number(cap.bytes) / 1024).toFixed(0),
  }));
}

/// The watchlist's current version and its history chain.
export function watchlistState(): { version: number; hash: string; history: string } | null {
  const current = currentDocument("watchlist");
  if (!current) return null;
  return {
    version: current.version,
    hash: current.hash,
    history: documentHistory("watchlist").map((r) => `v${r.version}`).join(" ← "),
  };
}

const DATE = "2026-10-23"; // the fixture's loaded Friday
const DEPART = 7 * 3600;

// The versioned-document demo: seed a v1 watchlist, query it
// (labels are v1 notes), migrate forward to v2 (colors dropped and
// REPORTED), roll back — the old version still queries, nothing
// was destroyed.
const SEED_V1 = JSON.stringify({
  version: "v1",
  stops: [
    { id: "A", note: "home", color: "blue" },
    { id: "B", note: "work" },
  ],
});

/// Assemble every capability the manifest lists and restore the
/// document store. The panel DECLARES its needs; the resolver
/// assembles demand-driven. Statuses render from resolver state alone.
export function assembleDemo(): void {
  // (ensure is memoized: a manifest another need already loaded —
  // the bootstrap reads the edit document through cap-divert — still
  // assembles everything here.)
  (caps.manifest ? Promise.resolve(caps.manifest) : loadManifest())
    .then((manifest) => ensure(manifest.capabilities.map((c) => c.id)))
    .catch((error: unknown) => {
      demo.steps.push({
        label: "manifest",
        detail: error instanceof Error ? error.message : String(error),
        ok: false,
      });
    });
  restoreDocuments();
}

function fmtTime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

/// The degraded-state simulation: point the router capability at a
/// hash nobody serves. The resolver marks it unavailable; the
/// composition runs its gtfs half only — exactly what a user whose
/// bundle stopped at "know when the bus comes" would ship.
export async function simulateDegraded(): Promise<void> {
  demo.degraded = true;
  if (caps.manifest) {
    const router = caps.manifest.capabilities.find((c) => c.id === "router");
    if (router) {
      router.url = "caps/router-0000000000000000000000000000000000000000000000000000000000000000.wasm";
      forgetCapability("router");
    }
  }
  await runDemo();
}

export async function seedV1(): Promise<void> {
  demo.docSteps = [];
  demo.board = [];
  demo.docMessage = "";
  const record = await putDocument("watchlist", SEED_V1, 1);
  demo.docSteps.push({
    label: "seed v1",
    detail: `${record.hash.slice(0, 19)}… (2 stops, one with a color)`,
    ok: true,
  });
  await queryBoard();
}

async function queryBoard(): Promise<void> {
  try {
    const current = currentDocument("watchlist");
    if (!current) {
      demo.docMessage = "seed the watchlist first";
      return;
    }
    const bytes = new Uint8Array(await (await fetch("caps/sample-gtfs.zip")).arrayBuffer());
    const boarding = await ensureBoardBoarding(bytes);
    if (!boarding) {
      demo.docMessage = "board capability unavailable";
      return;
    }
    const module = await boardModule();
    if (!module) {
      demo.docMessage = "board capability unavailable";
      return;
    }
    const document = JSON.parse(current.bytes) as Watchlist;
    const cap = boardCap(module);
    const { watchlistHash } = cap.loadWatchlist(document);
    const stops = cap.watchlistBoard(watchlistHash, boarding.networkHash, null, DATE, DEPART, 3);
    demo.board = stops.map((s) => ({ id: s.id, label: s.label, count: s.board.rows.length }));
    demo.docSteps.push({
      label: `board @ v${current.version}`,
      detail: demo.board.map((s) => `${s.id} "${s.label}" (${s.count})`).join(", "),
      ok: true,
    });
  } catch (error) {
    demo.docMessage = error instanceof Error ? error.message : String(error);
  }
}

export async function migrateForward(): Promise<void> {
  demo.docSteps = [];
  const outcome = await migrateWatchlistForward("board");
  if ("error" in outcome) {
    demo.docMessage = outcome.error;
    return;
  }
  demo.docSteps.push({
    label: `migrate → v${outcome.record.version}`,
    detail: `${outcome.record.hash.slice(0, 19)}… (from ${outcome.record.produced_by?.from.slice(0, 19)}…)`,
    ok: true,
  });
  for (const note of outcome.notes) {
    demo.docSteps.push({
      label: "migration note",
      detail: `${note.kind}: ${note.field} ×${note.count} — no silent loss`,
      ok: false,
    });
  }
  await queryBoard();
}

export async function rollback(): Promise<void> {
  demo.docSteps = [];
  const record = await rollbackDocument("watchlist");
  if (!record) {
    demo.docMessage = "nothing to roll back to";
    return;
  }
  demo.docSteps.push({
    label: `rollback → v${record.version}`,
    detail: `${record.hash.slice(0, 19)}… still addressable, still queries`,
    ok: true,
  });
  await queryBoard();
}

export async function runDemo(): Promise<void> {
  demo.running = true;
  demo.steps = [];
  demo.result = "";
  try {
    const bytes = new Uint8Array(await (await fetch("caps/sample-gtfs.zip")).arrayBuffer());

    const modules = await ensure(["gtfs", "router", "network", "board"]);
    const gtfs = modules.gtfs ? gtfsCap(modules.gtfs) : null;
    const router = modules.router ? routerCap(modules.router) : null;
    const boardC = modules.board ? boardCap(modules.board) : null;

    if (!gtfs) {
      demo.steps.push({ label: "gtfs capability", detail: "unavailable", ok: false });
      return;
    }
    const loadedGtfs: GtfsLoadFeed = gtfs.loadFeed(bytes);
    demo.steps.push({
      label: "gtfs.load_feed",
      detail: `${loadedGtfs.feedHash.slice(0, 19)}… (${loadedGtfs.summary.trips} trips)`,
      ok: true,
    });

    // The document chain: the same bytes into the network
    // capability, the derived document into the board — identity
    // asserted at every hop, never assumed.
    const boarding = await ensureBoardBoarding(bytes);
    if (!boardC || !boarding) {
      demo.steps.push({ label: "board capability", detail: "unavailable", ok: false });
      return;
    }
    demo.steps.push({
      label: "hash handoff",
      detail: `${boarding.networkHash.slice(0, 19)}… network document == import's == board's`,
      ok: true,
    });

    const departures = boardC.stopBoard(boarding.networkHash, null, "A", DATE, DEPART, 5, null).rows;
    demo.steps.push({
      label: "board.stop_board",
      detail: `${departures.length} departures at A from ${fmtTime(DEPART)}`,
      ok: true,
    });

    // The router plans over the SAME network document (derived by
    // the import, loaded into its registry, identity asserted —
    // never the feed bytes: the router left the feed in U5).
    const routerBoarding = await ensureRouterBoarding(bytes);
    if (!router || !routerBoarding) {
      demo.steps.push({
        label: "router capability",
        detail: "unavailable — the assembly degraded to its day half",
        ok: false,
      });
      demo.result = `${departures.length} departures served; planning skipped (capability unavailable).`;
      return;
    }
    demo.steps.push({
      label: "hash handoff",
      detail: `${routerBoarding.networkHash.slice(0, 19)}… network document == import's == router's`,
      ok: true,
    });

    const plan = router.plan(routerBoarding.networkHash, null, "A", "B", DATE, DEPART, 4);
    demo.steps.push({
      label: "router.plan",
      detail: plan
        ? `A → B: depart ${fmtTime(plan.depart_secs)}, arrive ${plan.arrival_hhmmss} (${plan.legs.length} legs)`
        : "unreachable",
      ok: true,
    });
    demo.result = plan
      ? `Composition ok: ${departures.length} departures via board, arrival ${plan.arrival_hhmmss} via router — one feed, one network document, four capabilities.`
      : "Composition ok; the pair is unreachable on this date.";
  } catch (error) {
    demo.steps.push({
      label: "failed",
      detail: error instanceof Error ? error.message : String(error),
      ok: false,
    });
  } finally {
    demo.running = false;
  }
}
