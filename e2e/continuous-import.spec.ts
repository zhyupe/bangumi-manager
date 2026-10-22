import { expect, test, type Page } from "@playwright/test";
import { sourceFixture } from "./source-fixture";

async function fixture(page: Page, resultCount = 2, longTitles = false) {
  await sourceFixture(page, ["S01E01.mkv"]);
  await page.evaluate(
    ({ resultCount, longTitles }) => {
      const runtime = (window as any).__TAURI_INTERNALS__;
      const original = runtime.invoke;
      const works: any[] = [];
      const calls: any[] = [];
      (window as any).__importCalls = calls;
      (window as any).__importedWorks = works;
      const subject = (id: string, provider: string, kind: string) => ({
        id,
        provider,
        kind,
        title: longTitles
          ? `Fictional-${id}-${"LongTitle".repeat(12)}`
          : `虚构条目-${id}`,
        original_title: longTitles
          ? `Fictional-${id}-${"LongTitle".repeat(12)}`
          : `虚构条目-${id}`,
        year: "2020",
        overview: "",
        poster: "",
        cached: false,
        episodes: [
          { id: `${id}-1`, title: "虚构章节", season: 1, sort: 1, kind: 0 },
        ],
        seasons: longTitles
          ? Array.from({ length: 30 }, (_, i) => ({
              number: i + 1,
              title: `虚构季度 ${i + 1}`,
            }))
          : [],
        relations:
          provider === "bangumi" && id === "910000031"
            ? [{ id: "910000032", title: "虚构后续季度", relation: "续集" }]
            : [],
      });
      runtime.invoke = async (command: string, args: any = {}) => {
        calls.push({ command, args });
        if (command === "snapshot") {
          const snapshot = await original(command, args);
          return {
            ...snapshot,
            works: [...snapshot.works, ...structuredClone(works)],
          };
        }
        if (command === "search_media") {
          return Array.from({ length: resultCount }, (_, i) =>
            subject(String(910000031 + i), args.provider, args.kind),
          );
        }
        if (command === "media_detail") {
          return subject(args.id, args.provider, args.kind);
        }
        if (command === "bind_subject") {
          if ((window as any).__holdImport) {
            await new Promise<void>((resolve) => {
              (window as any).__finishImport = resolve;
            });
          }
          if ((window as any).__failImport) throw new Error("模拟导入失败");
          const request = args.request;
          let work = works.find((item) => item.id === request.work_id);
          if (!work) {
            work = {
              id: `added-${works.length + 1}`,
              title: request.title,
              kind: request.kind,
              year: "2020",
              overview: "",
              poster: "",
              folder_name: `added-${works.length + 1}`,
              parent_id: request.parent_id,
              tmdb_id: request.tmdb_id,
              episodes: [],
              seasons: [],
              bindings: [],
            };
            works.push(work);
          }
          const existingBinding = work.bindings.find(
            (binding: any) =>
              binding.provider === request.subject.provider &&
              binding.source_kind === request.subject.kind &&
              binding.remote_id === request.subject.id,
          );
          const bindingId =
            existingBinding?.id ||
            `${work.id}-${request.subject.provider}-${request.subject.kind}-${request.subject.id}`;
          if (!existingBinding)
            work.bindings.push({
              id: bindingId,
              provider: request.subject.provider,
              remote_id: request.subject.id,
              source_kind: request.subject.kind,
              source_title: request.subject.title,
              season: request.season,
            });
          work.episodes.push(
            ...request.subject.episodes
              .filter(
                (episode: any) =>
                  !work.episodes.some(
                    (local: any) =>
                      local.binding_id === bindingId &&
                      local.remote_id === episode.id,
                  ),
              )
              .map((episode: any) => ({
                ...episode,
                id: `${bindingId}-${episode.id}`,
                remote_id: episode.id,
                binding_id: bindingId,
                season: request.season,
                number: episode.sort,
                enabled: true,
              })),
          );
          return structuredClone(work);
        }
        return original(command, args);
      };
    },
    { resultCount, longTitles },
  );
  await page.getByRole("button", { name: "添加作品", exact: true }).click();
  return page.getByRole("dialog");
}

