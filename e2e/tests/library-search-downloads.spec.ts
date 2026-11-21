import { expect, Page, test } from "@playwright/test";
import { attachPageErrorGuard, startTestServer } from "../helpers/fmd-server";

async function completeSetup(page: Page, baseUrl: string) {
  await page.goto(baseUrl);
  await page.fill("#setup-username", "admin");
  await page.fill("#setup-password", "AdminPassword123!");
  await page.fill("#setup-confirm-password", "AdminPassword123!");
  await page.click('#setup-wizard-form button[type="submit"]');
  await expect(page.locator("#main-app-container")).toBeVisible();
}

test("search/fetch manga, add/remove favorites, live WebSocket downloads, catalog sync & merger", async ({
  page,
}) => {
  const srv = await startTestServer();
  const assertNoPageErrors = attachPageErrorGuard(page);

  try {
    await completeSetup(page, srv.baseUrl);

    // 1. Search & Fetch manga from FixtureManga module
    await page.click("#tab-btn-search");
    await expect(page.locator("#tab-search")).toBeVisible();
    await expect(
      page.locator('#fetch-module-select option[value="fixture-mod"]'),
    ).toBeAttached();
    await page.selectOption("#fetch-module-select", "fixture-mod");
    await page.fill("#fetch-manga-url", `${srv.originUrl}/manga/one`);
    await page.click("#btn-inspect-manga");

    await expect(page.locator("#manga-preview")).toBeVisible();
    await expect(page.locator("#prev-title")).toHaveText("One Manga");
    await expect(page.locator("#prev-chapters-count")).toHaveText("2");

    // 2. Add to favorites & verify in Library tab
    await page.click("#btn-add-favorite");
    await page.click("#tab-btn-favorites");
    await expect(page.locator("#favorites-list")).toContainText("One Manga");

    // 3. Queue chapter download & verify live WebSocket update in Downloads tab
    await page.click("#tab-btn-search");
    // Chapters are checked by default; uncheck the second so exactly 1 download is queued
    await page
      .locator('#chapters-list-container input[type="checkbox"]')
      .nth(1)
      .uncheck();
    await page.click("#btn-dl-queue");

    await page.click("#tab-btn-downloads");
    await expect(page.locator("#downloads-table-body")).toContainText(
      "One Manga",
    );

    // Fetch download ID via API and simulate worker completion callback -> WebSocket updates DOM live
    const dls: Array<{ id: string }> = await page.evaluate(async () => {
      const res = await fetch("/api/downloads");
      return res.json();
    });
    expect(dls.length).toBe(1);

    const completeRes = await page.request.post(
      `${srv.baseUrl}/api/internal/tasks/complete`,
      {
        headers: { "X-FMD-Internal-Token": srv.internalToken },
        data: {
          task_id: dls[0].id,
          success: true,
          error_message: null,
          saved_path: "/tmp/one_ch1.cbz",
        },
      },
    );
    expect(completeRes.ok()).toBeTruthy();

    await expect(page.locator("#downloads-table-body")).toContainText(
      /Completed|Terminé/i,
    );

    // 4. Catalog sync & search
    await page.click("#tab-btn-catalog");
    await expect(
      page.locator('#catalog-module-select option[value="fixture-mod"]'),
    ).toBeAttached();
    await page.selectOption("#catalog-module-select", "fixture-mod");
    await page.click("#btn-sync-catalog");

    await expect(page.locator("#catalog-table-body")).toContainText(
      "One Manga",
      {
        timeout: 10_000,
      },
    );
    await page.fill("#catalog-search-input", "Two");
    await expect(page.locator("#catalog-table-body")).toContainText(
      "Two Manga",
    );

    // 5. Merger tab search
    await page.click("#tab-btn-merger");
    await page.fill("#merger-title-input", "One");
    await page.click("#btn-merger-search");
    await expect(page.locator("#merger-sources-box")).toBeVisible();

    assertNoPageErrors();
  } finally {
    await srv.stop();
  }
});
