import { expect, test, type Page } from "@playwright/test";
import { sourceFixture } from "./source-fixture";

async function fixture(page: Page) {
  await sourceFixture(page);
  await page.evaluate(() => {
    const runtime = (window as any).__TAURI_INTERNALS__;
    const original = runtime.invoke;
    runtime.invoke = async (command: string, args: any = {}) => {
      if (command === "search_media") {
        return new Promise((resolve, reject) => {
          (window as any).__finishSearch = (fail = false) =>
            fail ? reject(new Error("模拟搜索失败")) : resolve([]);
        });
      }
      return original(command, args);
    };
  });
  await page.getByRole("button", { name: "添加作品", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("搜索词", { exact: true }).fill("虚构搜索词");
  return dialog;
}

test("loading stays above the modal and retains busy dismissal guards", async ({
  page,
}) => {
  const dialog = await fixture(page);
  await dialog.getByRole("button", { name: "搜索", exact: true }).click();
  const activity = page.getByRole("status").filter({ hasText: "搜索远程条目" });
  await expect(activity).toBeVisible();
  for (const viewport of [
    { width: 1440, height: 1000 },
    { width: 900, height: 560 },
  ]) {
    await page.setViewportSize(viewport);
    await expect(activity).toBeInViewport();
    expect(
      await activity.evaluate((element) => {
        const box = element.getBoundingClientRect();
        return element.contains(
          document.elementFromPoint(
            box.x + box.width / 2,
            box.y + box.height / 2,
          ),
        );
      }),
    ).toBe(true);
    await page.screenshot({
      path: `test-results/modal-loading-${viewport.width}.png`,
      fullPage: true,
    });
  }
  await expect(
    dialog.getByRole("button", { name: "关闭添加窗口" }),
  ).toBeDisabled();
  await page.keyboard.press("Escape");
  await page.mouse.click(8, 8);
  await expect(dialog).toBeVisible();
  await page.evaluate(() => (window as any).__finishSearch());
  await expect(activity).toHaveCount(0);
  await expect(
    dialog.getByRole("button", { name: "关闭添加窗口" }),
  ).toBeEnabled();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});

test("dismissing a notification preserves the modal and form state", async ({
  page,
}) => {
  const dialog = await fixture(page);
  await dialog.getByRole("button", { name: "搜索", exact: true }).click();
  await page.evaluate(() => (window as any).__finishSearch(true));
  const error = page.locator('.floating-notice[data-tone="error"]');
  await expect(error).toContainText("模拟搜索失败");
  await dialog.getByLabel("搜索词", { exact: true }).focus();
  await page.screenshot({
    path: "test-results/modal-notification.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "关闭错误消息" }).click();
  await expect(error).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await expect(dialog.getByLabel("搜索词", { exact: true })).toHaveValue(
    "虚构搜索词",
  );
  await expect(dialog.getByLabel("搜索词", { exact: true })).toBeFocused();

  // A notice that predates the modal must also remain accessible and dismissible.
  await dialog.getByRole("button", { name: "搜索", exact: true }).click();
  await page.evaluate(() => (window as any).__finishSearch(true));
  await expect(error).toBeVisible();
  await dialog.getByRole("button", { name: "关闭添加窗口" }).click();
  await page.getByRole("button", { name: "添加作品", exact: true }).click();
  await page.getByRole("button", { name: "关闭错误消息" }).click();
  await expect(error).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await page.mouse.click(8, 8);
  await expect(dialog).toBeHidden();
});
