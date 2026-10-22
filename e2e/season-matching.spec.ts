import { expect, test } from "@playwright/test";
import { chooseWork, sourceFixture } from "./source-fixture";

test("season totals track unique matched episodes and missing ranges across selection and file filters", async ({
  page,
}) => {
  const names = [
    "fictionalQe01.ts",
    "fictionalQe03.ts",
    "fictionalQe01.alt.mkv",
  ];
  await sourceFixture(page, names, "D:\\Incoming", 4);
  await chooseWork(page, "tv");
  const summary = page.getByRole("region", {
    name: "季度匹配概况",
    exact: true,
  });
  await expect(summary).toContainText("共 4 集");
  await expect(summary).toContainText("已匹配 0 集");
  await expect(summary).toContainText("未匹配 4 集：EP01–EP04");
  const season = page.getByRole("combobox", {
    name: "批量映射季号",
    exact: true,
  });
  expect((await summary.boundingBox())!.y).toBeGreaterThan(
    (await season.boundingBox())!.y,
  );
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  await expect(summary).toContainText("已匹配 2 集");
  await expect(summary).toContainText("未匹配 2 集：EP02、EP04");
  await page.getByRole("button", { name: "全不选", exact: true }).click();
  await page
    .getByRole("textbox", { name: "筛选文件", exact: true })
    .fill("e03");
  await expect(summary).toContainText("已匹配 2 集");
  await page.getByRole("textbox", { name: "筛选文件", exact: true }).fill("");
  await page
    .getByRole("combobox", { name: `${names[0]} 多集结束位置`, exact: true })
    .click();
  await page.locator('[role="option"][data-item-value="tv-1-2"]').click();
  await expect(summary).toContainText("已匹配 3 集");
  await expect(summary).toContainText("未匹配 1 集：EP04");
  await page
    .getByRole("combobox", { name: `${names[2]} 对应集数`, exact: true })
    .click();
  await page.locator('[role="option"][data-item-value="tv-1-4"]').click();
  await expect(summary).toContainText("已匹配 4 集");
  await expect(summary).toContainText("已全部匹配");
  await page
    .getByRole("checkbox", { name: `选择 ${names[2]}`, exact: true })
    .click();
  await page
    .getByRole("button", { name: "选中项作为附加文件", exact: true })
    .click();
  await expect(summary).toContainText("未匹配 1 集：EP04");
  await page.setViewportSize({ width: 1050, height: 800 });
  await summary.scrollIntoViewIfNeeded();
  await expect(summary).toBeInViewport();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/season-matching.png",
    fullPage: true,
  });
});

test("coverage follows the selected season and uses track terminology for music", async ({
  page,
}) => {
  await sourceFixture(page, ["fictionalQe01.ts"], "D:\\Incoming", 3);
  await chooseWork(page, "tv");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  const summary = page.getByRole("region", {
    name: "季度匹配概况",
    exact: true,
  });
  await expect(summary).toContainText("已匹配 1 集");
  await page
    .getByRole("combobox", { name: "批量映射季号", exact: true })
    .click();
  await page.locator('[role="option"][data-value="2"]').click();
  await expect(summary).toContainText("共 3 集");
  await expect(summary).toContainText("已匹配 0 集");
  await expect(summary).toContainText("未匹配 3 集：EP01–EP03");
  await chooseWork(page, "music");
  await expect(summary).toHaveCount(0);
  const tracks = page.getByRole("region", {
    name: "碟片匹配概况",
    exact: true,
  });
  await expect(tracks).toContainText("共 3 首");
  await expect(tracks).toContainText("未匹配 3 首：曲目 01–曲目 03");
  await chooseWork(page, "movie");
  await expect(tracks).toHaveCount(0);
  await expect(summary).toHaveCount(0);
});
