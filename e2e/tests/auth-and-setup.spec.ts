import { expect, test } from "@playwright/test";
import { attachPageErrorGuard, startTestServer } from "../helpers/fmd-server";

test("setup wizard creates admin, persists cookie across reload, and enforces login & rate limit", async ({
  page,
}) => {
  const srv = await startTestServer();
  const assertNoPageErrors = attachPageErrorGuard(page);

  try {
    await page.goto(srv.baseUrl);
    await expect(page.locator("#screen-setup-wizard")).toBeVisible();

    // Create admin via setup wizard
    await page.fill("#setup-username", "e2e_admin");
    await page.fill("#setup-password", "E2eStrongPassword123!");
    await page.fill("#setup-confirm-password", "E2eStrongPassword123!");
    await page.click('#setup-wizard-form button[type="submit"]');

    await expect(page.locator("#main-app-container")).toBeVisible();
    await expect(page.locator("#ws-badge")).toContainText(
      /Connecté|Connected|接続済み/i,
    );

    // Reload keeps authenticated session via HttpOnly cookie
    await page.reload();
    await expect(page.locator("#main-app-container")).toBeVisible();
    await expect(page.locator("#screen-login")).toBeHidden();

    // Logout shows login screen
    await page.click('button[onclick="logout()"]');
    await expect(page.locator("#screen-login")).toBeVisible();

    // Wrong password displays error, and 5 failures trigger rate-limit lockout
    for (let i = 0; i < 5; i++) {
      await page.fill("#login-username", "e2e_admin");
      await page.fill("#login-password", "WrongPass!");
      await page.click('#login-form button[type="submit"]');
      await expect(page.locator("#login-error")).toBeVisible();
    }

    // 6th attempt shows 429 rate-limit message
    await page.fill("#login-username", "e2e_admin");
    await page.fill("#login-password", "E2eStrongPassword123!");
    await page.click('#login-form button[type="submit"]');
    await expect(page.locator("#login-error")).toContainText(
      /429|patienter|wait|tentatives|Trop d/i,
    );

    assertNoPageErrors();
  } finally {
    await srv.stop();
  }
});

test("silent admin bootstrap via env bypasses setup wizard and allows login", async ({
  page,
}) => {
  const srv = await startTestServer({
    env: {
      FMD_ADMIN_USER: "boot_admin",
      FMD_ADMIN_PASSWORD: "BootPassword123!",
    },
  });
  const assertNoPageErrors = attachPageErrorGuard(page);

  try {
    await page.goto(srv.baseUrl);
    await expect(page.locator("#screen-setup-wizard")).toBeHidden();
    await expect(page.locator("#screen-login")).toBeVisible();

    await page.fill("#login-username", "boot_admin");
    await page.fill("#login-password", "BootPassword123!");
    await page.click('#login-form button[type="submit"]');

    await expect(page.locator("#main-app-container")).toBeVisible();
    assertNoPageErrors();
  } finally {
    await srv.stop();
  }
});
