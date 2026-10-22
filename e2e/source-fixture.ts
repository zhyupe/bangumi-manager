import { expect, type Page } from "@playwright/test";

export async function sourceFixture(
  page: Page,
  names = ["虚构远航 S02E01.mkv"],
  parent = "D:\\Incoming",
  episodesPerSeason = 1,
  extraPaths: Record<string, string> = {},
  settingsOverrides: Record<string, unknown> = {},
) {
  await page.addInitScript(
    ({ names, parent, episodesPerSeason, extraPaths, settingsOverrides }) => {
      (window as any).isTauri = true;
      const calls: any[] = [];
      (window as any).__sourceCalls = calls;
      const works = ["tv", "movie", "music"].map((kind) => ({
        id: kind,
        kind,
        title: `虚构作品-${kind}`,
        folder_name: `sample-${kind}`,
        year: "",
        tmdb_id: "",
        parent_id: "",
        overview: "",
        poster: "",
        bindings: [
          {
            id: kind,
            provider: kind === "music" ? "bangumi" : "tmdb",
            remote_id: "910000001",
            source_kind: kind,
            source_title: `虚构作品-${kind}`,
            season: 1,
          },
        ],
        seasons: [1, 2].map((n) => ({
          binding_id: kind,
          number: n,
          remote_number: n,
          title: `分卷 ${n}`,
          remote_title: "",
          custom_title: false,
        })),
        episodes: (kind === "movie" ? [1] : [1, 2]).flatMap((n) =>
          Array.from(
            { length: kind === "movie" ? 1 : episodesPerSeason },
            (_, index) => ({
              id: index === 0 ? `${kind}-${n}` : `${kind}-${n}-${index + 1}`,
              binding_id: kind,
              remote_id: `91000000${n}`,
              sort: 1,
              title: `虚构章节${n}`,
              info: {
                localized_title: `虚构章节${n}`,
                original_title: `Fictional Section ${n}`,
              },
              kind: 0,
              season: n,
              number: index + 1,
              enabled: true,
            }),
          ),
        ),
      }));
      const state = {
        settings: {
          tv_root: "D:\\Library\\TV",
          movie_root: "D:\\Library\\Movies",
          music_root: "D:\\Library\\Music",
          extras_root: "D:\\Extras",
          backup_root: "D:\\Backups",
          ffprobe: "ffprobe",
          tmdb_token: "",
          verify_hash: false,
          default_operation: "copy",
          ...settingsOverrides,
        },
        sources: names.map((name) => ({
          path: `${parent}\\${name}`,
          name,
          parent,
          extension: name.split(".").at(-1),
          media: !/\.(ass|ssa|srt|sub|idx|vtt|nfo|txt)$/i.test(name),
          hint: extraPaths[name] ? "extra" : "episode",
          extra_relative_path: extraPaths[name] ?? null,
          signature: { size: 100, modified: 100 },
          probe: null,
        })),
        works,
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
      let serial = 0;
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
          if (
            [
              "inspect_source_nfos",
              "list_resources",
              "list_backups",
              "search_media",
            ].includes(command)
          )
            return [];
          if (command === "plugin:event|listen") return args.handler;
          if (
            ["plugin:event|unlisten", "record_frontend_timing"].includes(
              command,
            )
          )
            return;
          throw new Error(`Unexpected command: ${command}`);
        },
      };
      (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener: () => {},
      };
    },
    { names, parent, episodesPerSeason, extraPaths, settingsOverrides },
  );
  await page.goto("/");
  await expect(
    page.getByRole("button", { name: /^(添加作品|Add title)$/ }),
  ).toBeEnabled();
}

export async function chooseWork(page: Page, kind: string) {
  await page
    .getByRole("combobox", { name: "整理到本地条目", exact: true })
    .click();
  await page.locator(`[role="option"][data-item-value="${kind}"]`).click();
}
