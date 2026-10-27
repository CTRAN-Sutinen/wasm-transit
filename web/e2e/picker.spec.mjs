// The front door: / lists every generated UI (one card per
// src/uis/<id>/ui.json, discovered at build time) and demotes the
// original app to a link. The expected set is read from the source
// tree, so a new UI is covered without editing this spec.

import { test, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import path from "node:path";

const uisDir = path.join(path.dirname(fileURLToPath(import.meta.url)), "../src/uis");
const uis = fs
  .readdirSync(uisDir, { withFileTypes: true })
  .filter((d) => d.isDirectory())
  .map((d) => JSON.parse(fs.readFileSync(path.join(uisDir, d.name, "ui.json"), "utf8")));

test("the picker lists every generated UI and links the original app", async ({ page }) => {
  await page.goto("/");
  const list = page.getByTestId("ui-list");
  for (const ui of uis) {
    const card = list.getByTestId(`ui-${ui.id}`);
    await expect(card).toContainText(ui.title);
    await expect(card).toHaveAttribute("href", `${ui.id}.html`);
  }
  await expect(list.locator("a.card")).toHaveCount(uis.length);

  await page.getByTestId("original-app").click();
  await expect(page).toHaveURL(/\/app\.html$/);
  await page.waitForFunction(() => typeof window.__setNow === "function");
});

for (const ui of uis) {
  test(`the ${ui.id} card opens its UI`, async ({ page }) => {
    await page.goto("/");
    await page.getByTestId(`ui-${ui.id}`).click();
    await expect(page).toHaveURL(new RegExp(`/${ui.id}\\.html$`));
    await expect(page.locator("#app")).not.toBeEmpty();
  });
}
