import { expect, test } from "@playwright/test";
import { chooseWork, sourceFixture } from "./source-fixture";

test("bracketed episode numbers take precedence over numbers in the title", async ({
  page,
}) => {
  const names = [
    "[Fictional Journey to the Floating Isles! 2][09][720P][GB].mp4",
    "[Fictional Journey to the Floating Isles! 2][10][720P][GB].mp4",
  ];
  await sourceFixture(page, names, "D:\\Incoming", 10);
  await chooseWork(page, "tv");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await expect(page.getByLabel("数字位置", { exact: false })).toContainText(
    "示例 9",
  );
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  for (const [index, name] of names.entries()) {
    await expect(
      page.getByRole("combobox", { name: `${name} 对应集数`, exact: true }),
    ).toContainText(index === 0 ? "S01E09" : "S01E10");
  }
  await expect(page.getByRole("status")).toContainText("已匹配 2 / 2 项");
});

test("numeric matching applies attached and SxxExx episode markers and shows its result", async ({
  page,
}) => {
  const names = [
    "fictionalQe01.ts",
    "fictionalQe02.ts",
    "Fictional.Series.S01E02.xxx.mkv",
  ];
  await sourceFixture(page, names, "D:\\Incoming", 3);
  await chooseWork(page, "tv");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await expect(page.getByLabel("数字位置", { exact: false })).toContainText(
    "示例",
  );
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  for (const [index, name] of names.entries()) {
    await expect(
      page.getByRole("combobox", { name: `${name} 对应集数`, exact: true }),
    ).toContainText(index === 0 ? "S01E01" : "S01E02");
  }
  await expect(page.getByRole("status")).toContainText("已匹配 3 / 3 项");
  await expect(page.locator(".source-plan-actions")).toContainText(
    "已匹配 3 / 3 项",
  );
  await page.screenshot({
    path: "test-results/number-matching.png",
    fullPage: true,
  });
});

test("changing selection resets an unavailable numeric field and offset still applies", async ({
  page,
}) => {
  const names = ["Fictional_7.01.mkv", "fictionalQe02.ts"];
  await sourceFixture(page, names, "D:\\Incoming", 3);
  await chooseWork(page, "tv");
  await page
    .getByRole("checkbox", { name: `选择 ${names[0]}`, exact: true })
    .click();
  const field = page.getByLabel("数字位置", { exact: false });
  await field.click();
  await page.locator('[role="option"][data-value="1"]').click();
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  await expect(
    page.getByRole("combobox", { name: `${names[0]} 对应集数`, exact: true }),
  ).toContainText("S01E01");
  await page.getByRole("button", { name: "全不选", exact: true }).click();
  await page
    .getByRole("checkbox", { name: `选择 ${names[1]}`, exact: true })
    .click();
  await expect(field).toHaveAttribute("data-value", "0");
  await page.getByLabel("集数偏移", { exact: true }).fill("1");
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  await expect(
    page.getByRole("combobox", { name: `${names[1]} 对应集数`, exact: true }),
  ).toContainText("S01E03");
  await expect(
    page.getByRole("combobox", { name: `${names[0]} 对应集数`, exact: true }),
  ).toContainText("S01E01");
  await expect(page.getByRole("status")).toContainText("已匹配 1 / 1 项");
});

test("no matching number reports a reason and clears stale feedback on setting changes", async ({
  page,
}) => {
  await sourceFixture(page, ["Fictional.Series.2026.1080p.H264.mkv"]);
  await chooseWork(page, "tv");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("已匹配 0 / 1 项");
  await expect(page.getByRole("status")).toContainText("未识别到可用数字");
  await expect(
    page.getByRole("button", { name: "创建并查看", exact: true }),
  ).toBeDisabled();
  await page.getByLabel("集数偏移", { exact: true }).fill("1");
  await expect(page.getByRole("status")).toHaveCount(0);
});

test("an unavailable episode reports a partial match without assigning another episode", async ({
  page,
}) => {
  await sourceFixture(page, ["fictionalQe01.ts", "fictionalQe09.ts"]);
  await chooseWork(page, "tv");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("已匹配 1 / 2 项");
  await expect(page.getByRole("status")).toContainText("未能唯一匹配");
  await expect(
    page.getByRole("combobox", {
      name: "fictionalQe09.ts 对应集数",
      exact: true,
    }),
  ).toHaveAttribute("data-value", "");
});
