import { expect, test } from "@playwright/test";
import { sourceFixture } from "./source-fixture";

test("settings draft survives navigation, refresh and failed save", async ({
  page,
}) => {
  await sourceFixture(page);
  await page.evaluate(() => {
    const win = window as any;
    const original = win.__TAURI_INTERNALS__.invoke;
    let saved: any;
    win.__failSave = true;
    win.__snapshots = 0;
    win.__TAURI_INTERNALS__.invoke = async (command: string, args: any) => {
      if (command === "preview_naming") return {};
      if (command === "save_settings") {
        if (win.__failSave) throw new Error("Fictional save failure");
        saved = args.settings;
        return;
      }
      const result = await original(command, args);
      if (command === "snapshot") {
        win.__snapshots++;
        if (saved) result.settings = saved;
      }
      return result;
    };
  });
  await page.getByRole("button", { name: "设置", exact: true }).click();
  const output = page.getByRole("textbox", {
    name: "整体输出目录",
    exact: true,
  });
  await output.fill("D:\\FictionalLibrary");
  await page.getByRole("button", { name: "整理", exact: true }).click();
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(output).toHaveValue("D:\\FictionalLibrary");
  await page.evaluate(() =>
    (window as any).__sourceEmit("job-error", "Fictional background event"),
  );
  await expect(
    page.getByText("Fictional background event", { exact: true }),
  ).toBeVisible();
  await expect
    .poll(() => page.evaluate(() => (window as any).__snapshots))
    .toBe(1);
  await expect(output).toHaveValue("D:\\FictionalLibrary");
  await page.getByRole("button", { name: "保存全部设置" }).click();
  await expect(
    page.getByText("Fictional save failure", { exact: true }),
  ).toBeVisible();
  await expect(output).toHaveValue("D:\\FictionalLibrary");
  await page.evaluate(() => {
    (window as any).__failSave = false;
  });
  await page.getByRole("button", { name: "保存全部设置" }).click();
  await expect(page.getByText("所有设置已保存", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "整理", exact: true }).click();
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(output).toHaveValue("D:\\FictionalLibrary");
});
