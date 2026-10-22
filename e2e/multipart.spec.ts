import { expect, test } from "@playwright/test";
import { chooseWork, sourceFixture } from "./source-fixture";

test("episode coverage and explicit parts remain separate and support manual corrections", async ({
  page,
}) => {
  const names = ["Fictional.S01E01.Part1.mkv", "Fictional.S01E01.Part2.mkv"];
  await sourceFixture(page, names, "D:\\Incoming", 3);
  await chooseWork(page, "tv");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  await expect(
    page.getByRole("columnheader", { name: "包含集数", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("columnheader", { name: "文件范围", exact: true }),
  ).toHaveCount(0);
  for (const [i, name] of names.entries()) {
    await expect(
      page.getByRole("combobox", { name: `${name} 对应集数`, exact: true }),
    ).toContainText("S01E01");
    await expect(
      page.getByRole("combobox", { name: `${name} 分段编号`, exact: true }),
    ).toContainText(`Part ${i + 1}`);
    await expect(
      page.getByRole("combobox", { name: `${name} 多集结束位置`, exact: true }),
    ).toBeDisabled();
  }
  await expect(
    page.getByRole("region", { name: "季度匹配概况" }),
  ).toContainText("已匹配 1 集");
  await expect(
    page.getByText(
      "同一集的多个 part 请匹配到相同集数，再分别选择编号。已匹配集数不表示所有 part 均已齐全。",
    ),
  ).toBeVisible();
  await page.screenshot({
    path: "test-results/multipart-organize.png",
    fullPage: true,
  });
  const part = page.getByRole("combobox", {
    name: `${names[1]} 分段编号`,
    exact: true,
  });
  await part.click();
  await page.getByRole("option", { name: "不分段", exact: true }).click();
  await expect(
    page.getByRole("combobox", {
      name: `${names[1]} 多集结束位置`,
      exact: true,
    }),
  ).toBeEnabled();
  await part.click();
  await page.getByRole("option", { name: "Part 2", exact: true }).click();
  const mappings = await page.evaluate(() =>
    Object.values(JSON.parse(localStorage.getItem("bangumi.mappings")!)),
  );
  expect(mappings).toMatchObject([
    { episode_ids: ["tv-1"], part: 1 },
    { episode_ids: ["tv-1"], part: 2 },
  ]);
  await page.setViewportSize({ width: 1050, height: 760 });
  await page.keyboard.press("Escape");
  await expect(page.getByRole("listbox")).toHaveCount(0);
  const filenameWidth = await page
    .locator(".file-table th")
    .nth(1)
    .evaluate((el) => el.getBoundingClientRect().width);
  expect(filenameWidth).toBeGreaterThanOrEqual(220);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.locator(".file-table th").nth(1).scrollIntoViewIfNeeded();
  await page.screenshot({
    path: "test-results/multipart-organize-1050.png",
    fullPage: true,
  });
});

for (const suffix of ["mkv", "chs&jpn.ass"]) {
  test(`movie numeric suffixes map to distinct parts for ${suffix}`, async ({
    page,
  }) => {
    const names = [1, 2, 3].map((n) => `Fictional Feature - 0${n}.${suffix}`);
    await sourceFixture(page, names);
    await chooseWork(page, "movie");
    await page.getByRole("button", { name: "全选", exact: true }).click();
    await page
      .getByRole("button", { name: "应用批量匹配", exact: true })
      .click();
    for (const [i, name] of names.entries()) {
      await expect(
        page.getByRole("combobox", { name: `${name} 对应内容`, exact: true }),
      ).toContainText("正片");
      await expect(
        page.getByRole("combobox", { name: `${name} 分段编号`, exact: true }),
      ).toContainText(`Part ${i + 1}`);
    }
  });
}

test("standalone chs subtitles retain episode and part matching", async ({
  page,
}) => {
  const names = [
    "Fictional.S01E02.Part1.chs.ass",
    "Fictional.S01E02.Part2.chs.ass",
  ];
  await sourceFixture(page, names, "D:\\Captions", 3);
  await chooseWork(page, "tv");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  for (const [i, name] of names.entries()) {
    await expect(
      page.getByRole("combobox", { name: `${name} 对应集数`, exact: true }),
    ).toContainText("S01E02");
    await expect(
      page.getByRole("combobox", { name: `${name} 分段编号`, exact: true }),
    ).toContainText(`Part ${i + 1}`);
  }
});
