import { expect, test, type Page } from "@playwright/test";
import { sourceFixture } from "./source-fixture";

async function fixture(page: Page, scan = "current") {
  await sourceFixture(page, ["Fictional.S01E01.mkv"], "D:\\Incoming", 3);
  await page.evaluate((scan) => {
    const win = window as any;
    const invoke = win.__TAURI_INTERNALS__.invoke;
    win.__TAURI_INTERNALS__.invoke = async (command: string, args: any) => {
      if (command === "refresh_library") return {};
      const result = await invoke(command, args);
      if (command !== "snapshot") return result;
      const work = result.works.find((w: any) => w.kind === "tv");
      work.episodes.push(
        { ...work.episodes[0], id: "duplicate", binding_id: "other" },
        { ...work.episodes[0], id: "disabled", season: 0, enabled: false },
      );
      work.seasons.push(
        { ...work.seasons[0], number: 0, title: "Disabled specials" },
        { ...work.seasons[0], number: 3, title: "Unconfirmed season" },
      );
      const full = {
        ...work,
        id: "full",
        title: "Fictional Complete Collection",
      };
      full.seasons = work.seasons.filter((s: any) => s.number === 1);
      full.episodes = work.episodes.filter((e: any) => e.season === 1);
      const empty = {
        ...work,
        id: "empty",
        title: "Fictional Empty Collection",
        episodes: [],
        seasons: [],
      };
      const long = {
        ...work,
        id: "long",
        title: "Fictional Extended Collection",
      };
      long.seasons = Array.from({ length: 19 }, (_, i) => ({
        ...work.seasons[0],
        number: i + 1,
      }));
      long.episodes = long.seasons.map((s: any) => ({
        ...work.episodes[0],
        id: `long-${s.number}`,
        season: s.number,
      }));
      result.works.push(full, empty, long);
      const asset = (
        workId: string,
        season: number,
        number: number,
        end = number,
        version = "mkv",
      ) => ({
        path: `D:\\Library\\${workId}\\S${season}E${number}.${version}`,
        work_id: workId,
        season,
        number,
        end,
        signature: { size: 100, modified: 1 },
      });
      result.index.assets = [
        asset("tv", 1, 1, 3),
        asset("tv", 1, 1, 1, "mp4"),
        asset("tv", 2, 1),
        asset("tv", 2, 1, 1, "mp4"),
        asset("full", 1, 1, 3),
        asset("movie", 1, 1),
        asset("music", 1, 1),
        ...Array.from({ length: 19 }, (_, i) => asset("long", i + 1, 1)),
      ];
      result.index.scanned_at =
        scan === "unscanned" ? "" : "2026-10-07T01:00:00Z";
      result.index.available = scan !== "offline";
      return result;
    };
  }, scan);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: "扫描", exact: true }).click();
}

test("catalog cards show only overall coverage on the cover and keep season details inside", async ({
  page,
}) => {
  await fixture(page);
  const card = page.locator(".work-card").filter({ hasText: "虚构作品-tv" });
  await expect(card.locator(".work-collection-summary")).toHaveText(
    "已收齐 1/3 季",
  );
  await expect(card).not.toContainText("份资源");
  await expect(card.locator('[data-slot="badge"]')).toHaveCount(1);
  await expect(
    card.locator(".work-cover .work-collection-summary"),
  ).toBeVisible();
  await expect(
    card.locator(".work-season-collection, [data-season]"),
  ).toHaveCount(0);
  const full = page
    .locator(".work-card")
    .filter({ hasText: "Fictional Complete Collection" });
  await expect(full.locator(".work-collection-summary")).toHaveAttribute(
    "data-tone",
    "green",
  );
  await expect(
    page
      .locator(".work-card")
      .filter({ hasText: "Fictional Empty Collection" }),
  ).toContainText("暂无已启用集数");
  const extended = page
    .locator(".work-card")
    .filter({ hasText: "Fictional Extended Collection" });
  await expect(extended.locator(".work-collection-summary")).toHaveText(
    "已收齐 19/19 季",
  );
  await expect(extended.locator('[data-slot="badge"]')).toHaveCount(1);
  for (const width of [1440, 1050]) {
    await page.setViewportSize({ width, height: 1000 });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    expect(
      await page.locator(".work-collection-summary").evaluateAll((elements) =>
        elements.every((el) => {
          const badge = el.getBoundingClientRect();
          const cover = el.closest(".work-cover")!.getBoundingClientRect();
          return (
            badge.right <= cover.right &&
            Math.abs(badge.left - cover.left - 11) <= 1 &&
            Math.abs(cover.bottom - badge.bottom - 11) <= 1
          );
        }),
      ),
    ).toBe(true);
    await page.screenshot({
      path: `test-results/catalog-collection-${width}.png`,
      fullPage: true,
    });
  }
  await card.click();
  await expect(
    page.locator('.season-menu-item[data-season="1"] .season-completeness'),
  ).toHaveText("3/3");
  await expect(
    page.locator('.season-menu-item[data-season="2"] .season-completeness'),
  ).toHaveText("1/3");
  for (const kind of ["电影", "音乐"]) {
    await page.getByRole("tab", { name: kind, exact: true }).click();
    await expect(page.locator(".work-cover [data-slot=badge]")).toHaveText(
      "1 份资源",
    );
    await expect(page.locator(".work-season-collection")).toHaveCount(0);
  }
});

for (const scan of ["offline", "unscanned"]) {
  test(`catalog collection stays unconfirmed when ${scan}`, async ({
    page,
  }) => {
    await fixture(page, scan);
    const card = page
      .locator(".work-card")
      .filter({ hasText: "Fictional Complete Collection" });
    await expect(card.locator(".work-collection-summary")).toHaveText(
      scan === "offline" ? "离线 · 上次记录" : "等待扫描",
    );
    await expect(card.locator('[data-tone="green"]')).toHaveCount(0);
    await expect(card.locator('[data-slot="badge"]')).toHaveCount(1);
  });
}

test("catalog season status uses the selected interface language", async ({
  page,
}) => {
  await fixture(page);
  await page.evaluate(() => {
    localStorage.setItem("bangumi.uiLanguage", "en");
    window.dispatchEvent(
      new StorageEvent("storage", { key: "bangumi.uiLanguage" }),
    );
  });
  const card = page.locator(".work-card").filter({ hasText: "虚构作品-tv" });
  await expect(card.locator(".work-collection-summary")).toHaveText(
    "1/3 seasons complete",
  );
  await expect(
    page
      .locator(".work-card")
      .filter({ hasText: "Fictional Complete Collection" })
      .locator(".work-collection-summary"),
  ).toHaveText("1/1 season complete");
  await page.setViewportSize({ width: 1050, height: 1000 });
  await page.screenshot({
    path: "test-results/catalog-collection-en.png",
    fullPage: true,
  });
});
