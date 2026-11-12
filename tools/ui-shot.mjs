#!/usr/bin/env node
/// The UI QA driver: open one generated UI's page in a real browser,
/// optionally run steps on it, and save a screenshot — printing what a
/// reviewer needs beside the picture: the page's status line, its URL
/// hash (the selection), and every page error and console error.
///
/// It runs against `just qa` (vite on :5173 over a sync server on the
/// QA feed) and keeps ONE browser profile in .qa/profile, so the plans
/// a walk forked stay in IndexedDB between runs — a walk forks once.
///
///   node tools/ui-shot.mjs <ui> [options]
///
///   --hash '#stage=route&route=025'   the selection to open (the page
///                                     follows its hash)
///   --steps walk.mjs                  a module whose default export is
///                                     async (page, { shot, status }) =>
///                                     …; shot(label) saves a picture
///   --out file.png                    the final screenshot (default
///                                     .qa/shots/<ui>-<time>.png)
///   --full                            the whole page, not the viewport
///   --wait MS                         settle time after load and after
///                                     the steps (default 4000)
///   --size 2560x1300                  the viewport (default 1600x1000)
///   --light                           light scheme (default dark)
///   --base URL                        default http://127.0.0.1:5173
///   --fresh                           start from an empty profile
///
/// Service workers are blocked (a worker would serve the last build's
/// files, not the dev server's).

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const { chromium } = await import(path.join(root, "web/node_modules/playwright/index.mjs"));

const args = process.argv.slice(2);
const ui = args.find((a, i) => !a.startsWith("--") && !args[i - 1]?.match(/^--(hash|steps|out|wait|size|base)$/));
if (!ui) {
  console.error("usage: node tools/ui-shot.mjs <ui> [--hash H] [--steps FILE] [--out FILE] [--full] [--wait MS] [--size WxH] [--light] [--base URL] [--fresh]");
  process.exit(2);
}
const opt = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
const flag = (name) => args.includes(`--${name}`);

const qa = path.join(root, ".qa");
const profile = path.join(qa, "profile");
const shots = path.join(qa, "shots");
if (flag("fresh")) fs.rmSync(profile, { recursive: true, force: true });
fs.mkdirSync(shots, { recursive: true });

const [width, height] = opt("size", "1600x1000").split("x").map(Number);
const wait = Number(opt("wait", "4000"));
const base = opt("base", "http://127.0.0.1:5173");
const hash = opt("hash", "");
const stamp = new Date().toISOString().replace(/[:.]/g, "-");
const out = path.resolve(opt("out", path.join(shots, `${ui}-${stamp}.png`)));

const ctx = await chromium.launchPersistentContext(profile, {
  viewport: { width, height },
  colorScheme: flag("light") ? "light" : "dark",
  serviceWorkers: "block",
  acceptDownloads: true,
});
const page = ctx.pages()[0] ?? (await ctx.newPage());
const problems = [];
page.on("pageerror", (e) => problems.push(`page error: ${e.message}`));
page.on("console", (m) => {
  if (m.type() === "error") problems.push(`console: ${m.text().slice(0, 400)}`);
});
page.on("dialog", (d) => d.accept());

const status = async () => (await page.getByTestId("status").first().textContent({ timeout: 1000 }).catch(() => ""))?.trim() ?? "";
let n = 0;
async function shot(label, { full = false } = {}) {
  await page.waitForTimeout(500);
  const file = out.replace(/\.png$/, `-${String(++n).padStart(2, "0")}-${label.replace(/[^\w-]+/g, "_")}.png`);
  await page.screenshot({ path: file, fullPage: full });
  console.log(`shot ${file}\n  status: ${await status()}\n  hash: ${decodeURIComponent(new URL(page.url()).hash)}`);
  return file;
}

try {
  await page.goto(`${base}/${ui}.html${hash}`);
  await page.waitForTimeout(wait);
  const steps = opt("steps");
  if (steps) {
    const mod = await import(path.resolve(steps));
    await mod.default(page, { shot, status });
    await page.waitForTimeout(wait);
  }
  await page.screenshot({ path: out, fullPage: flag("full") });
  console.log(`shot ${out}\n  status: ${await status()}\n  hash: ${decodeURIComponent(new URL(page.url()).hash)}`);
} catch (e) {
  problems.push(`driver: ${e.message.split("\n")[0]}`);
  await page.screenshot({ path: out }).catch(() => {});
  console.log(`shot ${out} (at the failure)`);
} finally {
  for (const p of problems) console.log(p);
  if (!problems.length) console.log("no page or console errors");
  await ctx.close();
}
process.exit(problems.some((p) => p.startsWith("driver:")) ? 1 : 0);
