// The test's checks: each prints PASS or FAIL with what it saw; done()
// throws when any failed, so tools/ui-shot.mjs exits 1.

const results = [];

export function check(name, ok, saw = "") {
  results.push({ name, ok: !!ok });
  console.log(`  ${ok ? "PASS" : "FAIL"} ${name}${saw ? ` — ${saw}` : ""}`);
  return !!ok;
}

export function done(step) {
  const failed = results.filter((r) => !r.ok);
  console.log(`  ${step}: ${results.length - failed.length}/${results.length} passed`);
  if (failed.length) throw new Error(`${step}: failed ${failed.map((f) => f.name).join("; ")}`);
}

/// Poll `fn` until it is truthy or `ms` pass; false on timeout.
export async function until(page, fn, ms = 20000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (await fn().catch(() => false)) return true;
    await page.waitForTimeout(250);
  }
  return false;
}

export const clean = (s) => (s ?? "").replace(/\s+/g, " ").trim();
