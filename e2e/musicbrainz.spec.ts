import { expect, test, type Page } from "@playwright/test";
import { sourceFixture } from "./source-fixture";

const releaseId = "00000000-0000-4000-8000-000000000001";
const groupId = "00000000-0000-4000-8000-000000000002";

async function fixture(page: Page) {
  await sourceFixture(
    page,
    ["01 - 虚构音轨.flac"],
    "D:\\Music\\虚构专辑\\Disc 01",
  );
  await page.evaluate(
    async ({ releaseId }) => {
      const runtime = (window as any).__TAURI_INTERNALS__;
      const original = runtime.invoke;
      const state = await original("snapshot");
      const release = {
        provider: "musicbrainz",
        id: releaseId,
        kind: "music",
        title: "虚构回声档案",
        original_title: "虚构回声档案",
        year: "2024",
        overview: "Official · 2024-03-01 · JP · CD · 2 碟 · TEST-001",
        poster: "",
        cached: false,
        info: {
          artist: "虚构演奏者甲 & 虚构演奏者乙",
          external_ids: { musicbrainz: releaseId },
        },
        seasons: [1, 2].map((number) => ({
          number,
          remote_number: number,
          title: number === 1 ? "主碟" : "附碟",
          remote_title: "",
          custom_title: false,
        })),
        episodes: [1, 2].map((season) => ({
          id: `00000000-0000-4000-8000-00000000001${season}`,
          title: `虚构音轨${season}`,
          sort: 1,
          season,
          kind: 0,
          info: {
            artist: "虚构演奏者甲",
            external_ids: {
              musicbrainz: "00000000-0000-4000-8000-000000000003",
            },
          },
        })),
        relations: [],
      };
      (window as any).__mbCalls = [];
      runtime.invoke = async (command: string, args: any = {}) => {
        (window as any).__mbCalls.push({ command, args });
        if (command === "snapshot") return structuredClone(state);
        if (command === "musicbrainz_releases") {
          if ((window as any).__mbError === "search")
            throw new Error("MusicBrainz 暂时限流，请稍后重试");
          if (args.query === "no-results")
            return { items: [], total: 0, next_offset: null };
          return {
            items: [
              {
                ...release,
                ...(args.offset
                  ? {
                      id: releaseId.slice(0, -1) + "9",
                      overview: "Digital Media · US · 虚构数字版本",
                    }
                  : {}),
              },
            ],
            total: 2,
            next_offset: args.offset ? null : 1,
          };
        }
        if (command === "media_detail" && args.provider === "musicbrainz") {
          if ((window as any).__mbError === "detail")
            throw new Error("HTTP 404：MusicBrainz 发行版本不存在");
          return { ...release, cached: !!(window as any).__mbCached };
        }
        if (command === "bind_subject") {
          const w = {
            id: "imported-music",
            title: args.request.title,
            kind: "music",
            folder_name: "music-sample",
            year: release.year,
            tmdb_id: "",
            parent_id: "",
            overview: release.overview,
            poster: "",
            info: release.info,
            bindings: [
              {
                id: "mb-binding",
                provider: "musicbrainz",
                remote_id: releaseId,
                source_kind: "music",
                source_title: release.title,
                season: 1,
              },
            ],
            seasons: release.seasons.map((s) => ({
              ...s,
              binding_id: "mb-binding",
            })),
            episodes: release.episodes.map((e) => ({
              ...e,
              id: `local-${e.season}`,
              remote_id: e.id,
              binding_id: "mb-binding",
              number: e.sort,
              enabled: true,
            })),
          };
          state.works.push(w);
          return structuredClone(w);
        }
        if (command === "sync_work")
          return structuredClone(
            state.works.find((w: any) => w.id === args.id),
          );
        if (command === "preview_naming")
          return { tv: "TV", movie: "Movie", music: "Music" };
        if (command === "diagnostics_report")
          return {
            statistics: [],
            recent: [],
            active: [],
            directory: "D:\\Data\\logs",
          };
        if (command === "save_settings") {
          state.settings = args.settings;
          return;
        }
        return original(command, args);
      };
    },
    { releaseId },
  );
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await page.getByRole("button", { name: "添加作品", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("combobox").first().click();
  await page.getByRole("option", { name: "MusicBrainz", exact: true }).click();
  return dialog;
}

test("MusicBrainz release search pages editions and imports disc tracks with source identity", async ({
  page,
}) => {
  const dialog = await fixture(page);
  await dialog.getByRole("button", { name: "搜索", exact: true }).click();
  await expect(dialog).toContainText("虚构演奏者甲 & 虚构演奏者乙");
  await expect(dialog).toContainText("已显示 1 / 2 个发行版本");
  await dialog.getByRole("button", { name: "加载更多发行版本" }).click();
  await expect(dialog).toContainText("已显示 2 / 2 个发行版本");
  await expect(dialog).toContainText("Digital Media");
  await dialog.locator(".result").first().click();
  await expect(dialog).toContainText("2 条音轨");
  await dialog
    .getByRole("button", { name: "第 2 碟 · 附碟 · 1 轨", exact: true })
    .click();
  await expect(
    dialog.getByRole("table", { name: "第 2 碟音轨" }),
  ).toContainText("虚构音轨2");
  await expect(dialog.getByText("本条目对应的本地季号")).toHaveCount(0);
  for (const width of [1440, 1050]) {
    await page.setViewportSize({ width, height: 850 });
    await expect(
      dialog.getByRole("button", { name: "加入本地目录" }),
    ).toBeEnabled();
    await dialog
      .getByRole("button", { name: "加入本地目录" })
      .scrollIntoViewIfNeeded();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: `test-results/musicbrainz-${width}.png`,
      fullPage: true,
      animations: "disabled",
    });
  }
  await dialog.getByRole("button", { name: "加入本地目录" }).click();
  const request = await page.evaluate(
    () =>
      (window as any).__mbCalls.find((c: any) => c.command === "bind_subject")
        .args.request,
  );
  expect(request).toMatchObject({
    kind: "music",
    subject: { provider: "musicbrainz", id: releaseId },
  });
  expect(request.subject.episodes.map((e: any) => e.season)).toEqual([1, 2]);
  await expect(dialog.getByRole("status")).toContainText("已添加到");
  await dialog.getByRole("button", { name: "关闭添加窗口" }).click();
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("tab", { name: "音乐", exact: true }).click();
  await page
    .getByRole("button", { name: /虚构回声档案/ })
    .first()
    .click();
  await page.locator('.season-menu-item[data-season="2"]').click();
  const sync = page.getByRole("button", { name: /同步本/ });
  await expect(sync).toBeEnabled();
  await sync.click();
  expect(
    await page.evaluate(() =>
      (window as any).__mbCalls.some(
        (c: any) => c.command === "sync_work" && c.args.season === 2,
      ),
    ),
  ).toBe(true);
});