for (const provider of ["tmdb", "bangumi", "musicbrainz"]) {
  test(`reopening an imported ${provider} entry updates the same local record`, async ({
    page,
  }) => {
    const dialog = await fixture(page);
    const selectProvider = async () => {
      if (provider === "musicbrainz") {
        await dialog
          .getByRole("combobox", { name: "类型", exact: true })
          .click();
        await page
          .getByRole("option", { name: "音乐专辑", exact: true })
          .click();
      }
      await dialog.getByLabel("数据源").click();
      await page.locator(`[role="option"][data-value="${provider}"]`).click();
    };
    await selectProvider();
    await dialog.getByLabel("远程条目 ID").fill("910000031");
    await dialog.getByRole("button", { name: "加载 ID" }).click();
    await dialog.getByRole("button", { name: "加入本地目录" }).click();
    await expect(dialog.getByRole("status")).toContainText("已添加到");
    await dialog.getByRole("button", { name: "关闭添加窗口" }).click();
    await page.getByRole("button", { name: "添加作品", exact: true }).click();
    await selectProvider();
    if (provider !== "musicbrainz") {
      await dialog.getByLabel("搜索词", { exact: true }).fill("虚构条目");
      await dialog.getByRole("button", { name: "搜索", exact: true }).click();
      await expect(dialog.locator(".result").first()).toContainText("已入库");
      await dialog.locator(".result").first().click();
    } else {
      await dialog.getByLabel("远程条目 ID").fill("910000031");
      await dialog.getByRole("button", { name: "加载 ID" }).click();
    }
    await expect(dialog.getByLabel("关联本地条目")).toHaveAttribute(
      "data-value",
      "added-1",
    );
    await expect(dialog.getByLabel("关联本地条目")).toBeDisabled();
    await expect(
      dialog.getByRole("button", { name: "加入本地目录" }),
    ).toHaveCount(0);
    await dialog.getByRole("button", { name: "更新本地目录信息" }).click();
    await expect(dialog.getByRole("status")).toContainText(
      "已更新「虚构条目-910000031」的本地目录信息",
    );
    const result = await page.evaluate(() => ({
      works: (window as any).__importedWorks,
      request: (window as any).__importCalls
        .filter((call: any) => call.command === "bind_subject")
        .at(-1).args.request,
    }));
    expect(result.works).toHaveLength(1);
    expect(result.works[0].bindings).toHaveLength(1);
    expect(result.works[0].episodes).toHaveLength(1);
    expect(result.request.work_id).toBe("added-1");
  });
}

test("adds multiple search results without closing, blocks duplicate submits and retries a failed entry", async ({
  page,
}) => {
  const dialog = await fixture(page);
  await dialog.getByLabel("搜索词", { exact: true }).fill("虚构条目");
  await dialog.getByRole("button", { name: "搜索", exact: true }).click();
  await dialog.locator(".result").first().click();
  await page.evaluate(() => {
    (window as any).__holdImport = true;
  });
  const add = dialog.getByRole("button", { name: "加入本地目录", exact: true });
  await add.evaluate((button) => {
    (button as HTMLButtonElement).click();
    (button as HTMLButtonElement).click();
  });
  await expect(add).toBeDisabled();
  await expect(
    dialog.getByRole("button", { name: "关闭添加窗口" }),
  ).toBeDisabled();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__importCalls.filter(
            (c: any) => c.command === "bind_subject",
          ).length,
      ),
    )
    .toBe(1);
  await page.evaluate(() => {
    (window as any).__holdImport = false;
    (window as any).__finishImport();
  });
  await expect(dialog.getByRole("status")).toContainText(
    "已添加到「虚构条目-910000031」",
  );
  await expect(dialog.getByLabel("搜索词", { exact: true })).toHaveValue(
    "虚构条目",
  );
  await expect(dialog.locator(".result")).toHaveCount(2);
  await expect(dialog.locator(".result").first()).toContainText("本次已添加");
  await dialog.locator(".result").first().click();
  await expect(add).toHaveCount(0);
  await dialog.locator(".result").nth(1).click();
  await page.evaluate(() => {
    (window as any).__failImport = true;
  });
  await add.click();
  await expect(dialog.getByRole("alert")).toContainText("模拟导入失败");
  await expect(add).toBeEnabled();
  await expect(dialog.locator(".result").nth(1)).not.toContainText(
    "本次已添加",
  );
  await page.evaluate(() => {
    (window as any).__failImport = false;
  });
  await add.click();
  await expect(dialog.getByRole("status")).toContainText(
    "已添加到「虚构条目-910000032」",
  );
  await expect(dialog.getByRole("alert")).toHaveCount(0);
  const saved = await page.evaluate(() => (window as any).__importedWorks);
  expect(saved.map((work: any) => work.title)).toEqual([
    "虚构条目-910000031",
    "虚构条目-910000032",
  ]);
  await dialog.locator(".result").first().click();
  await expect(add).toHaveCount(0);
  await page.screenshot({
    path: "test-results/continuous-import.png",
    fullPage: true,
  });
  await dialog.getByRole("button", { name: "关闭添加窗口" }).click();
  await expect(dialog).toBeHidden();
  await expect(
    page.getByRole("combobox", { name: "整理到本地条目", exact: true }),
  ).toContainText("虚构条目-910000032");
});

