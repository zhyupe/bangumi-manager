import { expect, test } from "@playwright/test";
import { chooseWork, sourceFixture } from "./source-fixture";

for (const kind of ["tv", "movie", "music"]) {
  test(`directory extras remain separate from ${kind} episodes during matching`, async ({
    page,
  }) => {
    const main = kind === "music" ? "01.flac" : "S01E01.mkv";
    const names = [main, "clip.mkv", "clip.srt", "cover.jpg"];
    await sourceFixture(page, names, "D:\\Incoming", 1, {
      "clip.mkv": "Interview/clip.mkv",
      "clip.srt": "Interview/clip.srt",
      "cover.jpg": "Scans/Book/cover.jpg",
    });
    await page.getByRole("button", { name: "全选", exact: true }).click();
    await chooseWork(page, kind);
    const position =
      kind === "music"
        ? "对应曲目"
        : kind === "movie"
          ? "对应内容"
          : "对应集数";
    for (const name of names.slice(1)) {
      await expect(
        page.getByRole("combobox", {
          name: `${name} ${position}`,
          exact: true,
        }),
      ).toContainText("附加文件");
    }
    await expect(
      page
        .locator(".source-plan-actions")
        .getByText("已匹配 3 / 4 项", { exact: true }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "应用批量匹配", exact: true })
      .click();
    await expect(page.getByRole("status")).toContainText("已匹配 4 / 4 项");
    await expect
      .poll(async () =>
        page.evaluate(() =>
          JSON.parse(localStorage.getItem("bangumi.mappings") || "{}"),
        ),
      )
      .toMatchObject({
        [`D:\\Incoming\\${main}`]: { work_id: kind, extra: false },
        ...Object.fromEntries(
          names
            .slice(1)
            .map((name) => [
              `D:\\Incoming\\${name}`,
              { work_id: kind, extra: true, episode_ids: [] },
            ]),
        ),
      });
  });
}

test("automatic extras respect a manual episode assignment and clearing the match", async ({
  page,
}) => {
  await sourceFixture(page, ["clip.mkv", "cover.jpg"], "D:\\Incoming", 1, {
    "clip.mkv": "Interview/clip.mkv",
  });
  await chooseWork(page, "tv");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  const field = page.getByRole("combobox", {
    name: "clip.mkv 对应集数",
    exact: true,
  });
  await expect(field).toContainText("附加文件");
  await field.click();
  await page.locator('[role="option"][data-item-value="tv-1"]').click();
  await expect(field).toContainText("S01E01");
  await page.getByRole("button", { name: "全不选", exact: true }).click();
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await expect(field).toContainText("S01E01");
  await field.click();
  await page.getByRole("option", { name: "待匹配", exact: true }).click();
  await expect(field).toContainText("待匹配");
  await expect(page.locator(".source-plan-actions")).toContainText(
    "已匹配 0 / 2 项",
  );
});
