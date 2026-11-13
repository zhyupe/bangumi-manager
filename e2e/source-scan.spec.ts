import { expect, test } from "@playwright/test";
import { sourceFixture } from "./source-fixture";

test("source scan can cancel, retain results and scan again", async ({
  page,
}, testInfo) => {
  await sourceFixture(page);
  await page.evaluate(() => {
    const win = window as any;
    const original = win.__TAURI_INTERNALS__.invoke;
    let finish: (value: unknown) => void;
    let scans = 0;
    let cancellations = 0;
    win.__TAURI_INTERNALS__.invoke = async (command: string, args: any) => {
      if (command === "preview_naming") return {};
      if (command === "scan_files") {
        win.__sourceEmit("scan-active", true);
        win.__sourceEmit("scan-progress", 100);
        if (++scans === 1)
          return new Promise((resolve) => {
            finish = resolve;
          });
        win.__sourceEmit("scan-active", false);
        return { files: [], errors: [], cancelled: false };
      }
      if (command === "cancel_source_scan") {
        if (++cancellations === 1)
          throw new Error("Fictional cancellation failure");
        win.__finishScan = () => {
          win.__sourceEmit("scan-active", false);
          finish({ files: [], errors: [], cancelled: true });
        };
        return;
      }
      return original(command, args);
    };
  });
  await page.getByRole("button", { name: "路径", exact: true }).click();
  await page
    .getByRole("textbox", { name: "源目录路径" })
    .fill("D:\\FictionalInput");
  await page.getByRole("button", { name: "扫描路径", exact: true }).click();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "设置", exact: true }).click();
  const cancel = page.getByRole("button", { name: "取消扫描", exact: true });
  await expect(cancel).toBeInViewport();
  await page.setViewportSize({ width: 1050, height: 700 });
  await expect(cancel).toBeInViewport();
  await page.screenshot({ path: testInfo.outputPath("scan-cancel.png") });
  await cancel.focus();
  await page.keyboard.press("Enter");
  await expect(
    page
      .getByRole("alert")
      .filter({ hasText: "Fictional cancellation failure" }),
  ).toBeVisible();
  await expect(cancel).toBeEnabled();
  await cancel.click();
  await expect(
    page.getByRole("button", { name: "正在取消…", exact: true }),
  ).toBeDisabled();
  await page.evaluate(() => (window as any).__finishScan());
  await expect(
    page.getByText("扫描已取消，已发现的文件已保留", { exact: true }),
  ).toBeVisible();
  await expect(cancel).toHaveCount(0);
  await page.getByRole("button", { name: "整理", exact: true }).click();
  await page.getByRole("button", { name: "路径", exact: true }).click();
  await page
    .getByRole("textbox", { name: "源目录路径" })
    .fill("D:\\FictionalInput");
  await page.getByRole("button", { name: "扫描路径", exact: true }).click();
  await expect(
    page.locator('.floating-notice[data-tone="success"]'),
  ).toBeVisible();
});