test("keeps related entries available and links another season to the newly added title", async ({
  page,
}) => {
  const dialog = await fixture(page);
  await dialog.getByRole("combobox", { name: "数据源", exact: true }).click();
  await page.getByRole("option", { name: "Bangumi", exact: true }).click();
  await dialog.getByLabel("远程条目 ID").fill("910000031");
  await dialog.getByRole("button", { name: "加载 ID" }).click();
  await dialog.getByRole("button", { name: "加入本地目录" }).click();
  await expect(dialog.getByRole("status")).toContainText("已添加到");
  await dialog.getByRole("button", { name: "关联条目 · 1" }).click();
  await dialog.getByRole("button", { name: /虚构后续季度/ }).click();
  await dialog
    .getByRole("combobox", { name: "关联本地条目", exact: true })
    .click();
  await page
    .getByRole("option", { name: "虚构条目-910000031", exact: true })
    .click();
  await expect(dialog.getByLabel("本条目对应的本地季号")).toHaveValue("2");
  await dialog.getByRole("button", { name: "确认分季关联" }).click();
  await expect(dialog.getByRole("status")).toContainText(
    "已添加到「虚构条目-910000031」",
  );
  const saved = await page.evaluate(() => (window as any).__importedWorks);
  expect(saved).toHaveLength(1);
  expect(saved[0].bindings.map((binding: any) => binding.season)).toEqual([
    1, 2,
  ]);
});

test("a new movie does not inherit the previous series and source IDs are scoped by provider and kind", async ({
  page,
}) => {
  const dialog = await fixture(page);
  await dialog.getByRole("combobox", { name: "类型", exact: true }).click();
  await page
    .getByRole("option", { name: "电影 / 剧场版", exact: true })
    .click();
  await dialog.getByLabel("远程条目 ID").fill("910000031");
  await dialog.getByRole("button", { name: "加载 ID" }).click();
  await dialog
    .getByRole("combobox", { name: "所属系列（可选）", exact: true })
    .click();
  await page.getByRole("option", { name: "虚构作品-tv", exact: true }).click();
  await dialog.getByRole("button", { name: "加入本地目录" }).click();
  await expect(dialog.getByRole("status")).toContainText("已添加到");
  await dialog.getByLabel("远程条目 ID").fill("910000032");
  await dialog.getByRole("button", { name: "加载 ID" }).click();
  await expect(
    dialog.getByRole("combobox", { name: "所属系列（可选）", exact: true }),
  ).toContainText("独立电影");
  await dialog.getByRole("button", { name: "加入本地目录" }).click();
  await expect(dialog.getByRole("status")).toContainText("910000032");
  await dialog.getByRole("combobox", { name: "类型", exact: true }).click();
  await page.getByRole("option", { name: "剧集", exact: true }).click();
  await dialog.getByLabel("远程条目 ID").fill("910000031");
  await dialog.getByRole("button", { name: "加载 ID" }).click();
  await dialog.getByRole("button", { name: "加入本地目录" }).click();
  await expect(dialog.getByRole("status")).toContainText("910000031");
  await dialog.getByRole("combobox", { name: "数据源", exact: true }).click();
  await page.getByRole("option", { name: "Bangumi", exact: true }).click();
  await dialog.getByRole("button", { name: "加载 ID" }).click();
  await expect(
    dialog.getByRole("button", { name: "加入本地目录" }),
  ).toBeEnabled();
  const saved = await page.evaluate(() => (window as any).__importedWorks);
  expect(saved.map((work: any) => work.parent_id)).toEqual(["tv", "", ""]);
  expect(saved.map((work: any) => work.kind)).toEqual(["movie", "movie", "tv"]);
});

