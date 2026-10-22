import { expect, test, type Page } from "@playwright/test";
import { sourceFixture } from "./source-fixture";

async function settingsFixture(page: Page) {
  await sourceFixture(page, ["虚构远航 S02E01.mkv"]);
  await page.evaluate(() => {
    const runtime = (window as any).__TAURI_INTERNALS__;
    const original = runtime.invoke;
    runtime.invoke = async (command: string, args: any) => {
      if (command === "preview_naming")
        return { tv: "TV", movie: "Movies", music: "Music" };
      if (command === "diagnostics_report")
        return {
          statistics: [],
          recent: [],
          active: [],
          directory: "D:\\Data\\logs",
        };
      return original(command, args);
    };
  });
}

test("interface language switches immediately, preserves data and persists independently of TMDB", async ({
  page,
}) => {
  await settingsFixture(page);
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await page.getByRole("button", { name: "设置", exact: true }).click();
  const metadataLanguage = await page
    .getByRole("combobox", { name: "TMDB 查询语言", exact: true })
    .inputValue();
  await page.getByRole("combobox", { name: "界面语言", exact: true }).click();
  await page.getByRole("option", { name: "English", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect(
    page.getByRole("heading", { name: "Settings", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("combobox", { name: "TMDB metadata language", exact: true }),
  ).toHaveValue(metadataLanguage);
  await page.getByRole("button", { name: "Organize", exact: true }).click();
  await expect(
    page.getByText("虚构远航 S02E01.mkv", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Add title", exact: true }).click();
  await expect(
    page.getByRole("dialog").getByLabel("Remote title ID", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Close import dialog", exact: true })
    .click();
  await page.getByRole("button", { name: "Resources", exact: true }).click();
  for (const width of [1440, 1050]) {
    await page.setViewportSize({ width, height: 850 });
    await expect(
      page.getByRole("tab", { name: "Media files", exact: true }),
    ).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth + 1,
      ),
    ).toBe(true);
    const sidebarBounds = await page.locator(".sidebar").boundingBox();
    const activeBounds = await page
      .locator(".sidebar nav button.active")
      .boundingBox();
    expect(activeBounds!.x + activeBounds!.width).toBeLessThanOrEqual(
      sidebarBounds!.x + sidebarBounds!.width,
    );
    await page.screenshot({
      path: `test-results/i18n-resources-${width}.png`,
      fullPage: true,
    });
  }
  expect(
    await page.evaluate(() =>
      (window as any).__sourceCalls.some(
        (c: any) => c.command === "save_settings",
      ),
    ),
  ).toBe(false);
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Organize", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Interface language", exact: true })
    .click();
  await page.getByRole("option", { name: "简体中文", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
  await expect(
    page.getByRole("button", { name: "整理", exact: true }),
  ).toBeVisible();
});

test.describe("system language", () => {
  test.use({ locale: "en-US" });
  test("starts in English and displays translated backend failures without changing the error path", async ({
    page,
  }) => {
    await sourceFixture(page);
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    await page.evaluate(() => {
      const runtime = (window as any).__TAURI_INTERNALS__;
      const original = runtime.invoke;
      runtime.invoke = async (command: string, args: any) => {
        if (command === "list_resources")
          throw "文件在预览后发生变化：D:\\作品\\虚构文件.ts";
        return original(command, args);
      };
    });
    await page.getByRole("button", { name: "Resources", exact: true }).click();
    await expect(page.getByRole("alert").first()).toContainText(
      "File changed after preview: D:\\作品\\虚构文件.ts",
    );
    await page.getByRole("button", { name: "Tasks", exact: true }).click();
    await expect(
      page.getByText("No organization plans", { exact: true }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Library", exact: true }).click();
    await expect(
      page.getByText("虚构作品-tv", { exact: true }).first(),
    ).toBeVisible();
  });
});
