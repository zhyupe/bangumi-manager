import { expect, test } from "@playwright/test";
import { sourceFixture } from "./source-fixture";

test("catalog remembers sorting and opens a currently airing title", async ({
  page,
}) => {
  await sourceFixture(page, ["Fictional.S01E01.mkv"]);
  await page.evaluate(() => {
    const win = window as any;
    const invoke = win.__TAURI_INTERNALS__.invoke;
    win.__TAURI_INTERNALS__.invoke = async (command: string, args: any) => {
      if (command === "refresh_library") return {};
      const result = await invoke(command, args);
      if (command !== "snapshot") return result;
      const base = result.works.find((w: any) => w.kind === "tv");
      const date = (offset: number) => {
        const d = new Date();
        d.setDate(d.getDate() + offset);
        return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
      };
      result.works = [
        {
          ...base,
          id: "a",
          title: "Fictional 10",
          created_at: "2026-02-01T00:00:00Z",
          info: { air_date: "2024-01-01" },
          seasons: [],
          episodes: [],
        },
        {
          ...base,
          id: "b",
          title: "Fictional 2",
          created_at: "2026-01-01T00:00:00Z",
          info: { air_date: "2025-01-01" },
          seasons: [{ ...base.seasons[0], number: 1 }],
          episodes: [-7, 7].map((offset, i) => ({
            ...base.episodes[0],
            id: `ep-${i}`,
            kind: 0,
            season: 1,
            number: i + 1,
            info: { air_date: date(offset) },
          })),
        },
      ];
      return result;
    };
  });
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: "扫描", exact: true }).click();
  const titles = page.locator(".work-card-title");
  await expect(titles).toHaveText(["Fictional 2", "Fictional 10"]);
  await page.getByRole("combobox", { name: "展示顺序" }).click();
  await page.getByRole("option", { name: "按添加时间排序" }).click();
  await expect(titles).toHaveText(["Fictional 10", "Fictional 2"]);
  await page.getByRole("combobox", { name: "排序方向" }).click();
  await page.getByRole("option", { name: "升序", exact: true }).click();
  await expect(titles).toHaveText(["Fictional 2", "Fictional 10"]);
  await page.getByRole("combobox", { name: "展示顺序" }).click();
  await page.getByRole("option", { name: "按发布时间排序" }).click();
  await expect(titles).toHaveText(["Fictional 2", "Fictional 10"]);
  expect(
    await page.evaluate(() => localStorage.getItem("bangumi.catalog.sort")),
  ).toBe("release");
  await expect(page.locator(".catalog-airing-item")).toHaveCount(1);
  await page.screenshot({ path: testInfo.outputPath("catalog-desktop.png") });
  await page.setViewportSize({ width: 850, height: 850 });
  await page.screenshot({ path: testInfo.outputPath("catalog-compact.png") });
  await page.locator(".catalog-airing-item").click();
  await expect(page.locator(".catalog-search")).toHaveCount(0);
  await expect(
    page.getByText("Fictional 2", { exact: true }).first(),
  ).toBeVisible();
});