test("import columns scroll independently and stack in narrow windows without losing actions", async ({
  page,
}) => {
  const dialog = await fixture(page, 30, true);
  const search = dialog.getByRole("region", {
    name: "搜索数据库或输入条目 ID",
    exact: true,
  });
  const preview = dialog.getByRole("region", { name: "条目详情", exact: true });
  await expect(preview).toContainText("选择条目查看详情并确认添加");
  await dialog.getByLabel("搜索词", { exact: true }).fill("fictional");
  await dialog.getByRole("button", { name: "搜索", exact: true }).click();
  await dialog.locator(".result").nth(20).click();
  await preview
    .getByRole("button", { name: "查看季集预览", exact: true })
    .click();
  const leftScroll = await search.evaluate((element) => element.scrollTop);
  expect(leftScroll).toBeGreaterThan(0);
  await preview.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  expect(
    await preview.evaluate((element) => element.scrollTop),
  ).toBeGreaterThan(0);
  expect(await search.evaluate((element) => element.scrollTop)).toBe(
    leftScroll,
  );
  await expect(
    dialog.getByRole("button", { name: "关闭添加窗口" }),
  ).toBeInViewport();
  await dialog.locator(".result").nth(21).click();
  await expect(dialog.locator(".result").nth(21)).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect
    .poll(() => preview.evaluate((element) => element.scrollTop))
    .toBe(0);
  expect(await search.evaluate((element) => element.scrollTop)).toBeGreaterThan(
    0,
  );
  for (const viewport of [
    { width: 1440, height: 1000 },
    { width: 1050, height: 850 },
    { width: 900, height: 560 },
    { width: 700, height: 760 },
    { width: 480, height: 760 },
  ]) {
    await page.setViewportSize(viewport);
    const { left, right, bounds } = await dialog.evaluate((element) => ({
      left: element
        .querySelector(".remote-search")!
        .getBoundingClientRect()
        .toJSON(),
      right: element
        .querySelector(".remote-preview")!
        .getBoundingClientRect()
        .toJSON(),
      bounds: element.getBoundingClientRect().toJSON(),
    }));
    if (viewport.width > 860) {
      expect(left.x + left.width).toBeLessThanOrEqual(right.x + 1);
      expect(Math.abs(left.y - right.y)).toBeLessThan(1);
    } else {
      expect(left.y + left.height).toBeLessThanOrEqual(right.y + 1);
      expect(Math.abs(left.x - right.x)).toBeLessThan(1);
    }
    expect(bounds.x).toBeGreaterThanOrEqual(0);
    expect(bounds.y).toBeGreaterThanOrEqual(0);
    expect(bounds.x + bounds.width).toBeLessThanOrEqual(viewport.width + 1);
    expect(bounds.y + bounds.height).toBeLessThanOrEqual(viewport.height + 1);
    for (const panel of [dialog, search, preview]) {
      expect(
        await panel.evaluate(
          (element) => element.scrollWidth <= element.clientWidth + 1,
        ),
      ).toBe(true);
    }
    const add = preview.getByRole("button", {
      name: "加入本地目录",
      exact: true,
    });
    await add.scrollIntoViewIfNeeded();
    await expect(add).toBeInViewport();
    await expect(
      dialog.getByRole("button", { name: "关闭添加窗口" }),
    ).toBeInViewport();
    await page.screenshot({
      path: `test-results/import-columns-${viewport.width}.png`,
      fullPage: true,
    });
  }
  await preview
    .getByRole("button", { name: "加入本地目录", exact: true })
    .click();
  await expect(preview.getByRole("status")).toContainText("已添加到");
  await dialog.getByRole("button", { name: "关闭添加窗口" }).click();
  await expect(dialog).toBeHidden();
});
