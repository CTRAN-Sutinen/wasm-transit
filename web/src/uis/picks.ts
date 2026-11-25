// What the planner's UIs read of the stops document for a pick. The
// server's GET /feed exports a pick the same way.

import { ask, available, produced, type Doc } from "../engine";
import type { Network } from "../types/Network";
import type { NetworkWarning } from "../types/NetworkWarning";
import type { PublishedPlans } from "../types/cap_plans/PublishedPlans";

export async function stopWarnings(network: Doc<Network>, published: PublishedPlans | null, stops: Doc | null): Promise<NetworkWarning[]> {
  if (!stops || !(await available("stops"))) return [];
  return (await ask("stops", "read_pick", { network_hash: network, stops_hash: stops, published: published ?? undefined })).warnings;
}

export async function withStopVersions(network: Doc<Network>, published: PublishedPlans | null, stops: Doc | null): Promise<Doc<Network>> {
  if (!stops || !(await available("stops"))) return network;
  const out = await ask("stops", "export_pick", { network_hash: network, stops_hash: stops, published: published ?? undefined });
  return out.network_hash === network.hash ? network : produced("network", out.network, "stops", out.network_hash);
}
