import { expect, test, type Page } from "@playwright/test";
import { sourceFixture } from "./source-fixture";

async function fixture(page: Page, mode = "unused") {
  await page.addInitScript(() => {
    localStorage.setItem(
      "bangumi.mappings",
      JSON.stringify({
        "D:\\Incoming\\S01E01.mkv": {
          source: "D:\\Incoming\\S01E01.mkv",
          work_id: "tv",
          episode_ids: ["tv-1"],
          extra: false,
        },
        "D:\\Incoming\\Other.mkv": {
          source: "D:\\Incoming\\Other.mkv",
          work_id: "movie",
          episode_ids: ["movie-1"],
          extra: false,
        },
      }),
    );
  });
  await sourceFixture(page, ["S01E01.mkv", "Other.mkv"]);
  await page.evaluate((mode) => {
    const state = window as any;
    const original = state.__TAURI_INTERNALS__.invoke;
    state.__deleteCalls = [];
    state.__deleteError = "";
    state.__TAURI_INTERNALS__.invoke = async (
      command: string,
      args: any = {},
    ) => {
      if (command === "snapshot") {
        const snapshot = await original(command, args);
        if (mode === "offline")
          snapshot.index.resources = [
            { available: false, asset: { work_id: "tv" } },
          ];
        if (mode === "task")
          snapshot.plans = [
            {
              id: "pending",
              state: "draft",
              rows: [],
              works: [snapshot.works[0]],
              metadata_errors: [],
              created_at: "2026-10-07",
            },
          ];
        // Keep returning the old snapshot to exercise late reply protection.
        return snapshot;
      }
      if (command === "refresh_library") return {};
      if (command === "delete_work") {
        state.__deleteCalls.push(args);
        if (state.__deleteError) throw new Error(state.__deleteError);
        return new Promise((resolve) => {
          state.__finishDeleteWork = resolve;
        });
      }
      return original(command, args);
    };
  }, mode);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: "扫描", exact: true }).click();
  await page.getByRole("button", { name: /虚构作品-tv/ }).click();
}

test("delete unused titles with episode metadata, retry errors and clear only their matches", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "删除作品", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("虚构作品-tv");
  await expect(dialog).toContainText("源文件、媒体、字幕和备份均保留");
  await expect(dialog).toContainText("清除待匹配记录");
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  expect(await page.evaluate(() => (window as any).__deleteCalls.length)).toBe(
    0,
  );
  await page.getByRole("button", { name: "删除作品", exact: true }).click();
  await page.evaluate(() => {
    (window as any).__deleteError =
      "此作品已关联资料库目录，不能删除，请先核对目录关联";
  });
  await dialog.getByRole("button", { name: "确认删除", exact: true }).click();
  await expect(dialog.getByRole("alert")).toContainText("请先核对目录关联");
  await expect(
    dialog.getByRole("button", { name: "确认删除", exact: true }),
  ).toBeEnabled();
  await page.screenshot({
    path: "test-results/delete-work-error.png",
    fullPage: true,
    animations: "disabled",
  });
  await page.evaluate(() => {
    (window as any).__deleteError = "";
  });
  await dialog.getByRole("button", { name: "确认删除", exact: true }).click();
  await expect(
    dialog.getByRole("button", { name: "正在删除…", exact: true }),
  ).toBeDisabled();
  await expect(
    dialog.getByRole("button", { name: "取消", exact: true }),
  ).toBeDisabled();
  await page.keyboard.press("Escape");
  await page.mouse.click(8, 8);
  await expect(dialog).toBeVisible();
  expect(await page.evaluate(() => (window as any).__deleteCalls.length)).toBe(
    2,
  );
  await page.evaluate(() => (window as any).__finishDeleteWork());
  await expect(dialog).toHaveCount(0);
  await expect(page.locator(".work-detail-layout")).toHaveCount(0);
  await expect(
    page.getByText("作品已从本地目录删除，文件已保留。", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "扫描", exact: true }).click();
  await expect(page.getByRole("button", { name: /虚构作品-tv/ })).toHaveCount(
    0,
  );
  const mappings = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("bangumi.mappings")!),
  );
  expect(Object.values(mappings).map((m: any) => m.work_id)).toEqual(["movie"]);
  await page.getByRole("button", { name: "整理", exact: true }).click();
  await expect(
    page.getByText("S01E01.mkv", { exact: true }).first(),
  ).toBeVisible();
  await expect(
    page.getByText("Other.mkv", { exact: true }).first(),
  ).toBeVisible();
  const workSelect = page.getByRole("combobox", {
    name: "整理到本地条目",
    exact: true,
  });
  await workSelect.click();
  await expect(
    page.locator('[role="option"][data-item-value="tv"]'),
  ).toHaveCount(0);
  await expect(
    page.locator('[role="option"][data-item-value="movie"]'),
  ).toBeVisible();
});

for (const [mode, reason] of [
  ["offline", "已有关联资源，不能删除"],
  ["task", "请先取消或完成此作品的任务"],
]) {
  test(`deletion stays unavailable for ${mode} associations`, async ({
    page,
  }) => {
    await fixture(page, mode);
    await expect(
      page.getByRole("button", { name: "删除作品", exact: true }),
    ).toBeDisabled();
    await expect(page.getByText(reason, { exact: true })).toBeVisible();
    expect(
      await page.evaluate(() => (window as any).__deleteCalls.length),
    ).toBe(0);
  });
}

test("deletion confirmation works in English and remains readable in short windows", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.getByRole("combobox", { name: "界面语言", exact: true }).click();
  await page.getByRole("option", { name: "English", exact: true }).click();
  await page.getByRole("button", { name: "Library", exact: true }).click();
  await page.getByRole("button", { name: /虚构作品-tv/ }).click();
  await page.getByRole("button", { name: "Delete title", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(
    dialog.getByRole("heading", { name: "Delete this title?" }),
  ).toBeVisible();
  await page.setViewportSize({ width: 1050, height: 600 });
  await expect(
    dialog.getByRole("button", { name: "Confirm deletion", exact: true }),
  ).toBeInViewport();
  expect(await dialog.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(
    true,
  );
  await page.screenshot({
    path: "test-results/delete-work-english.png",
    fullPage: true,
    animations: "disabled",
  });
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__deleteCalls.length)).toBe(
    0,
  );
});
