import { expect, test, type Page } from "@playwright/test";

async function fixture(
  page: Page,
  mode: "normal" | "partial" | "error" | "delayed" | "empty" = "normal",
) {
  await page.addInitScript(
    ({ mode }) => {
      (window as any).isTauri = true;
      const calls: any[] = [];
      (window as any).__nfoCalls = calls;
      const sources = [1, 2, 3].map((n) => ({
        path: `D:\\Incoming\\sample-${n}.mkv`,
        name: `sample-${n}.mkv`,
        parent: "D:\\Incoming",
        extension: "mkv",
        media: true,
        hint: "episode",
        signature: { size: 100, modified: 100 },
        probe: null,
      }));
      const state: any = {
        settings: {
          tv_root: "D:\\Library\\TV",
          movie_root: "D:\\Library\\Movies",
          extras_root: "D:\\Extras",
          backup_root: "D:\\Backups",
          ffprobe: "ffprobe",
          tmdb_token: "",
          verify_hash: false,
          default_operation: "copy",
        },
        works: [],
        sources,
        plans: [],
        data_dir: "D:\\Data",
        index: {
          assets: [],
          unbound: [],
          errors: [],
          scanned_at: "",
          available: true,
        },
        queue: { ids: [], paused: false, error: "" },
      };
      const matches = sources.map((s, i) => ({
        path: s.path,
        nfo_path: s.path.replace(".mkv", ".nfo"),
        title: `虚构影片${i + 1}`,
        kind: "movie",
        ids: { tmdb: `91000000${i + 1}`, imdb: `tt91000000${i + 1}` },
        episode_count: 1,
        existing_title: "",
        fingerprint: `stamp-${i + 1}`,
        error: i === 2 ? "NFO 中的 tmdb ID 冲突" : "",
      }));
      let attempts = 0,
        serial = 0;
      (window as any).__TAURI_INTERNALS__ = {
        metadata: {
          currentWindow: { label: "main" },
          currentWebview: { label: "main" },
        },
        transformCallback: () => ++serial,
        unregisterCallback: () => {},
        invoke: async (command: string, args: any = {}) => {
          calls.push({ command, args });
          if (command === "snapshot") return structuredClone(state);
          if (command === "inspect_source_nfos") {
            attempts++;
            if (mode === "error" && attempts === 1)
              throw new Error("共享暂时不可用");
            if (mode === "delayed" && attempts === 1)
              await new Promise((resolve) => {
                (window as any).__resolveNfo = resolve;
              });
            return mode === "empty" ? [] : matches;
          }
          if (command === "sync_source_nfos") {
            const mappings: any[] = [],
              errors: any[] = [];
            for (const item of args.selections) {
              if (mode === "partial" && item.path === sources[1].path) {
                errors.push({
                  ...matches[1],
                  error: "媒体或 NFO 已变化，请重新检测并确认",
                });
                continue;
              }
              const i = sources.findIndex((s) => s.path === item.path);
              const work = {
                id: `w${i}`,
                title: `虚构影片${i + 1}`,
                folder_name: `sample-${i + 1}`,
                kind: "movie",
                year: "",
                tmdb_id: `91000000${i + 1}`,
                parent_id: "",
                overview: "",
                poster: "",
                seasons: [],
                bindings: [
                  {
                    id: `b${i}`,
                    provider: "tmdb",
                    remote_id: `91000000${i + 1}`,
                    source_kind: "movie",
                    source_title: `虚构影片${i + 1}`,
                    season: 1,
                  },
                ],
                episodes: [
                  {
                    id: `e${i}`,
                    binding_id: `b${i}`,
                    remote_id: `91000000${i + 1}`,
                    title: "正片",
                    season: 1,
                    number: 1,
                    sort: 1,
                    kind: 0,
                    enabled: true,
                  },
                ],
              };
              if (!state.works.some((w: any) => w.id === work.id))
                state.works.push(work);
              mappings.push({
                source: item.path,
                work_id: work.id,
                episode_ids: [`e${i}`],
                extra: false,
              });
            }
            return { mappings, errors };
          }
          if (command === "plugin:event|listen") return args.handler;
          if (
            command === "plugin:event|unlisten" ||
            command === "record_frontend_timing"
          )
            return;
          if (["list_resources", "list_backups"].includes(command)) return [];
          throw new Error(`Unexpected command: ${command}`);
        },
      };
      (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener: () => {},
      };
    },
    { mode },
  );
  await page.goto("/");
  await expect(page.getByRole("button", { name: /NFO 快速同步/ })).toHaveCount(0);
  await page.getByRole("checkbox", { name: "选择筛选结果", exact: true }).click();
}

test("NFO selection happens in the sidebar and one successful sync closes the dialog", async ({ page }) => {
  await fixture(page);
  await page.getByRole("combobox", {name: "NFO 文件", exact: true}).click();
  await page.getByRole("option", {name: "D:\\Incoming\\sample-2.nfo", exact: true}).click();
  await page.getByRole("button", {name: "NFO 快速同步 · 2 项可同步", exact: true}).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("TMDB #910000002", {exact: true})).toBeVisible();
  await expect(dialog.getByText("TMDB #910000001", {exact: true})).toHaveCount(0);
  await expect(dialog.getByRole("checkbox")).toHaveCount(0);
  await dialog.getByRole("button", {name: "同步所选 1 项", exact: true}).click();
  await expect(dialog).toBeHidden();
  const selections = await page.evaluate(() => (window as any).__nfoCalls.find((c:any) => c.command === "sync_source_nfos").args.selections);
  expect(selections).toEqual([{path:"D:\\Incoming\\sample-2.mkv",fingerprint:"stamp-2"}]);
});

test("failed single NFO sync keeps its file and error visible", async ({ page }) => {
  await fixture(page, "partial");
  await page.getByRole("combobox", {name: "NFO 文件", exact: true}).click();
  await page.getByRole("option", {name: "D:\\Incoming\\sample-2.nfo", exact: true}).click();
  await page.getByRole("button", {name: "NFO 快速同步 · 2 项可同步", exact: true}).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", {name: "同步所选 1 项", exact: true}).click();
  await expect(dialog.getByRole("alert")).toContainText("媒体或 NFO 已变化");
  await expect(dialog).toBeVisible();
});

test("NFO inspection can retry and leaves matching usable while loading", async ({ page }) => {
  await fixture(page, "error");
  await expect(page.getByRole("alert")).toContainText("共享暂时不可用");
  await page.getByRole("button", {name:"重新检测",exact:true}).click();
  await expect(page.getByRole("button", {name:"NFO 快速同步 · 2 项可同步",exact:true})).toBeEnabled();
});

test("NFO has no action without matching sidecars", async ({ page }) => {
  await fixture(page, "empty");
  await expect(page.getByRole("button", {name:/NFO 快速同步/})).toHaveCount(0);
  await expect(page.getByRole("button", {name:"添加作品",exact:true})).toBeEnabled();
});
