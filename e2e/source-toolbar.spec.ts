import { expect, test } from "@playwright/test";
import { sourceFixture } from "./source-fixture";

for (const language of ["zh-CN", "en"]) {
  test(`source toolbar keeps filters and actions aligned with readable folder menus in ${language}`, async ({
    page,
  }) => {
    await page.addInitScript((language) => {
      localStorage.setItem("bangumi.uiLanguage", language);
    }, language);
    const folder = [
      "D:",
      "Incoming",
      "Fictional Collection With A Long Directory Name",
      "Archive".repeat(12),
      "待整理资源",
    ].join("\\");
    await sourceFixture(
      page,
      ["Fictional.E01.mkv", "Fictional.E02.mkv", "Fictional.E03.chs.ass"],
      folder,
    );
    const toolbar = page.locator(".source-toolbar");
    const directory = toolbar.getByRole("combobox", { includeHidden: true });
    await expect(page.locator(".source-panel h2")).toHaveCount(0);
    for (const width of [1440, 1050]) {
      await page.setViewportSize({ width, height: 900 });
      await directory.click();
      const menu = page.getByRole("listbox");
      const option = menu.getByRole("option", { name: folder, exact: true });
      await expect(option).toBeVisible();
      await expect
        .poll(async () =>
          Math.abs(
            (await menu.boundingBox())!.width -
              (await directory.boundingBox())!.width,
          ),
        )
        .toBeLessThan(1);
      expect(
        await option.evaluate((element) => {
          const text = document.createRange();
          text.selectNodeContents(element.lastElementChild!);
          const bounds = text.getBoundingClientRect();
          const row = element.getBoundingClientRect();
          return (
            bounds.left >= row.left &&
            bounds.right <= row.right &&
            bounds.bottom <= row.bottom &&
            bounds.height > 24
          );
        }),
      ).toBe(true);
      await page.screenshot({
        path: `test-results/source-directory-${language}-${width}.png`,
      });
      await page.keyboard.press("End");
      await expect(option).toBeFocused();
      await page.keyboard.press("Enter");
      await expect(menu).toBeHidden();
      await expect(directory).toHaveAttribute("data-value", folder);
      await expect(directory).toHaveAttribute("title", folder);
      expect(
        await toolbar.evaluate((element) => {
          const controls = [
            ...element.querySelectorAll(
              ".filter-row > *, .button-row > button",
            ),
          ];
          const bounds = element.getBoundingClientRect();
          return controls.every((control) => {
            const rect = control.getBoundingClientRect();
            return (
              Math.abs(
                rect.top + rect.height / 2 - bounds.top - bounds.height / 2,
              ) < 1 &&
              rect.left >= bounds.left &&
              rect.right <= bounds.right
            );
          });
        }),
      ).toBe(true);
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
      await page.screenshot({
        path: `test-results/source-toolbar-${language}-${width}.png`,
      });
    }
    await toolbar.getByRole("textbox").fill("E0");
    await expect(page.locator(".file-table tbody tr")).toHaveCount(3);
    await toolbar.getByRole("checkbox").check();
    await expect(page.locator(".file-table tbody tr")).toHaveCount(2);
    await toolbar.getByRole("textbox").fill("E02");
    await expect(page.locator(".file-table tbody tr")).toHaveCount(1);
    await page.evaluate(() => {
      const win = window as any;
      const invoke = win.__TAURI_INTERNALS__.invoke;
      let cleared = false;
      win.__TAURI_INTERNALS__.invoke = async (
        command: string,
        args: unknown,
      ) => {
        if (command === "clear_sources") {
          cleared = true;
          return;
        }
        const result = await invoke(command, args);
        if (cleared && command === "snapshot") result.sources = [];
        return result;
      };
    });
    await toolbar
      .getByRole("button", {
        name: language === "en" ? "Clear list" : "清空列表",
        exact: true,
      })
      .click();
    await expect(page.locator(".drop-area")).toBeVisible();
    await expect(toolbar.getByRole("combobox")).toHaveCount(0);
    await expect(toolbar.locator(".button-row > button")).toHaveCount(4);
  });
}