test("MusicBrainz group links require an edition and errors keep import disabled until retry", async ({
  page,
}) => {
  const dialog = await fixture(page);
  await dialog
    .getByLabel("远程条目 ID")
    .fill(`https://musicbrainz.org/release-group/${groupId}`);
  await dialog.getByRole("button", { name: "加载 ID" }).click();
  await expect(dialog).toContainText("已显示 1 / 2 个发行版本");
  await expect(
    dialog.getByRole("button", { name: "加入本地目录" }),
  ).toHaveCount(0);
  await page.evaluate(() => {
    (window as any).__mbError = "detail";
  });
  await dialog.locator(".result").first().click();
  await expect(dialog.getByRole("alert")).toContainText("发行版本不存在");
  await expect(
    dialog.getByRole("button", { name: "加入本地目录" }),
  ).toHaveCount(0);
  await page.evaluate(() => {
    (window as any).__mbError = "";
    (window as any).__mbCached = true;
  });
  await dialog.getByLabel("远程条目 ID").fill(releaseId);
  await dialog.getByRole("button", { name: "加载 ID" }).click();
  await expect(dialog).toContainText("网络不可用，使用旧缓存");
  await page.evaluate(() => {
    (window as any).__mbError = "search";
  });
  await dialog.getByRole("button", { name: "搜索", exact: true }).click();
  await expect(dialog.getByRole("alert")).toContainText("暂时限流");
  await expect(
    dialog.getByRole("button", { name: "加入本地目录" }),
  ).toHaveCount(0);
  await page.evaluate(() => {
    (window as any).__mbError = "";
  });
  await dialog.getByLabel("搜索词", { exact: true }).fill("no-results");
  await dialog.getByRole("button", { name: "搜索", exact: true }).click();
  await expect(dialog).toContainText("未找到发行版本");
  await dialog.getByRole("combobox", { name: "类型", exact: true }).click();
  await page.getByRole("option", { name: "电影 / 剧场版" }).click();
  await expect(dialog.getByRole("combobox").first()).toContainText("TMDB");
  await dialog.getByRole("combobox").first().click();
  await expect(
    page.getByRole("option", { name: "MusicBrainz", exact: true }),
  ).toBeDisabled();
});

test("MusicBrainz contact uses the project default and can be changed in settings", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "关闭添加窗口", exact: true }).click();
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(
    page.getByLabel("MusicBrainz 联系地址", { exact: true }),
  ).toHaveValue("https://github.com/zhyupe/bangumi-manager");
  await page
    .getByLabel("MusicBrainz 联系地址", { exact: true })
    .fill("https://example.invalid/contact");
  await page.getByRole("button", { name: "保存全部设置", exact: true }).click();
  expect(
    await page.evaluate(
      () =>
        (window as any).__mbCalls.find(
          (c: any) => c.command === "save_settings",
        ).args.settings.musicbrainz_contact,
    ),
  ).toBe("https://example.invalid/contact");
});
