import { expect, test } from "@playwright/test";
import { chooseWork, sourceFixture } from "./source-fixture";

test("movie mappings show the feature instead of season and episode numbering", async ({
  page,
}) => {
  await sourceFixture(page, ["虚构影片.mkv"]);
  await chooseWork(page, "movie");
  await expect(
    page.getByRole("columnheader", { name: "对应内容", exact: true }),
  ).toBeVisible();
  const mapping = page.getByRole("combobox", {
    name: "虚构影片.mkv 对应内容",
    exact: true,
  });
  await mapping.click();
  const option = page.locator('[role="option"][data-item-value="movie-1"]');
  await expect(option).toContainText("正片 · 虚构章节1");
  await expect(option).not.toContainText("S01E01");
  await option.click();
  await expect(mapping).toContainText("正片 · 虚构章节1");
  await expect(page.getByText("电影文件", { exact: true })).toBeVisible();
  await expect(page.getByText("单集文件", { exact: true })).toHaveCount(0);
});

test("music mappings distinguish discs and support track number searches", async ({
  page,
}) => {
  await sourceFixture(page, ["01 - 虚构音轨.flac"]);
  await chooseWork(page, "music");
  await expect(
    page.getByRole("columnheader", { name: "对应曲目", exact: true }),
  ).toBeVisible();
  const mapping = page.getByRole("combobox", {
    name: "01 - 虚构音轨.flac 对应曲目",
    exact: true,
  });
  await mapping.click();
  await expect(
    page.locator('[role="option"][data-item-value="music-1"]'),
  ).toContainText("第 1 碟 · 曲目 01");
  await expect(
    page.locator('[role="option"][data-item-value="music-2"]'),
  ).toContainText("第 2 碟 · 曲目 01");
  await page
    .getByPlaceholder("搜索碟号、曲目号、译名或原名…")
    .fill("CD2 Track1");
  await page.locator('[role="option"][data-item-value="music-2"]').click();
  await expect(mapping).toContainText("第 2 碟 · 曲目 01");
  await expect(mapping).not.toContainText("S02E01");
  await expect(page.getByText("单曲文件", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("combobox", { name: "批量映射碟号", exact: true }),
  ).toBeVisible();
  await page.screenshot({
    path: "test-results/music-mapping.png",
    fullPage: true,
  });
});

test("switching back to TV keeps season episode labels and range controls", async ({
  page,
}) => {
  await sourceFixture(page);
  await chooseWork(page, "movie");
  await chooseWork(page, "tv");
  const mapping = page.getByRole("combobox", {
    name: "虚构远航 S02E01.mkv 对应集数",
    exact: true,
  });
  await mapping.click();
  await page.getByPlaceholder("搜索集号、译名或原名…").fill("S2E1");
  await page.locator('[role="option"][data-item-value="tv-2"]').click();
  await expect(mapping).toContainText("S02E01");
  await expect(
    page.getByRole("combobox", {
      name: "虚构远航 S02E01.mkv 多集结束位置",
      exact: true,
    }),
  ).toBeEnabled();
});
