import { expect, test } from "@playwright/test";
import { chooseWork, sourceFixture } from "./source-fixture";

test("adding a work searches shared words from only selected videos using TMDB", async ({
  page,
}) => {
  const names = [
    "[虚构星图][S02E01][1080p][H264].mkv",
    "[虚构星图][S02E02][720p][HEVC].mkv",
    "虚构未选影片.mkv",
  ];
  await sourceFixture(page, names);
  await chooseWork(page, "music");
  for (const name of names.slice(0, 2))
    await page
      .getByRole("checkbox", { name: `选择 ${name}`, exact: true })
      .click();
  await page.getByRole("button", { name: "添加作品", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByLabel("搜索词", { exact: true })).toHaveValue(
    "虚构星图",
  );
  await expect(dialog.getByRole("combobox").first()).toContainText("TMDB");
  await dialog.getByRole("button", { name: "搜索", exact: true }).click();
  const call = await page.evaluate(() =>
    (window as any).__sourceCalls.find(
      (c: any) => c.command === "search_media",
    ),
  );
  expect(call.args).toMatchObject({
    query: "虚构星图",
    provider: "tmdb",
    kind: "tv",
  });
  await page.screenshot({
    path: "test-results/selected-file-search.png",
    fullPage: true,
  });
});

test("selected music uses the album folder and changing to movies switches to TMDB", async ({
  page,
}) => {
  const names = ["01 - 虚构音轨甲.flac", "02 - 虚构音轨乙.flac"];
  await sourceFixture(page, names, "D:\\Music\\虚构回响\\Disc 01");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await page.getByRole("button", { name: "添加作品", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByLabel("搜索词", { exact: true })).toHaveValue(
    "虚构回响",
  );
  await expect(dialog.getByRole("combobox").first()).toContainText("Bangumi");
  await expect(
    dialog.getByRole("combobox", { name: "类型", exact: true }),
  ).toContainText("音乐专辑");
  await dialog.getByLabel("搜索词", { exact: true }).fill("手动修订的搜索词");
  await dialog.getByRole("combobox", { name: "类型", exact: true }).click();
  await page.locator('[role="option"][data-value="movie"]').click();
  await expect(dialog.getByRole("combobox").first()).toContainText("TMDB");
  await expect(dialog.getByLabel("搜索词", { exact: true })).toHaveValue(
    "手动修订的搜索词",
  );
});

test("reopening add work follows a changed selection and no selection stays blank", async ({
  page,
}) => {
  await sourceFixture(page, ["虚构影片甲.mkv", "虚构影片乙.mkv"]);
  await page
    .getByRole("checkbox", { name: "选择 虚构影片甲.mkv", exact: true })
    .click();
  await page.getByRole("button", { name: "添加作品", exact: true }).click();
  await expect(page.getByLabel("搜索词", { exact: true })).toHaveValue(
    "虚构影片甲",
  );
  await page.getByRole("button", { name: "关闭添加窗口", exact: true }).click();
  await page
    .getByRole("checkbox", { name: "选择 虚构影片甲.mkv", exact: true })
    .click();
  await page
    .getByRole("checkbox", { name: "选择 虚构影片乙.mkv", exact: true })
    .click();
  await page.getByRole("button", { name: "添加作品", exact: true }).click();
  await expect(page.getByLabel("搜索词", { exact: true })).toHaveValue(
    "虚构影片乙",
  );
  await page.getByRole("button", { name: "关闭添加窗口", exact: true }).click();
  await page.getByRole("button", { name: "全不选", exact: true }).click();
  await page.getByRole("button", { name: "添加作品", exact: true }).click();
  await expect(page.getByLabel("搜索词", { exact: true })).toHaveValue("");
  await expect(
    page.getByRole("dialog").getByRole("combobox").first(),
  ).toContainText("TMDB");
});
