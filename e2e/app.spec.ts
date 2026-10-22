import packageInfo from "../package.json" with { type: "json" };
const { version } = packageInfo;
import { test, expect, type Page, type Locator } from "@playwright/test";

async function pick(page: Page, control: Locator, value: string) {
  await control.click();
  await page
    .locator(
      `[role="option"][data-value=${JSON.stringify(value)}], [role="option"][data-item-value=${JSON.stringify(value)}]`,
    )
    .click();
}

async function fixture(
  page: Page,
  fileCount = 2,
  subtitles = false,
  scenario:
    | "default"
    | "title-fallback"
    | "combobox"
    | "batch-all"
    | "catalog-layout"
    | "catalog-seasons"
    | "resources"
    | "resources-layout"
    | "tmdb-groups"
    | "tmdb-saved-group" = "default",
) {
  await page.addInitScript(
    ({ fileCount, subtitles, scenario }) => {
      (window as any).isTauri = true;
      const callbacks = new Map<number, (data: unknown) => void>();
      const listeners = new Map<string, number[]>();
      let serial = 0;
      const organizePreviews = new Map<string, any>();
      const episode = (n: number) => ({
        id: `e${n}`,
        binding_id: "b1",
        remote_id: `101${n}`,
        sort: n,
        title: `第 ${n} 話`,
        info: {
          localized_title: n === 1 ? "新旅程：我们在异世界相遇" : `第 ${n} 話`,
          original_title: n === 2 ? "" : `旅の始まり ${n}`,
          language: "zh-CN",
          original_language: "ja",
        },
        kind: 0,
        season: 2,
        number: n,
        enabled: true,
      });
      const work = {
        id: "w1",
        title: "FICTIONAL VOYAGE",
        kind: "tv",
        year: "2011",
        folder_name: "FICTIONAL VOYAGE",
        tmdb_id: "",
        parent_id: "",
        overview: "",
        poster: "",
        seasons: [
          {
            binding_id: "b1",
            number: 2,
            remote_number: 1,
            title: "FICTIONAL VOYAGE 第二季",
            remote_title: "FICTIONAL VOYAGE 第二季",
            custom_title: false,
          },
        ],
        bindings: [
          {
            id: "b1",
            provider: "bangumi",
            remote_id: "100",
            source_kind: "tv",
            source_title: "FICTIONAL VOYAGE’",
            season: 2,
          },
        ],
        episodes: [
          episode(1),
          episode(2),
          {
            ...episode(3),
            id: "sp",
            title: "OVA",
            kind: 1,
            season: 0,
            number: 0,
            enabled: false,
          },
        ],
      };
      const source = (n: number) => ({
        path: `D:\\Incoming\\[FICTIONAL_VOYAGE_2][0${n}].mkv`,
        name: `[FICTIONAL_VOYAGE_2][0${n}].mkv`,
        parent: "D:\\Incoming",
        extension: "mkv",
        signature: { size: 1024 ** 3, modified: 100 },
        media: true,
        hint: "episode",
        probe: {
          width: 1920,
          height: 1080,
          duration: 1440,
          bitrate: 12000000,
          video_codec: "h264",
          audio: ["flac / jpn"],
          error: null,
        },
      });
      const state = {
        settings: {
          tv_root: "D:\\Library\\TV Shows",
          movie_root: "D:\\Library\\Movies",
          extras_root: "D:\\Extras",
          backup_root: "D:\\Backups",
          ffprobe: "ffprobe",
          tmdb_token: "",
          verify_hash: false,
          default_operation: "copy",
        },
        works: [work],
        sources: Array.from({ length: fileCount }, (_, i) => source(i + 1)),
        index: {
          assets: [],
          unbound: [],
          errors: [],
          scanned_at: "2026-10-05T01:00:00Z",
          available: true,
        },
        plans: [] as any[],
        queue: { ids: [] as string[], paused: false, error: "" },
        data_dir: "C:\\Local\\BangumiManager",
      };
      if (scenario === "resources" || scenario === "resources-layout") {
        work.episodes = Array.from({ length: fileCount }, (_, i) =>
          episode(i + 1),
        );
        state.index.assets = Array.from({ length: fileCount }, (_, i) => ({
          path: `D:\\Library\\Season 02\\S02E${String(i + 1).padStart(2, "0")}.ts`,
          work_id: "w1",
          season: 2,
          number: i + 1,
          end: i + 1,
          signature: { size: (i + 1) * 1024 ** 3, modified: 1 },
        })) as any;
      }
      const resourceBackups: any[] =
        scenario === "resources" || scenario === "resources-layout"
          ? [
              {
                id: "backup1",
                plan_id: "old-task",
                created_at: "2026-10-05T01:00:00Z",
                root: "D:\\Backups",
                entry: {
                  original: "D:\\Library\\old.ts",
                  backup: "D:\\Backups\\old-task\\old.ts",
                  signature: { size: 123, modified: 1 },
                },
                signature: { size: 123, modified: 1 },
                complete: true,
                state: "available",
                error: "",
              },
            ]
          : [];
      if (scenario === "resources-layout") {
        work.title =
          "虚构资源页面布局测试：一个需要完整保留且足够长的双语作品标题 Fictional Archive";
        for (const kind of ["movie", "music"]) {
          state.works.push({
            ...work,
            id: kind,
            kind,
            title: `虚构${kind}作品`,
          });
        }
        state.index.assets = state.index.assets.map(
          (asset: any, i: number) => ({
            ...asset,
            path: `D:\\Library\\${"虚构目录\\".repeat(8)}${"Fictional.Long.File.Name.".repeat(5)}${i + 1}.${i === 2 ? "flac" : "mkv"}`,
            work_id: i === 1 ? "movie" : i === 2 ? "music" : "w1",
          }),
        ) as any;
        resourceBackups[0].entry.original = `D:\\Library\\${"虚构原文件目录\\".repeat(12)}archive.ts`;
        resourceBackups[0].entry.backup = `D:\\Backups\\${"虚构备份目录\\".repeat(12)}archive.ts`;
        resourceBackups[0].plan_id = "12345678-1234-4321-8765-123456789abc";
      }
      const resourcePreviews = new Map<string, any>();
      if (scenario === "catalog-layout") {
        const titles = [
          "短标题",
          "这是一个较长的剧集名称，需要折行展示第二行",
          "英文标题 A Very Long Television Series Name with Several Words",
          "普通作品",
          "虚构森林日记",
          "剧场版及特别篇合集名称",
          "另外一部作品",
          "长标题对应的多语言作品名称，完整保留而不截断",
        ];
        for (const [i, title] of titles.entries()) {
          state.works.push(
            Object.assign(
              {
                ...work,
                id: `grid-${i}`,
                title,
                year: String(2010 + i),
                category: "日番",
                poster:
                  i % 2
                    ? "data:image/svg+xml," +
                      encodeURIComponent(
                        `<svg xmlns="http://www.w3.org/2000/svg" width="200" height="300"><rect width="200" height="300" fill="#e8c9c4"/><text x="35" y="145" font-size="36" fill="#805b63">${2010 + i}</text></svg>`,
                      )
                    : "",
              },
              {
                info: {
                  localized_title:
                    i === 2
                      ? "这是一段较长的本地语言译名，需要保持对齐"
                      : title,
                  original_title: title,
                },
              },
            ),
          );
        }
        state.works.push({
          ...work,
          id: "movie",
          title: "电影示例",
          kind: "movie",
          seasons: [],
          episodes: [episode(1)],
        });
        state.works.push({
          ...work,
          id: "music",
          title: "音乐专辑",
          kind: "music",
        });
      }
      if (scenario === "catalog-seasons") {
        work.overview =
          "作品资料集中在信息页；各季度分别管理名称、远程来源和本地资源。";
        work.seasons.push({
          ...work.seasons[0],
          number: 1,
          remote_number: 1,
          binding_id: "b0",
          title: "第一季",
          remote_title: "第一季",
        });
        work.seasons.push({
          ...work.seasons[0],
          number: 3,
          remote_number: 3,
          binding_id: "tmdb",
          title: "尚未播出的第三季",
          remote_title: "Season 3",
        });
        work.seasons.push({
          ...work.seasons[0],
          number: 2,
          remote_number: 2,
          binding_id: "tmdb",
          title: "第二季补充资料",
          remote_title: "Season 2",
        });
        work.bindings.push({
          ...work.bindings[0],
          id: "b0",
          remote_id: "99",
          season: 1,
        });
        work.bindings.push({
          ...work.bindings[0],
          id: "tmdb",
          provider: "tmdb",
          remote_id: "202",
          season: 1,
        });
        work.episodes.push(
          episode(3),
          { ...episode(1), id: "duplicate", binding_id: "tmdb" },
          { ...episode(1), id: "s1e1", season: 1, binding_id: "b0" },
          { ...episode(1), id: "s4e1", season: 4 },
        );
        state.index.assets = [
          {
            path: "D:\\Library\\S02E01-E02.mkv",
            work_id: "w1",
            season: 2,
            number: 1,
            end: 2,
            signature: { size: 123, modified: 1 },
          },
          {
            path: "D:\\Library\\S02E01.mp4",
            work_id: "w1",
            season: 2,
            number: 1,
            end: 1,
            signature: { size: 123, modified: 1 },
          },
        ] as any;
      }
      if (scenario === "batch-all") {
        work.episodes = Array.from({ length: fileCount }, (_, i) =>
          episode(i + 1),
        );
      }
      if (scenario === "combobox") {
        Object.assign(work, {
          info: {
            localized_title: "虚构远航",
            original_title: "FICTIONAL VOYAGE",
          },
        });
        work.episodes = [
          ...Array.from({ length: 40 }, (_, i) => episode(i + 1)),
          work.episodes[2],
        ];
        state.works.push(
          Object.assign(
            { ...work, id: "w2", year: "2015" },
            {
              info: {
                localized_title: "虚构远航 三",
                original_title: "架空の航海",
              },
            },
          ),
        );
      }
      if (scenario === "title-fallback") {
        work.title = "Fictional Mystery";
        Object.assign(work.seasons[0], {
          number: 5,
          remote_number: 5,
          title: "Fictional Mystery 第五季",
          remote_title: "Fictional Mystery 第五季",
        });
        Object.assign(work.bindings[0], {
          season: 5,
          source_title: "Fictional Mystery",
        });
        Object.assign(work.episodes[0], {
          season: 5,
          title: "虚构线索（虚构航班182号班机）",
          info: {
            localized_title: "虚构线索（虚构航班182号班机）",
            original_title: "Hidden Signal (Fictional Flight 182)",
            language: "zh-CN",
            original_language: "en",
          },
        });
        Object.assign(work.episodes[1], { season: 5, number: 7 });
        const name =
          "Fictional.Mystery.S05E07.Hidden.Signal.虚构航班182号.1985.mkv";
        Object.assign(state.sources[0], { name, path: `D:\Incoming\${name}` });
      }
      if (subtitles) {
        state.sources.push(
          ...([1, 2].map((n) => ({
            ...source(n),
            media: false,
            extension: "ass",
            probe: null,
            name: source(n).name.replace(".mkv", ".zh-CN.ass"),
            path: source(n).path.replace(".mkv", ".zh-CN.ass"),
          })) as any),
        );
        state.sources.push({
          ...source(3),
          media: false,
          extension: "srt",
          probe: null,
          name: "unmatched.srt",
          path: "D:\\Incoming\\unmatched.srt",
        } as any);
      }
      (window as any).__emitPlan = (plan: any) => {
        for (const h of listeners.get("job-progress") || [])
          callbacks.get(h)?.({
            event: "job-progress",
            payload: JSON.parse(JSON.stringify(plan)),
          });
      };
      const emitJob = (id: string, status: string, event = "job-progress") => {
        const plan = state.plans.find((p: any) => p.id === id);
        plan.state = status;
        for (const h of listeners.get(event) || [])
          callbacks.get(h)?.({
            event,
            payload: JSON.parse(JSON.stringify(plan)),
          });
      };
      const emitQueue = () => {
        for (const h of listeners.get("queue-changed") || [])
          callbacks.get(h)?.({
            event: "queue-changed",
            payload: JSON.parse(JSON.stringify(state.queue)),
          });
      };
      const startNext = () => {
        if (
          state.queue.paused ||
          state.plans.some((p: any) =>
            ["running", "cancelling"].includes(p.state),
          )
        )
          return;
        const id = state.queue.ids.shift();
        if (id) emitJob(id, "running");
        emitQueue();
      };
      (window as any).__finishNext = (failed = false) => {
        const plan = state.plans.find((p: any) => p.state === "running");
        plan.rows.forEach((r: any, i: number) => {
          r.state = failed ? (i === 0 ? "failed" : "pending") : "complete";
        });
        if (failed) {
          plan.rows[0].error = "模拟共享连接失败";
          state.queue.paused = true;
          state.queue.error = plan.rows[0].error;
        }
        emitJob(plan.id, failed ? "failed" : "complete", "job-finished");
        emitQueue();
        startNext();
      };
      (window as any).__jobEvent = emitJob;
      (window as any).__plans = state.plans;
      if (scenario === "tmdb-groups" || scenario === "tmdb-saved-group") {
        work.bindings[0].provider = "tmdb";
        work.bindings[0].remote_id = "910000030";
        if (scenario === "tmdb-saved-group")
          (work.bindings[0] as any).episode_group = {
            id: "abcdef0123456789abcdef01",
            name: "虚构群岛纪事",
            description: "首播日期分组",
            type: 1,
            group_count: 19,
            episode_count: 114,
          };
      }
      (window as any).__calls = [];
      const invoke = async (command: string, args: any = {}) => {
        (window as any).__calls.push({ command, args });
        if (command === "open_in_file_manager") {
          if ((window as any).__fileManagerMode === "pending")
            await new Promise<void>((resolve) => {
              (window as any).__finishFileManager = resolve;
            });
          if ((window as any).__fileManagerMode === "error")
            throw "无法访问路径：网络共享离线";
          return (window as any).__fileManagerMode === "parent"
            ? { path: "D:\\Library", fallback: true }
            : { path: args.path, fallback: false };
        }
        if (command === "inspect_source_nfos") return [];
        if (command === "plugin:event|listen") {
          const handlers = listeners.get(args.event) || [];
          handlers.push(args.handler);
          listeners.set(args.event, handlers);
          return args.handler;
        }
        if (command === "plugin:event|unlisten") {
          listeners.set(
            args.event,
            (listeners.get(args.event) || []).filter(
              (id) => id !== args.eventId,
            ),
          );
          return;
        }
        if (
          command === "record_frontend_timing" ||
          command === "open_log_directory"
        )
          return;
        if (command === "diagnostics_report")
          return {
            directory: "D:\\AppData\\logs",
            session_id: "test-session",
            write_error: null,
            active: [],
            statistics: [
              {
                operation: "http.headers",
                count: 2,
                failures: 0,
                total_ms: 2300,
                max_ms: 1400,
                last_ms: 900,
              },
            ],
            recent: [
              {
                timestamp: "2026-10-05",
                trace_id: "trace-test",
                operation: "http.headers",
                status: "ok",
                elapsed_ms: 900,
                fields: { host: "api.themoviedb.org", http_status: 200 },
              },
            ],
          };
        if (command === "list_episode_groups")
          return [
            {
              id: "abcdef0123456789abcdef01",
              name: "虚构群岛纪事",
              description: "首播日期分组",
              type: 1,
              group_count: 19,
              episode_count: 114,
            },
          ];
        if (
          command === "media_detail" &&
          (scenario === "tmdb-groups" || scenario === "tmdb-saved-group")
        )
          return {
            provider: "tmdb",
            id: "910000030",
            kind: "tv",
            title: "虚构群岛纪事",
            original_title: "虚构群岛纪事",
            year: "2009",
            overview: "",
            poster: "",
            cached: false,
            relations: [],
            episode_group: args.episodeGroupId
              ? {
                  id: args.episodeGroupId,
                  name: "虚构群岛纪事",
                  description: "首播日期分组",
                  type: 1,
                  group_count: 19,
                  episode_count: 114,
                }
              : null,
            seasons: [
              {
                number: args.episodeGroupId ? 3 : 2,
                title: "虚构群岛：序章",
                remote_number: 3,
              },
            ],
            episodes: [
              {
                id: "1011",
                sort: 1,
                title: "第 1 話",
                kind: 0,
                season: args.episodeGroupId ? 3 : 2,
              },
            ],
          };
        if (command === "media_detail" && String(args.id).includes("910000020"))
          return {
            provider: "tmdb",
            id: "910000020",
            kind: "movie",
            title: "虚构河岸之行",
            original_title: "虚构河岸之行",
            year: "2022",
            overview: "",
            poster: "",
            episodes: [],
            relations: [],
            cached: false,
          };
        if (command === "list_work_extras") return {path: "D:\\Extras\\sample", files: (window as any).__storedExtras || [], errors: []};
        if (command === "snapshot") return JSON.parse(JSON.stringify(state));
        if (command === "list_resources") {
          if ((window as any).__resourceListError)
            throw new Error("资源列表读取失败，请重试");
          return state.index.assets.map((a: any, i: number) => ({
            path: a.path,
            name: a.path.split("\\").pop(),
            extension: a.path.split(".").pop(),
            kind: state.works.find((w) => w.id === a.work_id)?.kind || "tv",
            signature: a.signature,
            asset: a,
            available: true,
            probe: i % 3 === 2 ? null : source(1).probe,
          }));
        }
        if (command === "list_backups")
          return JSON.parse(JSON.stringify(resourceBackups));
        if (command === "cleanup_backups") {
          for (const b of resourceBackups)
            if (args.ids.includes(b.id)) b.state = "removed";
          return JSON.parse(JSON.stringify(resourceBackups));
        }
        if (command === "probe_resources") {
          for (const [i, path] of args.paths.entries()) {
            for (const h of listeners.get("resource-probe-progress") || [])
              callbacks.get(h)?.({
                event: "resource-probe-progress",
                payload: {
                  done: i + 1,
                  total: args.paths.length,
                  path,
                  probe: source(1).probe,
                },
              });
            if ((window as any).__holdResourceProbe) {
              await new Promise<void>((resolve) => {
                (window as any).__releaseResourceProbe = resolve;
              });
              break;
            }
          }
          return;
        }
        if (command === "cancel_resource_probe") {
          (window as any).__holdResourceProbe = false;
          (window as any).__releaseResourceProbe?.();
          return;
        }
        if (command === "scan_replacements")
          return {
            candidates: args.paths.map((path: string) => ({
              old_path: path,
              new_path: path.replace(/\.[^.]+$/, ".mkv"),
              reason: "同目录主文件名一致",
            })),
            errors: [],
          };
        if (command === "preview_replacements") {
          const p = {
            id: "replacement1",
            kind: "replacement",
            created_at: "2026-10-05T01:00:00Z",
            state: "draft",
            works: [work],
            rows: args.requests.map((r: any, i: number) => ({
              id: `replacement-row${i}`,
              source: r.new_path,
              target: (window as any).__duplicatePathSpelling
                ? r.new_path.replace(/\\/g, "/").toLowerCase()
                : r.old_path.replace(/\.[^.]+$/, ".mkv"),
              work_id: "w1",
              episode_ids: ["e1"],
              extra: false,
              attachment: false,
              parent_id: null,
              signature: { size: 512 * 1024 ** 2, modified: 1 },
              target_signature: null,
              probe: source(1).probe,
              target_probe: source(1).probe,
              conflicts: [],
              warnings: ["新文件的音轨或字幕轨道减少"],
              reviewed: false,
              action: "replace",
              operation: "adopt",
              state: "pending",
              error: "",
              stage: "",
              digest: "",
              backups: (window as any).__duplicatePathSpelling
                ? [
                    {
                      original: r.old_path,
                      backup: `D:\\Backups\\replacement1\\row${i}\\old.ts`,
                      signature: { size: 100, modified: 1 },
                      probe: null,
                    },
                  ]
                : [],
              replacement: {
                old_asset: state.index.assets.find(
                  (a: any) => a.path === r.old_path,
                ),
                root: "D:\\Library",
                adopt: true,
              },
            })),
            tv_root: state.settings.tv_root,
            movie_root: state.settings.movie_root,
            extras_root: state.settings.extras_root,
            backup_root: state.settings.backup_root,
            verify_hash: false,
            metadata_errors: [],
          };
          resourcePreviews.set(p.id, p);
          return JSON.parse(JSON.stringify(p));
        }
        if (command === "preview_renames") {
          if ((window as any).__renameError)
            throw "共享目录离线，请连接后重新预览";
          const p = {
            id: "rename1",
            kind: "rename",
            created_at: "2026-10-05T01:00:00Z",
            state: "draft",
            works: [work],
            metadata_errors: [],
            metadata_notices: [],
            tv_root: state.settings.tv_root,
            movie_root: state.settings.movie_root,
            extras_root: state.settings.extras_root,
            backup_root: state.settings.backup_root,
            verify_hash: false,
            rows: (window as any).__renameNoop
              ? []
              : args.paths.map((path: string, i: number) => ({
                  id: "rename-row" + i,
                  source: path,
                  target: "D:\\Library\\Season 03\\S03E0" + (i + 1) + ".ts",
                  work_id: "w1",
                  episode_ids: ["e" + (i + 1)],
                  extra: false,
                  attachment: false,
                  parent_id: null,
                  signature: source(1).signature,
                  target_signature: null,
                  probe: null,
                  target_probe: null,
                  conflicts: (window as any).__renameConflict
                    ? ["目标已被未选中的文件占用，不能覆盖"]
                    : [],
                  warnings: [],
                  reviewed: true,
                  action: "copy",
                  operation: "rename",
                  state: "pending",
                  error: "",
                  stage: "",
                  digest: "",
                  backups: [
                    {
                      original: path.replace(".ts", ".nfo"),
                      backup: "D:\\Backups\\rename1\\original" + i + ".nfo",
                      signature: { size: 100, modified: 1 },
                      preserve_original: true,
                    },
                  ],
                  rename: {
                    root: "D:\\Library",
                    asset: state.index.assets.find((a: any) => a.path === path),
                    phase: "pending",
                  },
                })),
          };
          if (p.rows.length) {
            const row = p.rows[0];
            p.rows.push({
              ...row,
              id: row.id + "-sub",
              source: row.source.replace(".ts", ".zh.srt"),
              target: row.target.replace(".ts", ".zh.srt"),
              attachment: true,
              parent_id: row.id,
              backups: [],
            });
          }
          resourcePreviews.set(p.id, p);
          return JSON.parse(JSON.stringify(p));
        }
        if (command === "create_rename_plan") {
          const p = resourcePreviews.get(args.id);
          if ((window as any).__renameCreateError)
            throw "文件在预览后发生变化，请重新预览";
          state.plans.push(p);
          return JSON.parse(JSON.stringify(p));
        }
        if (command === "create_replacement_plan") {
          const p = resourcePreviews.get(args.id);
          p.rows.forEach((r: any) => (r.reviewed = true));
          state.plans.push(p);
          return JSON.parse(JSON.stringify(p));
        }
        if (command === "save_settings") {
          state.settings = args.settings;
          return;
        }
        if (command === "save_work") {
          state.works = [args.work];
          return;
        }
        if (command === "preview_naming")
          return {
            tv: "电视剧/日番/FICTIONAL VOYAGE (2011)/Season 02/FICTIONAL VOYAGE - S02E01.mkv",
            movie: "电影/动画电影/作品/作品.mkv",
            music: "音乐/专辑/作品/Disc 01/01 - 音轨.flac",
          };
        if (command === "refresh_library") {
          if ((window as any).__importNfo) {
            state.works.push({
              ...work,
              id: "nfo-movie",
              title: "虚构星海旅人！",
              kind: "movie",
              year: "2026",
              folder_name: "虚构星海旅人！ (2026)",
            });
            state.index = {
              ...state.index,
              imported: 1,
              updated: 0,
              unbound: [
                {
                  path: "D:\\Library\\TV Shows\\Conflict",
                  name: "Conflict",
                  kind: "tv",
                  files: 1,
                  reason: "NFO ID 同时指向多个本地作品",
                },
              ],
              errors: ["Conflict：NFO ID 同时指向多个本地作品"],
            } as any;
            (window as any).__importNfo = false;
          }
          return state.index;
        }
        if (command === "sync_work") return state.works[0];
        if (command === "search_media")
          return [
            {
              provider: "bangumi",
              id: "100",
              kind: "tv",
              title: "FICTIONAL VOYAGE 第二季",
              original_title: "FICTIONAL VOYAGE’",
              year: "2012",
              overview: "第二季",
              poster: "",
              episodes: [],
              relations: [],
              cached: false,
            },
          ];
        if (command === "media_detail")
          return {
            provider: args.provider || "bangumi",
            id: "100",
            kind: "tv",
            title: "FICTIONAL VOYAGE 第二季",
            original_title: "FICTIONAL VOYAGE’",
            year: "2012",
            overview: "第二季",
            poster: "",
            episodes: [
              { id: "1011", sort: 1, title: "第 1 話", kind: 0, season: 1 },
            ],
            relations: [],
            cached: false,
          };
        if (command === "bind_subject") return work;
        if (command === "scan_files")
          return (
            (window as any).__extraScan || {
              files: args.paths.map((path: string) => ({ path })),
              errors: [],
            }
          );
        if (command === "discard_organize_preview") {
          organizePreviews.delete(args.id);
          return;
        }
        if (command === "submit_organize") {
          if ((window as any).__failSubmit)
            throw new Error(
              "文件、关联或整理规则在预览后变化，请重新生成预览并核对",
            );
          const plan = organizePreviews.get(args.id);
          if (!plan) throw new Error("预览不存在，请重新生成");
          for (const row of plan.rows) {
            const choice = args.choices.find(
              (c: any) => c.id === (row.parent_id || row.id),
            );
            if (choice) Object.assign(row, { ...choice, id: row.id });
          }
          state.plans.push(plan);
          organizePreviews.delete(args.id);
          return plan;
        }
        if (command === "create_plan" || command === "preview_organize") {
          if ((window as any).__failPlan)
            throw new Error("无法读取源文件，请重试");
          const plan = {
            id:
              command === "preview_organize"
                ? `preview-${++serial}`
                : `p${state.plans.length + 1}`,
            created_at: "2026-10-05T01:00:00Z",
            state: "draft",
            rows: args.mappings.map((m: any, i: number) => ({
              id: `r${i}`,
              source: m.source,
              target: m.extra
                ? `${state.settings.extras_root}\\FICTIONAL VOYAGE\\${m.source
                    .split(/\\(?:bonus|extras?)\\/i)
                    .at(-1)
                    .replace(/^.*\\Incoming\\/i, "")}`
                : `D:\\Library\\TV Shows\\FICTIONAL VOYAGE\\Season 02\\S02E${String(command === "preview_organize" ? work.episodes.find((ep: any) => ep.id === m.episode_ids[0])?.number || i + 1 : i + 1).padStart(2, "0")}${m.part ? ` - part${m.part}` : ""}.mkv`,
              work_id: m.work_id,
              part: m.part || 0,
              episode_ids: m.episode_ids,
              extra: m.extra,
              attachment: false,
              parent_id: null,
              signature: source(1).signature,
              target_signature: null,
              probe: source(1).probe,
              target_probe: null,
              conflicts: [],
              warnings: [],
              reviewed: false,
              action: "copy",
              operation: state.settings.default_operation || "copy",
              state: "pending",
              error: "",
              stage: "",
              digest: "",
              backups: [],
            })),
            tv_root: state.settings.tv_root,
            movie_root: state.settings.movie_root,
            extras_root: state.settings.extras_root,
            backup_root: state.settings.backup_root,
            verify_hash: false,
            works: state.works,
            metadata_errors: [],
          };
          if (subtitles)
            for (const row of [...plan.rows]) {
              const subtitle = state.sources.find(
                (s) => s.path === row.source.replace(".mkv", ".zh-CN.ass"),
              );
              if (subtitle)
                plan.rows.push({
                  ...row,
                  id: `${row.id}-sub`,
                  source: subtitle.path,
                  target: row.target.replace(".mkv", ".zh-CN.ass"),
                  attachment: true,
                  parent_id: row.id,
                });
            }
          if (command === "preview_organize") {
            if ((window as any).__organizeConflict)
              Object.assign(plan.rows[0], {
                action: "undecided",
                conflicts: ["目标文件已存在"],
                target_signature: { size: 40, modified: 1 },
              });
            organizePreviews.set(plan.id, plan);
            if ((window as any).__holdOrganizePreview)
              await new Promise<void>((resolve) => {
                (window as any).__finishOrganizePreview = resolve;
              });
          } else state.plans.push(plan);
          return plan;
        }
        if (command === "save_decisions") {
          const plan = state.plans.find((p: any) => p.id === args.id);
          plan.rows = plan.rows.map((r: any) => {
            const choice = args.choices.find(
              (c: any) => c.id === (r.parent_id || r.id),
            );
            return choice
              ? {
                  ...r,
                  action: choice.action,
                  operation: choice.operation,
                  reviewed: choice.reviewed,
                }
              : r;
          });
          return plan;
        }
        if (command === "cancel_plan") {
          if ((window as any).__failCancel)
            throw new Error("任务取消失败，请重试");
          const plan = state.plans.find((p: any) => p.id === args.id);
          if (!["complete", "cancelled"].includes(plan.state)) {
            plan.state = plan.state === "running" ? "cancelling" : "cancelled";
            if (plan.state === "cancelled")
              plan.rows.forEach((r: any) => {
                if (!["complete", "skipped"].includes(r.state))
                  r.state = "cancelled";
              });
          }
          state.queue.ids = state.queue.ids.filter(
            (id: string) => id !== args.id,
          );
          emitQueue();
          return JSON.parse(JSON.stringify(plan));
        }
        if (command === "delete_plan" || command === "delete_plans") {
          if ((window as any).__failDelete)
            throw new Error("任务删除失败，请重试");
          const ids: string[] =
            command === "delete_plan" ? [args.id] : args.ids;
          if (
            ids.some(
              (id) =>
                !state.plans.some(
                  (p: any) =>
                    p.id === id && ["cancelled", "complete"].includes(p.state),
                ),
            )
          )
            throw new Error("只能删除已完成或已取消的任务");
          for (let i = state.plans.length - 1; i >= 0; i--)
            if (ids.includes(state.plans[i].id)) state.plans.splice(i, 1);
          return;
        }
        if (command === "cancel_job") {
          state.queue.paused = true;
          emitQueue();
          return;
        }
        if (command === "resume_queue") {
          state.queue.paused = false;
          state.queue.error = "";
          startNext();
          return;
        }
        if (command === "run_plan") {
          if ((window as any).__failEnqueue)
            throw new Error("还有未决冲突，请选择保留、替换或跳过");
          const plan = state.plans.find((p: any) => p.id === args.id);
          plan.state = "queued";
          const result = JSON.parse(JSON.stringify(plan));
          state.queue.ids.push(plan.id);
          (window as any).__emitPlan(plan);
          startNext();
          return result;
        }
        if (command === "plugin:dialog|open")
          return "D:\\Incoming\\[FICTIONAL_VOYAGE_2][01].mkv";
        throw new Error(`Unexpected command: ${command}`);
      };
      (window as any).__TAURI_INTERNALS__ = {
        invoke,
        convertFileSrc: () => "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6OAAAAABJRU5ErkJggg==",
        metadata: {
          currentWindow: { label: "main" },
          currentWebview: { label: "main" },
        },
        transformCallback: (cb: (data: unknown) => void) => {
          const id = ++serial;
          callbacks.set(id, cb);
          return id;
        },
        unregisterCallback: (id: number) => callbacks.delete(id),
      };
      (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener: () => {},
      };
      (window as any).__drop = (paths: string[], x: number, y: number) => {
        for (const h of listeners.get("tauri://drag-drop") || [])
          callbacks.get(h)?.({
            event: "tauri://drag-drop",
            payload: { paths, position: { x, y } },
          });
      };
    },
    { fileCount, subtitles, scenario },
  );
  await page.goto("/");
  await expect(
    page.getByText(
      `${fileCount + (subtitles ? 1 : 0)} 项 · 第 1 / ${Math.ceil((fileCount + (subtitles ? 1 : 0)) / 80)} 页`,
    ),
  ).toBeVisible();
}
test("file manager buttons preserve selection, prevent repeat requests and support retry", async ({
  page,
}) => {
  await fixture(page);
  const source = "D:\\Incoming\\[FICTIONAL_VOYAGE_2][01].mkv";
  const row = page.locator(".file-table tbody tr").first();
  const open = row.getByRole("button", {
    name: `在文件管理器中打开：${source}`,
    exact: true,
  });
  await expect(row.locator(".open-path-button")).toHaveCount(1);
  await page.evaluate(() => {
    (window as any).__fileManagerMode = "pending";
  });
  await open.focus();
  await page.keyboard.press("Enter");
  await expect(open).toBeDisabled();
  await expect(row.getByRole("checkbox")).not.toBeChecked();
  await expect(
    row.getByRole("button", {
      name: "在文件管理器中打开：D:\\Incoming",
      exact: true,
    }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "资源", exact: true }),
  ).toBeEnabled();
  expect(
    await page.evaluate(() =>
      (window as any).__calls
        .filter((c: any) => c.command === "open_in_file_manager")
        .map((c: any) => c.args.path),
    ),
  ).toEqual([source]);
  await page.evaluate(() => {
    (window as any).__fileManagerMode = "error";
    (window as any).__finishFileManager();
  });
  await expect(page.getByRole("alert")).toContainText("网络共享离线");
  await expect(open).toBeEnabled();
  await page.getByRole("button", { name: "关闭错误消息" }).click();
  await page.evaluate(() => {
    (window as any).__fileManagerMode = "parent";
  });
  await open.click();
  await expect(page.locator(".notification-viewport")).toContainText(
    "路径尚不存在，已打开父目录：D:\\Library",
  );
  await expect(row.getByRole("checkbox")).not.toBeChecked();
  expect(
    await page.evaluate(
      () =>
        (window as any).__calls.filter(
          (c: any) => c.command === "open_in_file_manager",
        ).length,
    ),
  ).toBe(2);
  await row
    .getByRole("button", {
      name: "[FICTIONAL_VOYAGE_2][01].mkv 文件信息",
      exact: true,
    })
    .click();
  await expect(page.locator(".file-info-popover")).toContainText(source);
  await expect(
    page.locator(".file-info-popover .open-path-button"),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", {
      name: `在文件管理器中打开：${source}`,
      exact: true,
    }),
  ).toHaveCount(1);
});

test("file manager actions cover resources, details and exact backup files without selecting them", async ({
  page,
}) => {
  await fixture(page, 2, false, "resources");
  await page.getByRole("button", { name: "资源", exact: true }).click();
  const row = page.locator(".resource-table tbody tr").first();
  const open = row.locator(".open-path-button");
  const label = (await open.getAttribute("aria-label"))!;
  await open.click();
  await expect(row.getByRole("checkbox")).not.toBeChecked();
  await row.locator(".resource-name .resource-title-link").click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: label, exact: true })
    .click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "关闭", exact: true })
    .click();
  await page.getByRole("tab", { name: "旧版本备份", exact: true }).click();
  const backupPath = "D:\\Backups\\old-task\\old.ts";
  await page
    .getByRole("button", {
      name: `在文件管理器中打开：${backupPath}`,
      exact: true,
    })
    .click();
  await expect(
    page.locator(".backup-table tbody tr").first().getByRole("checkbox"),
  ).not.toBeChecked();
  await page
    .getByRole("button", { name: "全选可清理备份", exact: true })
    .click();
  await page.getByRole("button", { name: "清理选中备份", exact: true }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", {
      name: `在文件管理器中打开：${backupPath}`,
      exact: true,
    })
    .click();
  const calls = await page.evaluate(() => (window as any).__calls);
  expect(
    calls
      .filter((c: any) => c.command === "open_in_file_manager")
      .map((c: any) => c.args.path),
  ).toEqual([
    label.replace("在文件管理器中打开：", ""),
    label.replace("在文件管理器中打开：", ""),
    backupPath,
    backupPath,
  ]);
  expect(
    calls.filter((c: any) => c.command === "cleanup_backups"),
  ).toHaveLength(0);
});

test("file manager opens edited storage paths without saving settings or launching executable files", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  const shared = "\\\\fileserver\\media\\虚构目录 [01]";
  await page
    .getByRole("textbox", { name: "整体输出目录", exact: true })
    .fill(shared);
  await page
    .getByRole("button", { name: `在文件管理器中打开：${shared}`, exact: true })
    .click();
  await page.getByRole("button", {name: "分别指定可执行文件", exact: true}).click();
  await expect(
    page.getByRole("button", {
      name: "在文件管理器中打开：ffprobe",
      exact: true,
    }),
  ).toBeDisabled();
  const executable = "D:\\Tools\\ffprobe.exe";
  await page
    .getByRole("textbox", { name: "FFprobe 可执行文件", exact: true })
    .fill(executable);
  await page
    .getByRole("button", {
      name: `在文件管理器中打开：${executable}`,
      exact: true,
    })
    .click();
  await page
    .getByRole("button", {
      name: "在文件管理器中打开：C:\\Local\\BangumiManager",
      exact: true,
    })
    .click();
  const calls = await page.evaluate(() => (window as any).__calls);
  expect(
    calls
      .filter((c: any) => c.command === "open_in_file_manager")
      .map((c: any) => c.args.path),
  ).toEqual([shared, executable, "C:\\Local\\BangumiManager"]);
  expect(calls.filter((c: any) => c.command === "save_settings")).toHaveLength(
    0,
  );
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
});

test("resource layouts contain long names and paths at desktop widths with accessible sorting", async ({
  page,
}) => {
  await fixture(page, 6, false, "resources-layout");
  await page.getByRole("button", { name: "资源", exact: true }).click();
  const panel = page.getByRole("tabpanel", { name: "媒体文件", exact: true });
  const table = page.getByRole("region", { name: "媒体文件列表", exact: true });
  await expect(panel).toBeVisible();
  await expect(panel.getByText("正片", { exact: true })).toBeVisible();
  await expect(
    panel.getByText("第 2 碟 · 曲目 03", { exact: true }),
  ).toBeVisible();
  await expect(
    panel.getByRole("columnheader", { name: "大小", exact: true }),
  ).toHaveAttribute("aria-sort", "descending");
  await panel.getByRole("button", { name: "大小", exact: true }).click();
  await expect(
    panel.getByRole("columnheader", { name: "大小", exact: true }),
  ).toHaveAttribute("aria-sort", "ascending");
  await panel
    .getByRole("button", { name: "全选筛选结果", exact: true })
    .click();
  await expect(table.locator('tbody tr[data-state="selected"]')).toHaveCount(6);
  for (const width of [1440, 1050]) {
    await page.setViewportSize({ width, height: 800 });
    const dimensions = await table.evaluate((element) => ({
      document: document.documentElement.scrollWidth,
      viewport: window.innerWidth,
      content: element.scrollWidth,
      available: element.clientWidth,
    }));
    expect(dimensions.document).toBeLessThanOrEqual(dimensions.viewport + 1);
    if (width === 1050)
      expect(dimensions.content).toBeGreaterThan(dimensions.available);
    await page.screenshot({
      path: `test-results/resources-layout-${width}.png`,
      fullPage: true,
    });
  }
  await table
    .locator('[data-slot="button"][data-variant="link"]')
    .first()
    .focus();
  const linkColor = await table
    .locator('.resource-name [data-slot="button"]')
    .first()
    .evaluate((element) => getComputedStyle(element).color);
  expect(linkColor).toBe("rgb(157, 64, 85)");
  await page.getByRole("tab", { name: "旧版本备份", exact: true }).click();
  await expect(
    page.getByRole("tabpanel", { name: "旧版本备份", exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/backups-layout-1050.png",
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "全选可清理备份", exact: true })
    .click();
  await page.getByRole("button", { name: "清理选中备份", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(
    dialog.getByRole("button", { name: "确认永久删除", exact: true }),
  ).toBeInViewport();
  await expect(dialog.locator(".backup-cleanup-list")).toContainText(
    "archive.ts",
  );
});

test("resource refresh errors stay visible and can be retried", async ({
  page,
}) => {
  await fixture(page, 2, false, "resources");
  await page.evaluate(() => {
    (window as any).__resourceListError = true;
  });
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("资源列表读取失败");
  await expect(page.getByText("尚无资源索引", { exact: true })).toHaveCount(0);
  await page.evaluate(() => {
    (window as any).__resourceListError = false;
  });
  await page.getByRole("button", { name: "刷新列表", exact: true }).click();
  await expect(page.locator(".resource-table tbody tr")).toHaveCount(2);
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("replacement review keeps actions visible in a short window", async ({
  page,
}) => {
  await fixture(page, 6, false, "resources");
  await page.setViewportSize({ width: 1050, height: 700 });
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await page.getByRole("button", { name: "全选筛选结果", exact: true }).click();
  await page.getByRole("button", { name: "扫描替代文件", exact: true }).click();
  const dialog = page.getByRole("dialog");
  for (let n = 1; n <= 6; n++) {
    const name = `S02E0${n}`;
    await pick(
      page,
      dialog.getByLabel(`替代 ${name}.ts`, { exact: true }),
      `D:\\Library\\Season 02\\${name}.mkv`,
    );
  }
  await dialog
    .getByRole("button", { name: "比较并预览（6）", exact: true })
    .click();
  await expect(
    dialog.getByRole("button", { name: "确认并创建任务", exact: true }),
  ).toBeInViewport();
  await expect(
    dialog.getByRole("button", { name: "返回选择", exact: true }),
  ).toBeInViewport();
  const body = dialog.locator(".replacement-body");
  expect(
    await body.evaluate(
      (element) => element.scrollHeight > element.clientHeight,
    ),
  ).toBe(true);
  await dialog
    .getByRole("button", { name: "音轨（1）", exact: true })
    .first()
    .click();
  await expect(
    body.getByText("1. flac / jpn", { exact: true }).first(),
  ).toBeVisible();
  await page.screenshot({
    path: "test-results/replacement-layout-1050.png",
    fullPage: true,
  });
});

test("resource filters select across pages and batch probing uses all selected files", async ({
  page,
}) => {
  await fixture(page, 90, false, "resources");
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await expect(page.getByText("90 个文件", { exact: false })).toBeVisible();
  await page.getByRole("button", { name: "全选筛选结果", exact: true }).click();
  await page.getByRole("button", { name: "探测选中", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__calls.find(
            (c: any) => c.command === "probe_resources",
          )?.args.paths.length,
      ),
    )
    .toBe(90);
  await page.getByLabel("搜索资源", { exact: true }).fill("S02E01");
  await expect(page.getByText("1 个文件 ·", { exact: false })).toBeVisible();
  await page.screenshot({ path: "test-results/resources.png", fullPage: true });
});

test("resource probing reports progress, survives navigation and can cancel and retry", async ({
  page,
}) => {
  await fixture(page, 3, false, "resources");
  await page.evaluate(() => {
    (window as any).__holdResourceProbe = true;
  });
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await page
    .getByRole("button", { name: "探测全部筛选结果", exact: true })
    .click();
  await expect(page.getByText("探测 1 / 3", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await page.getByRole("button", { name: "取消探测", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "探测全部筛选结果", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "探测全部筛选结果", exact: true })
    .click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__calls.filter(
            (c: any) => c.command === "probe_resources",
          ).length,
      ),
    )
    .toBe(2);
});

test("resource replacement previews differences and requires each warning before task creation", async ({
  page,
}) => {
  await fixture(page, 2, false, "resources");
  await page.evaluate(() => {
    (window as any).__duplicatePathSpelling = true;
  });
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await page.getByLabel("选择 S02E01.ts", { exact: true }).click();
  await page.getByRole("button", { name: "扫描替代文件", exact: true }).click();
  await pick(
    page,
    page.getByLabel("替代 S02E01.ts", { exact: true }),
    "D:\\Library\\Season 02\\S02E01.mkv",
  );
  await page.getByRole("button", { name: "比较并预览（1）" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("核对替换计划", { exact: true })).toBeVisible();
  await expect(
    dialog.locator(".replacement-comparison .open-path-button"),
  ).toHaveCount(3);
  await expect(
    dialog.getByRole("button", {
      name: "在文件管理器中打开：D:\\Library\\Season 02\\S02E01.mkv",
      exact: true,
    }),
  ).toHaveCount(1);
  await expect(
    dialog.getByRole("button", {
      name: "在文件管理器中打开：D:\\Library\\Season 02\\S02E01.ts",
      exact: true,
    }),
  ).toHaveCount(1);
  await dialog
    .getByRole("button", {
      name: "在文件管理器中打开：D:\\Library\\Season 02\\S02E01.mkv",
      exact: true,
    })
    .click();
  await expect(
    dialog.getByRole("button", { name: "确认并创建任务" }),
  ).toBeDisabled();
  await dialog.getByRole("checkbox").click();
  await page.screenshot({
    path: "test-results/replacement.png",
    fullPage: true,
  });
  await dialog.getByRole("button", { name: "确认并创建任务" }).click();
  await expect(page.getByRole("heading", { name: "整理任务" })).toBeVisible();
  await expect(
    page.locator(".plan-table .target-path .open-path-button"),
  ).toHaveCount(0);
  await page.locator(".plan-table .open-path-button").first().click();
  expect(
    await page.evaluate(
      () =>
        (window as any).__calls.filter((c: any) => c.command === "run_plan")
          .length,
    ),
  ).toBe(0);
  await page.getByRole("button", { name: "确认整理", exact: true }).click();
  await page.getByRole("button", { name: "确认并开始", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__calls.filter((c: any) => c.command === "run_plan")
            .length,
      ),
    )
    .toBe(1);
});

test("season rename previews files and NFO backups before creating a fixed queue task", async ({
  page,
}) => {
  await fixture(page, 2, false, "resources");
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.locator(".work-card").click();
  await expect(
    page.getByRole("button", { name: "重命名入库资源", exact: true }),
  ).toBeEnabled();
  await page.locator('.season-menu-item[data-season="2"]').click();
  await page
    .getByRole("button", { name: "重命名本季资源", exact: true })
    .click();
  const dialog = page.getByRole("dialog");
  await expect(
    dialog.getByRole("heading", { name: "检查重命名计划" }),
  ).toBeVisible();
  await expect(dialog.locator(".rename-table tbody tr")).toHaveCount(3);
  await expect(dialog).toContainText("D:\\Library\\Season 03\\S03E01.ts");
  await dialog.getByRole("button", { name: "NFO 备份 · 2 个文件" }).click();
  await expect(dialog).toContainText("D:\\Backups\\rename1\\original0.nfo");
  for (const width of [1440, 1050]) {
    await page.setViewportSize({ width, height: 700 });
    await expect
      .poll(async () => {
        const box = await dialog.boundingBox();
        return !!box && box.x >= 23 && box.x + box.width <= width - 23;
      })
      .toBe(true);
    await expect(
      dialog.getByRole("button", { name: "确认并创建任务" }),
    ).toBeInViewport();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: "test-results/rename-" + width + ".png",
      fullPage: true,
      animations: "disabled",
    });
  }
  expect(
    await page.evaluate(() =>
      (window as any).__calls.filter((c: any) =>
        ["create_rename_plan", "run_plan"].includes(c.command),
      ),
    ),
  ).toEqual([]);
  await dialog.getByRole("button", { name: "确认并创建任务" }).click();
  await expect(
    page.getByRole("heading", { name: "检查重命名计划" }),
  ).toBeVisible();
  await expect(page.locator(".job-batch-controls")).toHaveCount(0);
  await expect(page.locator(".plan-table [role=combobox]")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      (window as any).__calls.filter((c: any) => c.command === "run_plan"),
    ),
  ).toEqual([]);
  await page.getByRole("button", { name: "确认整理", exact: true }).click();
  await page.getByRole("button", { name: "确认并开始", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "暂停", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "取消任务", exact: true }),
  ).toBeDisabled();
  expect(
    await page.evaluate(() =>
      (window as any).__calls.filter(
        (c: any) => c.command === "save_decisions",
      ),
    ),
  ).toEqual([]);
});

test("resource rename selection handles offline errors, conflicts, no-op and changed-file retry", async ({
  page,
}) => {
  await fixture(page, 2, false, "resources");
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await page.getByLabel("选择 S02E01.ts", { exact: true }).check();
  await page.evaluate(() => {
    (window as any).__renameError = true;
  });
  await page.getByRole("button", { name: "重命名选中资源" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("alert")).toContainText("共享目录离线");
  await page.evaluate(() => {
    (window as any).__renameError = false;
    (window as any).__renameConflict = true;
  });
  await dialog.getByRole("button", { name: "重新预览" }).click();
  await expect(dialog).toContainText("目标已被未选中的文件占用");
  await expect(
    dialog.getByRole("button", { name: "确认并创建任务" }),
  ).toBeDisabled();
  await page.evaluate(() => {
    (window as any).__renameConflict = false;
    (window as any).__renameNoop = true;
  });
  await dialog.getByRole("button", { name: "重新预览" }).click();
  await expect(dialog).toContainText("无需重命名");
  await expect(
    dialog.getByRole("button", { name: "确认并创建任务" }),
  ).toBeDisabled();
  await page.evaluate(() => {
    (window as any).__renameNoop = false;
    (window as any).__renameCreateError = true;
  });
  await dialog.getByRole("button", { name: "重新预览" }).click();
  await dialog.getByRole("button", { name: "确认并创建任务" }).click();
  await expect(dialog.getByRole("alert")).toContainText("文件在预览后发生变化");
  await expect(dialog).toBeVisible();
  expect(await page.evaluate(() => (window as any).__plans.length)).toBe(0);
  const paths = await page.evaluate(() =>
    (window as any).__calls
      .filter((c: any) => c.command === "preview_renames")
      .map((c: any) => c.args.paths),
  );
  expect(
    paths.every(
      (p: string[]) =>
        p.length === 1 && p[0] === "D:\\Library\\Season 02\\S02E01.ts",
    ),
  ).toBe(true);
  await page.evaluate(() => {
    (window as any).__renameCreateError = false;
  });
  await dialog.getByRole("button", { name: "重新预览" }).click();
  await dialog.getByRole("button", { name: "确认并创建任务" }).click();
  await expect(
    page.getByRole("heading", { name: "检查重命名计划" }),
  ).toBeVisible();
  expect(await page.evaluate(() => (window as any).__plans.length)).toBe(1);
});

test("season resource scan and backup cleanup expose exact files before changes", async ({
  page,
}) => {
  await fixture(page, 2, false, "resources");
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page
    .getByRole("button", { name: /FICTIONAL VOYAGE/ })
    .first()
    .click();
  await page
    .getByRole("button", { name: /第二季/ })
    .first()
    .click();
  await page.getByRole("button", { name: "扫描替代文件", exact: true }).click();
  await expect(
    page.getByRole("dialog").getByText("S02E01.ts", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "取消", exact: true })
    .click();
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await page.getByRole("tab", { name: "旧版本备份" }).click();
  await page.getByRole("button", { name: "全选可清理备份" }).click();
  await page.getByRole("button", { name: "清理选中备份" }).click();
  await expect(
    page
      .getByRole("dialog")
      .getByText("D:\\Backups\\old-task\\old.ts", { exact: false }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        (window as any).__calls.filter(
          (c: any) => c.command === "cleanup_backups",
        ).length,
    ),
  ).toBe(0);
  await page.getByRole("button", { name: "确认永久删除", exact: true }).click();
  await expect(page.getByText("已清理", { exact: true })).toBeVisible();
});

test("batch matching previews destinations and requires explicit confirmation", async ({
  page,
}) => {
  await fixture(page);
  await page.getByLabel("选择筛选结果").click();
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  await pick(page, page.getByLabel("数字位置", { exact: false }), "0");
  await page.getByRole("button", { name: "应用批量匹配" }).click();
  await page.screenshot({ path: "test-results/sources.png", fullPage: true });
  await page.getByRole("button", { name: "创建并查看" }).click();
  await expect(
    page.getByText(
      "→ D:\\Library\\TV Shows\\FICTIONAL VOYAGE\\Season 02\\S02E01.mkv",
    ),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        (window as any).__calls.filter((c: any) => c.command === "run_plan")
          .length,
    ),
  ).toBe(0);
  await page.getByRole("button", { name: "确认整理", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.screenshot({
    path: "test-results/copy-confirmation.png",
    fullPage: true,
    animations: "disabled",
  });
  expect(
    await page.evaluate(
      () =>
        (window as any).__calls.filter((c: any) => c.command === "run_plan")
          .length,
    ),
  ).toBe(0);
  await page.getByRole("button", { name: "确认并开始" }).click();
  await expect(page.getByRole("dialog")).toBeHidden();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__calls.filter((c: any) => c.command === "run_plan")
            .length,
      ),
    )
    .toBe(1);
  await page.screenshot({ path: "test-results/copy-plan.png", fullPage: true });
});
test("browse metadata without files and keep season identity explicit", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await pick(page, page.getByLabel("数据源"), "bangumi");
  await page.getByLabel("搜索词").fill("FICTIONAL VOYAGE");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await page.getByRole("button", { name: /FICTIONAL VOYAGE’.*2012/ }).click();
  await expect(page.getByLabel("关联本地条目")).toHaveAttribute(
    "data-value",
    "w1",
  );
  await expect(page.getByLabel("本条目对应的本地季号")).toHaveValue("2");
  await page.getByRole("button", { name: "更新本地目录信息" }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__calls.find((c: any) => c.command === "bind_subject")
            ?.args.request.season,
      ),
    )
    .toBe(2);
});
test("missing episode accepts one file and disabled specials cannot accept drops", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await page.locator('.season-menu-item[data-season="2"]').click();
  await expect(
    page.getByText("尚未入库", { exact: true }).first(),
  ).toBeVisible();
  const row = page.locator('[data-drop-episode="e1"]');
  const box = await row.boundingBox();
  await page.evaluate(
    ({ x, y }) => (window as any).__drop(["D:\\Incoming\\01.mkv"], x, y),
    { x: box!.x + 20, y: box!.y + 20 },
  );
  await page.getByRole("button", { name: /^待导入 ·/ }).click();
  await expect(
    page.getByRole("region", { name: "待提交文件", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("heading", { name: "检查整理计划" })).toHaveCount(
    0,
  );
  expect(
    await page.evaluate(
      () =>
        (window as any).__calls.find(
          (c: any) => c.command === "preview_organize",
        ).args.mappings[0].episode_ids,
    ),
  ).toEqual(["e1"]);
  await page.getByRole("button", { name: "提交并查看", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "检查整理计划" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await page.locator('.season-menu-item[data-season="0"]').click();
  const sp = await page.locator('[data-drop-episode="sp"]').boundingBox();
  await page.evaluate(
    ({ x, y }) => (window as any).__drop(["D:\\Incoming\\OVA.mkv"], x, y),
    { x: sp!.x + 20, y: sp!.y + 20 },
  );
  await expect(
    page.locator('.floating-notice[data-tone="warning"]').last(),
  ).toContainText("请先启用此集");
  await page.screenshot({ path: "test-results/catalog.png", fullPage: true });
});
async function dropInto(page: Page, target: Locator, paths: string[], review = true) {
  const dialog = page.getByRole("dialog", { name: "待提交文件", exact: true });
  if (await dialog.isVisible()) {
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    await expect(page.locator('[data-slot="dialog-overlay"]')).toHaveCount(0);
  }
  await target.scrollIntoViewIfNeeded();
  const box = await target.boundingBox();
  await page.evaluate(
    ({ paths, x, y }) => (window as any).__drop(paths, x, y),
    {
      paths,
      x: box!.x + 20,
      y: box!.y + 20,
    },
  );
  if (review) {
    await expect(page.getByRole("dialog", { name: "待提交文件", exact: true })).toHaveCount(0);
    await page.getByRole("button", { name: /^待导入 ·/ }).click();
  }
}

test("season drops accumulate inline and submit-and-run stays on the season", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await page.locator('.season-menu-item[data-season="2"]').click();
  const preview = page.getByRole("region", { name: "待提交文件", exact: true });
  await dropInto(page, page.locator('[data-drop-episode="e1"]'), [
    "D:\\Incoming\\First.mkv",
  ]);
  await expect(preview).toContainText("First.mkv");
  await dropInto(page, page.locator('[data-drop-episode="e2"]'), [
    "D:\\Incoming\\Second.mkv",
  ]);
  await expect(preview.locator(".library-import-row")).toHaveCount(2);
  await dropInto(page, page.locator('[data-drop-episode="e1"]'), [
    "D:\\Incoming\\Updated.mkv",
  ]);
  await expect(preview).toContainText("Updated.mkv");
  await expect(preview).not.toContainText("First.mkv");
  expect(await page.evaluate(() => (window as any).__plans.length)).toBe(0);
  await page.screenshot({
    path: "test-results/season-inline-import.png",
    animations: "disabled",
    fullPage: true,
  });
  await pick(
    page,
    page.getByRole("combobox", { name: "切换提交操作", exact: true }),
    "execute",
  );
  await page.getByRole("button", { name: "提交并执行", exact: true }).click();
  await expect(preview).toHaveCount(0);
  await expect(
    page.locator('.season-menu-item[data-season="2"]'),
  ).toHaveAttribute("aria-current", "page");
  await expect(page.getByRole("heading", { name: "检查整理计划" })).toHaveCount(
    0,
  );
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__calls.filter((c: any) => c.command === "run_plan")
            .length,
      ),
    )
    .toBe(1);
  expect(await page.evaluate(() => (window as any).__plans.length)).toBe(1);
});

test("season drop accepts distinct parts of one episode and submits both", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await page.locator('.season-menu-item[data-season="2"]').click();
  await dropInto(page, page.locator('[data-drop-episode="e1"]'), [
    "D:\\Incoming\\Fictional.Part1.mkv",
    "D:\\Incoming\\Fictional.Part2.mkv",
  ]);
  const preview = page.getByRole("region", { name: "待提交文件", exact: true });
  await expect(preview.locator(".library-import-row")).toHaveCount(2);
  await expect(preview.getByText("Part 1", { exact: true })).toBeVisible();
  await expect(preview.getByText("Part 2", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "提交并查看", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "提交并查看", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "检查整理计划" }),
  ).toBeVisible();
  const rows = await page.evaluate(() => (window as any).__plans[0].rows);
  expect(rows.map((r: any) => [r.episode_ids, r.part])).toEqual([
    [["e1"], 1],
    [["e1"], 2],
  ]);
});

test("extras drop previews relative paths and keeps conflicts after a failed submission", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await page.evaluate(() => {
    (window as any).__organizeConflict = true;
    (window as any).__extraScan = {
      files: [
        { path: "D:\\Incoming\\Bonus\\Book A\\page.jpg" },
        { path: "D:\\Incoming\\Bonus\\Book B\\page.jpg" },
      ],
      errors: [],
    };
  });
  await dropInto(
    page,
    page.getByRole("button", { name: "附加文件", exact: true }),
    ["D:\\Incoming\\Bonus"],
  );
  const preview = page.getByRole("region", { name: "待提交文件", exact: true });
  await expect(
    page.getByRole("region", { name: "拖拽导入额外文件", exact: true, includeHidden: true }),
  ).toBeVisible();
  await expect(preview.locator(".library-import-row")).toHaveCount(2);
  await expect(preview).toContainText("Book A\\page.jpg");
  await expect(preview).toContainText("Book B\\page.jpg");
  await expect(
    page.getByRole("button", { name: "提交并查看", exact: true }),
  ).toBeDisabled();
  await pick(
    page,
    preview
      .getByRole("combobox", { name: "page.jpg 冲突处理", exact: true })
      .first(),
    "replace",
  );
  await page.evaluate(() => {
    (window as any).__failSubmit = true;
  });
  await page.getByRole("button", { name: "提交并查看", exact: true }).click();
  await expect(preview.getByRole("alert")).toContainText(
    "请重新生成预览并核对",
  );
  await expect(preview.locator(".library-import-row")).toHaveCount(2);
  expect(await page.evaluate(() => (window as any).__plans.length)).toBe(0);
  await page.setViewportSize({ width: 1000, height: 760 });
  await expect
    .poll(() =>
      page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
    )
    .toBe(true);
  await page.screenshot({
    path: "test-results/extras-inline-import.png",
    fullPage: true,
  });
  await page.evaluate(() => {
    (window as any).__failSubmit = false;
  });
  await page.getByRole("button", { name: "重新预览", exact: true }).click();
  await expect(preview.getByRole("alert")).toHaveCount(0);
  await pick(
    page,
    preview
      .getByRole("combobox", { name: "page.jpg 冲突处理", exact: true })
      .first(),
    "replace",
  );
  await page.getByRole("button", { name: "提交并查看", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "检查整理计划" }),
  ).toBeVisible();
  expect(
    await page.evaluate(() =>
      (window as any).__plans[0].rows.every(
        (r: any) => r.extra && !r.episode_ids.length,
      ),
    ),
  ).toBe(true);
  expect(
    await page.evaluate(
      () =>
        (window as any).__calls.filter((c: any) => c.command === "run_plan")
          .length,
    ),
  ).toBe(0);
});

test("discarding inline files and leaving during preview never creates a task", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await page.locator('.season-menu-item[data-season="2"]').click();
  await dropInto(page, page.locator('[data-drop-episode="e1"]'), [
    "D:\\Incoming\\First.mkv",
  ]);
  const preview = page.getByRole("region", { name: "待提交文件", exact: true });
  await expect(preview).toBeVisible();
  await preview
    .getByRole("button", { name: "移除此文件", exact: true })
    .click();
  await expect(preview).toHaveCount(0);
  await page.evaluate(() => {
    (window as any).__holdOrganizePreview = true;
  });
  await dropInto(page, page.locator('[data-drop-episode="e1"]'), [
    "D:\\Incoming\\Later.mkv",
  ], false);
  await expect
    .poll(() =>
      page.evaluate(() => typeof (window as any).__finishOrganizePreview),
    )
    .toBe("function");
  await page.getByRole("button", { name: "← 返回作品", exact: true }).click();
  await page.evaluate(() => (window as any).__finishOrganizePreview());
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__calls.filter(
            (c: any) => c.command === "discard_organize_preview",
          ).length,
      ),
    )
    .toBe(2);
  expect(await page.evaluate(() => (window as any).__plans.length)).toBe(0);
  await expect(preview).toHaveCount(0);
});

test("settings accept UNC paths and persist verification choice", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page
    .getByLabel("整体输出目录", { exact: true })
    .fill("\\\\nas\\media\\TV Shows");
  await page.getByLabel("完整读取并校验 SHA-256").click();
  await page.getByRole("button", { name: "保存全部设置" }).click();
  await page.screenshot({ path: "test-results/settings.png", fullPage: true });
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__calls.find(
            (c: any) => c.command === "save_settings",
          )?.args.settings.output_root,
      ),
    )
    .toBe("\\\\nas\\media\\TV Shows");
});
test("catalog tabs, season names, scoped refresh and compact navigation", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await expect(page.getByRole("tab", { name: "电视剧" })).toHaveAttribute(
    "data-state",
    "active",
  );
  await page.getByLabel("搜索已入库作品").fill("FICTIONAL");
  await page.screenshot({
    path: "test-results/catalog-grid.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await page.locator('.season-menu-item[data-season="2"]').click();
  await page.getByLabel("第 2 季名称").fill("第二章：归来");
  await expect(page.getByRole("button", { name: "同步本季" })).toBeDisabled();
  await page.getByRole("button", { name: "保存修改" }).click();
  await page.getByRole("button", { name: "同步本季" }).click();
  expect(
    await page.evaluate(
      () =>
        (window as any).__calls.find((c: any) => c.command === "sync_work")
          .args,
    ),
  ).toMatchObject({ workId: "w1", bindingId: "b1", season: 1 });
  await page.setViewportSize({ width: 1050, height: 800 });
  await expect(
    page.getByRole("button", { name: "整理", exact: true }),
  ).toBeVisible();
  await page.screenshot({
    path: "test-results/season-detail-1050.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "← 返回作品" }).click();
  await page.getByRole("tab", { name: "音乐", exact: true }).click();
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await expect(page.getByLabel("类型", { exact: true })).toHaveAttribute(
    "data-value",
    "music",
  );
});
test("TMDB import hides local season input and success notice expires", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await expect(page.getByLabel("数据源")).toHaveAttribute("data-value", "tmdb");
  await page.getByLabel("远程条目 ID").fill("100");
  await page.getByRole("button", { name: "加载 ID" }).click();
  await expect(page.getByLabel("本条目对应的本地季号")).toHaveCount(0);
  await page.getByRole("button", { name: "加入本地目录" }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByRole("dialog").getByRole("status")).toContainText(
    "已添加到「FICTIONAL VOYAGE」",
  );
  await page.getByRole("button", { name: "关闭添加窗口" }).click();
  await expect(page.getByRole("dialog")).toBeHidden();
  await expect(
    page.getByText("已关联 FICTIONAL VOYAGE", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("已关联 FICTIONAL VOYAGE", { exact: true }),
  ).toBeHidden({
    timeout: 10000,
  });
});

test("TMDB queries remain available during organizing but binding waits", async ({
  page,
}) => {
  await fixture(page);
  await createTask(page);
  await page.evaluate(() => (window as any).__jobEvent("p1", "running"));
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await expect(page.getByLabel("数据源")).toHaveAttribute("data-value", "tmdb");
  await page.getByLabel("搜索词").fill("虚构河岸之行");
  await expect(
    page.getByRole("button", { name: "搜索", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await page.getByLabel("远程条目 ID").fill("910000020");
  await page.getByRole("button", { name: "加载 ID" }).click();
  await expect(
    page.getByRole("heading", { name: "虚构河岸之行" }),
  ).toBeVisible();
  await expect(page.getByLabel("类型", { exact: true })).toHaveAttribute(
    "data-value",
    "movie",
  );
  await expect(
    page.getByRole("button", { name: "加入本地目录" }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "关闭添加窗口" }),
  ).toBeEnabled();
});

test("existing TMDB work can preview and confirm an episode group", async ({
  page,
}) => {
  await fixture(page, 2, false, "tmdb-groups");
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await page
    .getByRole("button", { name: "切换 TMDB 剧集组", exact: true })
    .click();
  await expect(
    page.getByText("当前排序：TMDB 默认季度（未选择剧集组）"),
  ).toBeVisible();
  await page.getByRole("button", { name: "加载可选剧集组" }).click();
  await pick(
    page,
    page.getByLabel("剧集组", { exact: true }),
    "abcdef0123456789abcdef01",
  );
  await expect(
    page.getByText("当前排序：虚构群岛纪事 · 19 组 / 114 集"),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "确认切换剧集组" }),
  ).toBeDisabled();
  await page.getByRole("button", { name: "核对编号变化" }).click();
  await expect(page.getByText(/S2E1 → S3E1/)).toBeVisible();
  await page.getByLabel("已核对季集变化并确认切换").check();
  await page.getByRole("button", { name: "确认切换剧集组" }).click();
  const request = await page.evaluate(
    () =>
      (window as any).__calls.find((c: any) => c.command === "bind_subject")
        .args.request,
  );
  expect(request.group_change_confirmed).toBe(true);
  expect(request.subject.episode_group.id).toBe("abcdef0123456789abcdef01");
  expect(request.work_id).toBe("w1");
  await expect(page.getByRole("dialog")).toBeHidden();
});

test("matching an existing TMDB entry keeps its saved episode group", async ({
  page,
}) => {
  await fixture(page, 2, false, "tmdb-saved-group");
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await page.getByLabel("远程条目 ID").fill("910000030");
  await page.getByRole("button", { name: "加载 ID" }).click();
  await expect(page.getByLabel("关联本地条目")).toHaveAttribute(
    "data-value",
    "w1",
  );
  await expect(
    page.getByText("当前排序：虚构群岛纪事 · 19 组 / 114 集"),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "确认切换剧集组" }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "更新本地目录信息" }),
  ).toBeEnabled();
  await page.screenshot({
    path: "test-results/existing-catalog-update.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "更新本地目录信息" }).click();
  const calls = await page.evaluate(() => (window as any).__calls);
  expect(
    calls.filter((call: any) => call.command === "media_detail"),
  ).toHaveLength(1);
  expect(
    calls.find((call: any) => call.command === "media_detail").args
      .episodeGroupId,
  ).toBe("abcdef0123456789abcdef01");
  expect(
    calls.find((call: any) => call.command === "bind_subject").args.request,
  ).toMatchObject({
    work_id: "w1",
    group_change_confirmed: false,
    subject: { episode_group: { id: "abcdef0123456789abcdef01" } },
  });
});

test("settings show timings and expose the log directory", async ({ page }) => {
  await fixture(page);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByRole("heading", { name: "耗时与日志" })).toBeVisible();
  await expect(
    page.locator(".diagnostics-panel .open-path-button"),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "操作统计 · 1 项" }).click();
  await expect(
    page.getByRole("cell", { name: "1.40 s", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "刷新耗时统计" }).click();
  await page.getByRole("button", { name: "打开日志目录" }).click();
  expect(
    await page.evaluate(
      () =>
        (window as any).__calls.filter(
          (c: any) => c.command === "open_log_directory",
        ).length,
    ),
  ).toBe(1);
  await page.screenshot({
    path: "test-results/diagnostics.png",
    fullPage: true,
  });
});

async function readable(page: Page) {
  const invalid = await page.evaluate(() =>
    [...document.querySelectorAll<HTMLElement>("body *")]
      .filter((el) => {
        const rect = el.getBoundingClientRect(),
          style = getComputedStyle(el);
        const text = [...el.childNodes].some(
          (n) => n.nodeType === Node.TEXT_NODE && n.textContent?.trim(),
        );
        return (
          (text || el.matches("input, select, textarea")) &&
          rect.width > 0 &&
          rect.height > 0 &&
          style.visibility !== "hidden" &&
          parseFloat(style.fontSize) < 12
        );
      })
      .map((el) => ({
        text: el.textContent?.slice(0, 40),
        class: el.className,
        font: getComputedStyle(el).fontSize,
      })),
  );
  expect(invalid).toEqual([]);
  const primary = await page
    .locator(
      'input, [data-slot="select-trigger"], textarea, [data-slot="button"], .file-name, .field > span, .work-card strong',
    )
    .evaluateAll((els) =>
      els
        .filter(
          (el) =>
            el.getClientRects().length &&
            parseFloat(getComputedStyle(el).fontSize) < 14,
        )
        .map((el) => el.outerHTML.slice(0, 150)),
    );
  expect(primary).toEqual([]);
}
test("readable typography and dense source viewport with loaded media details", async ({
  page,
}) => {
  await fixture(page, 24);
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  for (const viewport of [
    { width: 1440, height: 960 },
    { width: 1050, height: 700 },
  ]) {
    await page.setViewportSize(viewport);
    await readable(page);
    const layout = await page.evaluate(() => {
      const pane = document
        .querySelector(".source-panel")!
        .getBoundingClientRect();
      const view = document
        .querySelector(".file-viewport")!
        .getBoundingClientRect();
      const rows = [...document.querySelectorAll(".file-table tbody tr")].map(
        (el) => el.getBoundingClientRect(),
      );
      return {
        ratio: view.height / pane.height,
        visible: rows.filter(
          (r) => r.top >= view.top && r.bottom <= view.bottom,
        ).length,
        overflow: document.documentElement.scrollWidth - innerWidth,
        height: document.documentElement.scrollHeight - innerHeight,
      };
    });
    expect(layout.ratio).toBeGreaterThan(0.65);
    expect(layout.visible).toBeGreaterThanOrEqual(
      viewport.width === 1440 ? 10 : 6,
    );
    expect(layout.overflow).toBeLessThanOrEqual(1);
    expect(layout.height).toBeLessThanOrEqual(1);
    await page.screenshot({
      path: `test-results/sources-${viewport.width}.png`,
    });
  }
  const rowHeight = (await page
    .locator(".file-table tbody tr")
    .first()
    .boundingBox())!.height;
  await page
    .getByRole("button", {
      name: "[FICTIONAL_VOYAGE_2][01].mkv 文件信息",
      exact: true,
    })
    .click();
  await expect(page.getByText(/1920×1080/)).toBeVisible();
  await readable(page);
  expect(
    (await page.locator(".file-table tbody tr").first().boundingBox())!.height,
  ).toBe(rowHeight);
  await page.keyboard.press("Escape");
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await readable(page);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await readable(page);
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await readable(page);
  await page.getByRole("button", { name: "添加季度", exact: true }).click();
  await readable(page);
});
test("file selection supports all filtered pages, row toggles, drag inversion and escape", async ({
  page,
}) => {
  await fixture(page, 96);
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await expect(page.locator(".selection-bar")).toContainText("已选 96 项");
  await page.getByRole("button", { name: "下一页", exact: true }).click();
  await expect(
    page.locator('.file-table tbody tr[aria-selected="true"]'),
  ).toHaveCount(16);
  await page.getByRole("button", { name: "全不选", exact: true }).click();
  await page.getByRole("button", { name: "上一页", exact: true }).click();
  const rows = page.locator(".file-table tbody tr");
  await rows.nth(0).locator(".file-name").click();
  await expect(rows.nth(0)).toHaveAttribute("aria-selected", "true");
  const start = (await rows.nth(0).locator(".file-name").boundingBox())!;
  const end = (await rows.nth(3).locator(".file-name").boundingBox())!;
  await page.mouse.move(start.x + 12, start.y + 8);
  await page.mouse.down();
  await page.mouse.move(end.x + 80, end.y + 8, { steps: 8 });
  await expect(page.locator(".selection-box")).toBeVisible();
  await page.mouse.up();
  await expect(rows.nth(0)).toHaveAttribute("aria-selected", "false");
  for (let i = 1; i < 4; i++)
    await expect(rows.nth(i)).toHaveAttribute("aria-selected", "true");
  await page.mouse.move(start.x + 12, start.y + 8);
  await page.mouse.down();
  await page.mouse.move(end.x + 80, end.y + 8, { steps: 4 });
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await expect(
    page.locator('.file-table tbody tr[aria-selected="true"]'),
  ).toHaveCount(3);
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  await pick(
    page,
    page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 对应集数", { exact: true }),
    "e1",
  );
  await expect(rows.nth(0)).toHaveAttribute("aria-selected", "false");
  await pick(
    page,
    page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 多集结束位置", {
      exact: true,
    }),
    "e2",
  );
  await page.keyboard.press("Escape");
  await expect(rows.nth(0)).toContainText("包含 2 集");
  await expect(rows.nth(0)).toHaveAttribute("aria-selected", "false");
  await rows.nth(0).focus();
  await page.keyboard.press("Space");
  await expect(rows.nth(0)).toHaveAttribute("aria-selected", "true");
  await page.getByRole("button", { name: "全不选", exact: true }).click();
  await page.getByLabel("筛选文件", { exact: true }).fill("[01]");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await expect(page.locator(".selection-bar")).toContainText("已选 1 项");
});
test("job batch choices persist file operations and show a move confirmation", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  await pick(page, page.getByLabel("数字位置", { exact: false }), "0");
  await page.getByRole("button", { name: "应用批量匹配" }).click();
  await page.getByRole("button", { name: "创建并查看" }).click();
  await expect(
    page.getByRole("button", { name: "全选", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByLabel(/^选择任务 /)).toHaveCount(0);
  await pick(page, page.getByLabel("批量文件操作"), "move");
  await page.getByRole("button", { name: "应用文件操作" }).click();
  await expect(
    page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 文件操作", { exact: true }),
  ).toHaveAttribute("data-value", "move");
  await pick(page, page.getByLabel("批量冲突处理"), "skip");
  await page.getByRole("button", { name: "应用处理方式" }).click();
  await expect(
    page.getByLabel("[FICTIONAL_VOYAGE_2][02].mkv 冲突处理", { exact: true }),
  ).toHaveAttribute("data-value", "skip");
  await pick(
    page,
    page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 冲突处理", { exact: true }),
    "copy",
  );
  await readable(page);
  await page.screenshot({
    path: "test-results/jobs-batch.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "确认整理", exact: true }).click();
  await expect(page.getByRole("dialog")).toContainText("校验目标后删除源文件");
  await expect(page.getByRole("dialog")).toContainText("移动：1 个文件");
  await page.getByRole("button", { name: "确认并开始", exact: true }).click();
  const choices = await page.evaluate(
    () =>
      (window as any).__calls.find((c: any) => c.command === "save_decisions")
        .args.choices,
  );
  expect(choices.map((c: any) => [c.action, c.operation])).toEqual([
    ["copy", "move"],
    ["skip", "move"],
  ]);
});

test("episode select shows bilingual names, supports keyboard and keeps range in its own column", async ({
  page,
}) => {
  await fixture(page);
  await page.setViewportSize({ width: 1050, height: 700 });
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  const episode = page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 对应集数", {
    exact: true,
  });
  const range = page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 多集结束位置", {
    exact: true,
  });
  await expect(range).toBeDisabled();
  await episode.focus();
  await page.keyboard.press("ArrowDown");
  const option = page.locator('[role="option"][data-item-value="e1"]');
  await expect(
    page.getByRole("combobox", { name: "搜索集数", exact: true }),
  ).toBeFocused();
  await expect(option).toContainText("新旅程：我们在异世界相遇");
  await expect(option).toContainText("原名：旅の始まり 1");
  await expect(
    page.locator('[role="option"][data-item-value="e2"]'),
  ).toContainText("原名未提供");
  await readable(page);
  await page.screenshot({
    path: "test-results/episode-select-1050.png",
    animations: "disabled",
  });
  await page.keyboard.press("Home");
  await expect(
    page.locator('[role="option"][data-item-value=""]'),
  ).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowDown");
  await expect(
    page.locator('[role="option"][data-item-value="extra"]'),
  ).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowDown");
  await expect(option).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Enter");
  await expect(episode).toHaveAttribute("data-value", "e1");
  await expect(episode).toContainText("新旅程：我们在异世界相遇");
  await expect(episode).toContainText("旅の始まり 1");
  await expect(range).toBeEnabled();
  await expect(
    page
      .locator(".file-table tbody tr")
      .first()
      .locator("td")
      .nth(4)
      .getByRole("combobox"),
  ).toHaveAttribute("aria-label", "[FICTIONAL_VOYAGE_2][01].mkv 多集结束位置");
  await pick(page, range, "e2");
  await expect(range).toContainText("包含至 S02E02");
  await expect(page.locator(".file-table tbody tr").first()).toHaveAttribute(
    "aria-selected",
    "false",
  );
  expect(
    await page.evaluate(() =>
      Object.values(JSON.parse(localStorage.getItem("bangumi.mappings")!)).map(
        (r: any) => r.episode_ids,
      ),
    ),
  ).toEqual([["e1", "e2"]]);
  await pick(page, range, "");
  await expect(range).toContainText("单集文件");
  await pick(page, episode, "extra");
  await expect(range).toBeDisabled();
  expect(
    await page.evaluate(() =>
      Object.values(JSON.parse(localStorage.getItem("bangumi.mappings")!)).map(
        (r: any) => [r.extra, r.episode_ids],
      ),
    ),
  ).toEqual([[true, []]]);
  await pick(page, episode, "");
  await expect(episode).toContainText("待匹配");
});
test("create-only queues separate tasks without navigation or file execution", async ({
  page,
}) => {
  await fixture(page);
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  for (let n = 1; n <= 2; n++) {
    const name = `[FICTIONAL_VOYAGE_2][0${n}].mkv`;
    await page.getByLabel(`选择 ${name}`, { exact: true }).click();
    await pick(
      page,
      page.getByLabel(`${name} 对应集数`, { exact: true }),
      `e${n}`,
    );
    await pick(page, page.getByLabel("切换创建任务操作"), "create");
    await page.getByRole("button", { name: "仅创建", exact: true }).click();
    await expect(page.locator(".source-panel")).toBeVisible();
    await expect(
      page.locator('.floating-notice[role="status"]').last(),
    ).toContainText("已创建任务（1 项）");
    await expect(page.locator(".selection-bar")).toContainText("已选 0 项");
    await expect(
      page.getByRole("button", { name: "仅创建", exact: true }),
    ).toBeDisabled();
  }
  const calls = await page.evaluate(() => (window as any).__calls);
  expect(
    calls
      .filter((c: any) => c.command === "create_plan")
      .map((c: any) => c.args.mappings[0].episode_ids),
  ).toEqual([["e1"], ["e2"]]);
  expect(calls.filter((c: any) => c.command === "run_plan")).toHaveLength(0);
  await page
    .getByRole("navigation")
    .getByRole("button", { name: /任务/ })
    .click();
  await expect(page.locator(".job-list .job-item")).toHaveCount(2);
});

test("failed task creation preserves selection and mappings for retry", async ({
  page,
}) => {
  await fixture(page);
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await pick(page, page.getByLabel("数字位置", { exact: false }), "0");
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  await page.evaluate(() => {
    (window as any).__failPlan = true;
  });
  await pick(page, page.getByLabel("切换创建任务操作"), "create");
  await page.getByRole("button", { name: "仅创建", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("无法读取源文件");
  await expect(page.locator(".selection-bar")).toContainText("已选 2 项");
  await expect(
    page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 对应集数", { exact: true }),
  ).toHaveAttribute("data-value", "e1");
  await page.evaluate(() => {
    (window as any).__failPlan = false;
  });
  await pick(page, page.getByLabel("切换创建任务操作"), "create");
  await page.getByRole("button", { name: "仅创建", exact: true }).click();
  await expect(
    page.locator('.floating-notice[data-tone="success"]').last(),
  ).toContainText("已创建任务（2 项）");
  await expect(page.locator(".selection-bar")).toContainText("已选 0 项");
  expect(
    await page.evaluate(() =>
      (window as any).__calls.filter((c: any) => c.command === "run_plan"),
    ),
  ).toHaveLength(0);
});

async function createTask(page: Page) {
  await page.getByLabel("选择筛选结果").click();
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  await pick(page, page.getByLabel("数字位置", { exact: false }), "0");
  await page.getByRole("button", { name: "应用批量匹配" }).click();
  await page.getByRole("button", { name: "创建并查看" }).click();
  await expect(
    page.getByRole("button", { name: "取消任务", exact: true }),
  ).toBeVisible();
}
test("cancel draft task persists status and allows retry after a failed request", async ({
  page,
}) => {
  await fixture(page);
  await createTask(page);
  await page.evaluate(() => {
    (window as any).__failCancel = true;
  });
  await page.getByRole("button", { name: "取消任务", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("任务取消失败");
  await expect(
    page.getByRole("button", { name: "确认整理", exact: true }),
  ).toBeEnabled();
  await page.evaluate(() => {
    (window as any).__failCancel = false;
  });
  await page.getByRole("button", { name: "取消任务", exact: true }).click();
  await expect(
    page.locator('.floating-notice[role="status"]').last(),
  ).toContainText("任务已取消");
  await expect(
    page.getByRole("button", { name: "取消任务", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: /确认整理|继续 \/ 重试/ }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "导出计划 / 冲突报告" }),
  ).toBeEnabled();
  await expect(
    page.getByRole("button", { name: "任务", exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        (window as any).__calls.filter((c: any) => c.command === "run_plan")
          .length,
    ),
  ).toBe(0);
  await page.screenshot({
    path: "test-results/task-cancelled.png",
    fullPage: true,
  });
});
test("cancel running task waits for shutdown and ignores stale progress", async ({
  page,
}) => {
  await fixture(page);
  await createTask(page);
  await page.getByRole("button", { name: "确认整理", exact: true }).click();
  await page.getByRole("button", { name: "确认并开始", exact: true }).click();
  await page.evaluate(() => {
    (window as any).__jobEvent("p1", "running");
  });
  await expect(
    page.getByRole("button", { name: "暂停", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "取消任务", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "正在取消…", exact: true }),
  ).toBeDisabled();
  await page.evaluate(() => {
    (window as any).__jobEvent("p1", "running");
  });
  await expect(
    page.getByRole("button", { name: "正在取消…", exact: true }),
  ).toBeDisabled();
  await page.evaluate(() => {
    const p = (window as any).__plans[0];
    p.rows[0].state = "complete";
    p.rows[1].state = "cancelled";
    (window as any).__jobEvent("p1", "cancelled", "job-finished");
  });
  await expect(
    page.locator('.floating-notice[role="status"]').last(),
  ).toContainText("任务已取消");
  await expect(page.locator(".plan-table tbody tr").first()).toContainText(
    "已完成",
  );
  await expect(page.locator(".plan-table tbody tr").nth(1)).toContainText(
    "已取消",
  );
  expect(
    await page.evaluate(
      () =>
        (window as any).__calls.find((c: any) => c.command === "cancel_plan")
          .args,
    ),
  ).toMatchObject({ id: "p1" });
  await expect(
    page.getByRole("button", { name: /继续 \/ 重试|暂停/ }),
  ).toHaveCount(0);
});

test("catalog scan shows imported NFO works and leaves conflicts for review", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.evaluate(() => {
    (window as any).__importNfo = true;
  });
  await page.getByRole("button", { name: "扫描", exact: true }).click();
  await expect(
    page.getByText("本次从 NFO / 本地资料导入 1 部作品"),
  ).toBeVisible();
  await page.getByRole("tab", { name: "电影", exact: true }).click();
  await expect(
    page.getByRole("button", { name: /虚构星海旅人！.*2026/ }),
  ).toBeVisible();
  await page.getByRole("button", { name: "1 个目录尚未关联" }).click();
  await expect(page.locator(".unbound-row")).toContainText(
    "NFO ID 同时指向多个本地作品",
  );
  await page.screenshot({
    path: "test-results/nfo-import.png",
    fullPage: true,
  });
});

test("task badge does not move navigation titles and notices have even padding", async ({
  page,
}) => {
  await fixture(page);
  const taskTitle = page
    .locator('.sidebar button[aria-label="任务"] > span')
    .first();
  const original = await taskTitle.boundingBox();
  await createTask(page);
  await expect(page.locator(".nav-task-badge")).toHaveText("1");
  const after = await taskTitle.boundingBox();
  expect(after!.x).toBeCloseTo(original!.x, 1);
  expect(after!.y).toBeCloseTo(original!.y, 1);
  const otherTitle = await page
    .locator('.sidebar button[aria-label="整理"] > span')
    .boundingBox();
  expect(after!.x).toBeCloseTo(otherTitle!.x, 1);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.getByRole("button", { name: "保存全部设置" }).click();
  const alert = page.locator('.floating-notice[data-tone="success"]').last();
  await expect(alert).toContainText("设置已保存");
  const dimensions = await alert.evaluate((node) => {
    const box = node.getBoundingClientRect();
    const text = node
      .querySelector(".floating-notice-body")!
      .getBoundingClientRect();
    return {
      top: text.top - box.top,
      bottom: box.bottom - text.bottom,
      height: box.height,
    };
  });
  expect(Math.abs(dimensions.top - dimensions.bottom)).toBeLessThan(1);
  expect(dimensions.height).toBeLessThanOrEqual(95);
  const box = (await alert.boundingBox())!;
  expect(box.x).toBe(16);
  expect(box.y + box.height).toBe(page.viewportSize()!.height - 16);
  await page.screenshot({
    path: "test-results/sidebar-notice.png",
    fullPage: true,
  });
});

for (const state of ["cancelled", "complete"])
  test(`permanently delete ${state} task, retry errors and ignore late events`, async ({
    page,
  }) => {
    await fixture(page);
    await createTask(page);
    await expect(
      page.getByRole("button", { name: "永久删除", exact: true }),
    ).toHaveCount(0);
    if (state === "cancelled")
      await page.getByRole("button", { name: "取消任务", exact: true }).click();
    else
      await page.evaluate(() =>
        (window as any).__jobEvent("p1", "complete", "job-finished"),
      );
    await page.getByRole("button", { name: "永久删除", exact: true }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toContainText(
      "源文件、已整理的媒体、备份及临时文件均会保留",
    );
    await dialog.getByRole("button", { name: "返回", exact: true }).click();
    await expect(page.locator(".job-item")).toHaveCount(1);
    await page.getByRole("button", { name: "永久删除", exact: true }).click();
    await page.evaluate(() => {
      (window as any).__failDelete = true;
    });
    await dialog.getByRole("button", { name: "确认永久删除" }).click();
    await expect(
      page.locator('.floating-notice[data-tone="error"]').last(),
    ).toContainText("任务删除失败");
    await page.getByRole("button", { name: "关闭错误消息" }).click();
    await expect(
      page.locator('.floating-notice[data-tone="error"]'),
    ).toHaveCount(0);
    await expect(dialog).toBeVisible();
    const stale = await page.evaluate(() =>
      JSON.parse(JSON.stringify((window as any).__plans[0])),
    );
    await page.evaluate(() => {
      (window as any).__failDelete = false;
    });
    await dialog.getByRole("button", { name: "确认永久删除" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(page.locator(".job-item")).toHaveCount(0);
    await expect(
      page.locator('.floating-notice[data-tone="success"]').last(),
    ).toContainText("任务记录已永久删除");
    await page.evaluate((plan) => (window as any).__emitPlan(plan), {
      ...stale,
      state: "running",
    });
    await expect(page.locator(".job-item")).toHaveCount(0);
    await expect(page.locator(".nav-task-badge")).toHaveCount(0);
    await page.reload();
    // A reload of the mock starts with an empty database too; backend persistence is covered in Rust.
    await expect(page.locator(".nav-task-badge")).toHaveCount(0);
  });

test("matched subtitles are hidden from selection but included in the created task", async ({
  page,
}) => {
  await fixture(page, 2, true);
  await expect(page.locator(".file-table tbody tr")).toHaveCount(3);
  await expect(page.locator(".file-table")).not.toContainText("zh-CN.ass");
  await expect(page.locator(".file-table")).toContainText("unmatched.srt");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await expect(page.locator(".selection-bar")).toContainText("已选 3 项");
  await page.getByLabel("选择 unmatched.srt", { exact: true }).click();
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  await pick(page, page.getByLabel("数字位置", { exact: false }), "0");
  await page.getByRole("button", { name: "应用批量匹配" }).click();
  await page.getByRole("button", { name: "创建并查看" }).click();
  await expect(page.locator(".plan-table tbody tr")).toHaveCount(4);
  await expect(page.locator(".plan-summary")).toContainText("2字幕");
  const args = await page.evaluate(
    () =>
      (window as any).__calls.find((c: any) => c.command === "create_plan")
        .args,
  );
  expect(args.mappings).toHaveLength(2);
  expect(args.mappings.every((m: any) => m.source.endsWith(".mkv"))).toBe(true);
});

test("bulk cleanup uses red yellow green status colors and retains unfinished tasks", async ({
  page,
}) => {
  await fixture(page);
  await createTask(page);
  await page.evaluate(() => {
    const initial = (window as any).__plans[0];
    for (const [i, state] of [
      "complete",
      "cancelled",
      "failed",
      "paused",
    ].entries()) {
      const p = { ...initial, id: `extra-${i}`, state };
      (window as any).__plans.push(p);
      (window as any).__emitPlan(p);
    }
  });
  await expect(page.locator(".job-item")).toHaveCount(5);
  for (const [label, tone] of [
    ["已完成", "green"],
    ["已取消", "red"],
    ["需重试", "red"],
    ["已暂停", "amber"],
    ["待确认", "amber"],
  ]) {
    await expect(
      page.locator(".job-list [data-slot=badge]").filter({ hasText: label }),
    ).toHaveAttribute("data-tone", tone);
  }
  await page
    .getByRole("button", { name: "清理已结束任务", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toContainText("清理 2 个已结束任务");
  await expect(page.getByRole("dialog")).toContainText(
    "已完成 1 个，已取消 1 个",
  );
  await page.getByRole("button", { name: "确认永久删除" }).click();
  await expect(page.locator(".job-item")).toHaveCount(3);
  await expect(page.locator(".job-list")).not.toContainText("已完成");
  await expect(page.locator(".job-list")).not.toContainText("已取消");
  await expect(page.locator(".nav-task-badge")).toHaveText("3");
  await expect(
    page.getByRole("button", { name: "清理已结束任务", exact: true }),
  ).toBeDisabled();
  const padding = await page.locator(".job-list").evaluate((node) => {
    const last = node.querySelector(".job-item:last-child")!;
    return (
      node.getBoundingClientRect().bottom - last.getBoundingClientRect().bottom
    );
  });
  expect(padding).toBeLessThanOrEqual(14);
  await page.screenshot({
    path: "test-results/task-cleanup.png",
    fullPage: true,
  });
});

test("global default operation applies to new tasks and preserves existing tasks", async ({
  page,
}) => {
  await fixture(page);
  await createTask(page);
  await expect(
    page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 文件操作", { exact: true }),
  ).toContainText("复制");
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await pick(page, page.getByLabel("默认整理方式", { exact: true }), "move");
  await page.getByRole("button", { name: "保存全部设置" }).click();
  await page.getByRole("button", { name: "任务", exact: true }).click();
  await expect(
    page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 文件操作", { exact: true }),
  ).toContainText("复制");
  await page.getByRole("button", { name: "整理", exact: true }).click();
  await createTask(page);
  await expect(
    page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 文件操作", { exact: true }),
  ).toContainText("移动");
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByLabel("默认整理方式", { exact: true })).toContainText(
    "移动",
  );
});

test("name matching selects Hidden Signal despite its omitted flight description and different file numbering", async ({
  page,
}) => {
  await fixture(page, 1, false, "title-fallback");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  await expect(page.getByLabel("批量映射季号")).toHaveAttribute(
    "data-value",
    "5",
  );
  await pick(page, page.getByLabel("集数匹配方式"), "name");
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  const episode = page.getByLabel(
    "Fictional.Mystery.S05E07.Hidden.Signal.虚构航班182号.1985.mkv 对应集数",
    { exact: true },
  );
  await expect(episode).toHaveAttribute("data-value", "e1");
  await expect(episode).toContainText("S05E01");
  await expect(episode).toContainText("Hidden Signal (Fictional Flight 182)");
  await page.screenshot({
    path: "test-results/name-matching-title-fallback.png",
    fullPage: true,
  });
  await pick(page, page.getByLabel("切换创建任务操作"), "create");
  await page.getByRole("button", { name: "仅创建", exact: true }).click();
  const request = await page.evaluate(
    () =>
      (window as any).__calls.find((c: any) => c.command === "create_plan")
        ?.args,
  );
  expect(request.mappings).toHaveLength(1);
  expect(request.mappings[0].episode_ids).toEqual(["e1"]);
  expect(
    await page.evaluate(() =>
      (window as any).__calls.filter((c: any) => c.command === "run_plan"),
    ),
  ).toEqual([]);
});

test("work combobox integrates bilingual search, selection, empty results and clearing", async ({
  page,
}) => {
  await fixture(page, 2, false, "combobox");
  const work = page.getByRole("combobox", {
    name: "整理到本地条目",
    exact: true,
  });
  await expect(page.getByLabel("搜索本地条目", { exact: true })).toHaveCount(0);
  await work.click();
  const search = page.getByRole("combobox", {
    name: "搜索已入库作品",
    exact: true,
  });
  await expect(search).toBeFocused();
  await search.fill("FICTIONAL_VOYAGE");
  await expect(page.getByRole("option")).toHaveCount(2);
  await expect(
    page.getByRole("option").filter({ hasText: "2011" }),
  ).toBeVisible();
  await expect(
    page.getByRole("option").filter({ hasText: "2015" }),
  ).toBeVisible();
  await search.fill("架空の航海");
  await expect(page.getByRole("option")).toHaveCount(1);
  await search.press("Enter");
  await expect(work).toHaveAttribute("data-value", "w2");
  await expect(work).toBeFocused();
  await work.click();
  await expect(search).toHaveValue("");
  await search.fill("虚构远航 2011");
  await expect(page.getByRole("option")).toHaveCount(1);
  await search.dispatchEvent("keydown", {
    key: "Enter",
    code: "Enter",
    keyCode: 229,
    isComposing: true,
  });
  await expect(search).toBeVisible();
  await expect(work).toHaveAttribute("data-value", "w2");
  await search.press("Enter");
  await expect(work).toHaveAttribute("data-value", "w1");
  await work.click();
  await search.fill("没有这部作品");
  await expect(
    page.getByText("没有找到作品", { exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("option")).toHaveCount(0);
  await search.press("Enter");
  await expect(search).toBeVisible();
  await search.press("Escape");
  await expect(work).toHaveAttribute("data-value", "w1");
  await expect(work).toBeFocused();
  await page.setViewportSize({ width: 1050, height: 800 });
  await work.click();
  await expect(search).toHaveValue("");
  await readable(page);
  const popup = await page
    .getByRole("dialog", { name: "整理到本地条目选项", exact: true })
    .boundingBox();
  expect(popup!.x).toBeGreaterThanOrEqual(12);
  expect(popup!.x + popup!.width).toBeLessThanOrEqual(1038);
  await page.screenshot({
    path: "test-results/work-combobox-1050.png",
    fullPage: true,
  });
  await page.getByRole("option", { name: "清空作品选择", exact: true }).click();
  await expect(work).toHaveAttribute("data-value", "");
  await expect(
    page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 对应集数", { exact: true }),
  ).toBeDisabled();
  await page.getByRole("button", { name: "添加作品", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByLabel("搜索词")).toBeVisible();
});

test("episode combobox searches numbers and bilingual titles and supports searchable ranges", async ({
  page,
}) => {
  await fixture(page, 2, false, "combobox");
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  const episode = page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 对应集数", {
    exact: true,
  });
  const search = page.getByRole("combobox", { name: "搜索集数", exact: true });
  await episode.click();
  await search.fill("s2e12");
  await expect(page.getByRole("option")).toHaveCount(1);
  await expect(page.getByRole("option")).toContainText("S02E12");
  await search.press("Enter");
  await expect(episode).toHaveAttribute("data-value", "e12");
  await episode.click();
  await expect(search).toHaveValue("");
  await search.fill("旅の始まり 11");
  await expect(page.getByRole("option")).toHaveCount(1);
  await page.getByRole("option").click();
  await expect(episode).toHaveAttribute("data-value", "e11");
  await episode.click();
  await search.fill("不存在的集名");
  await expect(page.getByText("没有匹配的集数", { exact: true })).toBeVisible();
  await search.press("Escape");
  await expect(episode).toHaveAttribute("data-value", "e11");
  await episode.click();
  await search.press("End");
  const last = page.locator('[role="option"][data-item-value="e40"]');
  await expect(last).toHaveAttribute("aria-selected", "true");
  await expect(last).toBeInViewport();
  await search.fill("新旅程");
  await expect(page.getByRole("option")).toHaveCount(1);
  await expect(page.getByRole("option")).toContainText("原名：旅の始まり 1");
  await page.setViewportSize({ width: 1050, height: 700 });
  await readable(page);
  await page.screenshot({
    path: "test-results/episode-combobox-search-1050.png",
    fullPage: true,
  });
  await search.press("Enter");
  await expect(episode).toHaveAttribute("data-value", "e1");
  const range = page.getByLabel("[FICTIONAL_VOYAGE_2][01].mkv 多集结束位置", {
    exact: true,
  });
  await range.click();
  const rangeSearch = page.getByRole("combobox", {
    name: "搜索最后包含的集数",
    exact: true,
  });
  await rangeSearch.fill("S02E03");
  await expect(page.getByRole("option")).toHaveCount(1);
  await rangeSearch.press("Enter");
  await expect(range).toContainText("包含至 S02E03");
  expect(
    await page.evaluate(() =>
      Object.values(JSON.parse(localStorage.getItem("bangumi.mappings")!)).map(
        (row: any) => row.episode_ids,
      ),
    ),
  ).toEqual([["e1", "e2", "e3"]]);
  await pick(page, episode, "e1");
  await expect(range).toContainText("包含至 S02E03");
  await range.click();
  await rangeSearch.fill("单集");
  await rangeSearch.press("Enter");
  await expect(range).toContainText("单集文件");
  await expect(page.locator(".file-table tbody tr").first()).toHaveAttribute(
    "aria-selected",
    "false",
  );
});

test("task batch changes include hidden pages and attached subtitles without selection", async ({
  page,
}) => {
  await fixture(page, 96, true, "batch-all");
  await page.getByLabel("仅媒体", { exact: true }).check();
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  await pick(page, page.getByLabel("数字位置", { exact: false }), "0");
  await page.getByRole("button", { name: "应用批量匹配" }).click();
  await page.getByRole("button", { name: "创建并查看" }).click();
  await expect(page.locator(".job-batch-controls")).toContainText(
    "全部 98 个文件",
  );
  await expect(page.locator('.plan-table [role="checkbox"]')).toHaveCount(0);
  await page.getByLabel("仅看冲突与警告").check();
  await expect(page.locator(".plan-table tbody tr")).toHaveCount(0);
  await pick(page, page.getByLabel("批量文件操作"), "move");
  await page.getByRole("button", { name: "应用文件操作" }).click();
  await pick(page, page.getByLabel("批量冲突处理"), "skip");
  await page.getByRole("button", { name: "应用处理方式" }).click();
  await page.getByLabel("仅看冲突与警告").uncheck();
  await page.getByRole("button", { name: "下一页", exact: true }).click();
  await expect(
    page.getByLabel("[FICTIONAL_VOYAGE_2][096].mkv 文件操作", { exact: true }),
  ).toHaveAttribute("data-value", "move");
  await expect(
    page.getByLabel("[FICTIONAL_VOYAGE_2][096].mkv 冲突处理", { exact: true }),
  ).toHaveAttribute("data-value", "skip");
  await page.getByRole("button", { name: "确认整理", exact: true }).click();
  await page.getByRole("button", { name: "确认并开始", exact: true }).click();
  const rows = await page.evaluate(() => (window as any).__plans[0].rows);
  expect(rows).toHaveLength(98);
  expect(
    rows.every((r: any) => r.operation === "move" && r.action === "skip"),
  ).toBe(true);
  expect(rows.filter((r: any) => r.attachment)).toHaveLength(2);
});

test("artwork and directory cleanup notices do not mark completed tasks as failed", async ({
  page,
}) => {
  await fixture(page);
  await createTask(page);
  await page.evaluate(() => {
    const p = (window as any).__plans[0];
    p.state = "complete";
    p.rows.forEach((r: any) => {
      r.state = "complete";
    });
    p.metadata_notices = ["已有图片，已跳过：D:\\Library\\poster.jpg"];
    p.cleanup_notices = ["无法清理空源目录，已保留：D:\\Incoming：拒绝访问"];
    (window as any).__emitPlan(p);
  });
  await expect(
    page.locator('.job-list [data-slot="badge"]').filter({ hasText: "已完成" }),
  ).toHaveAttribute("data-tone", "green");
  await page
    .getByRole("button", { name: "元数据提示（1）", exact: true })
    .click();
  await expect(
    page.getByText("已有图片，已跳过：D:\\Library\\poster.jpg", {
      exact: true,
    }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "目录清理提示（1）", exact: true })
    .click();
  await expect(
    page.getByText("无法清理空源目录，已保留：D:\\Incoming：拒绝访问", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(page.getByText("元数据需重试", { exact: true })).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "重试元数据", exact: true }),
  ).toHaveCount(0);
});

test("split create button queues work in the background and pauses on error with blue running status", async ({
  page,
}) => {
  await fixture(page);
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  await pick(page, page.getByLabel("切换创建任务操作"), "execute");
  const footer = page.getByLabel(`当前版本 ${version}`);
  await expect(footer).toHaveText(`v${version}`);
  const footerBox = await footer.boundingBox();
  expect(footerBox!.y).toBeGreaterThan(page.viewportSize()!.height - 80);
  await page.screenshot({
    path: "test-results/source-actions-version.png",
    animations: "disabled",
  });
  await expect(page.locator(".source-plan-actions")).toContainText(
    "整理方式：复制",
  );
  for (const n of [1, 2]) {
    const filename = `[FICTIONAL_VOYAGE_2][0${n}].mkv`;
    await pick(
      page,
      page.getByLabel(`${filename} 对应集数`, { exact: true }),
      `e${n}`,
    );
    await page.getByLabel(`选择 ${filename}`, { exact: true }).check();
    await page.getByRole("button", { name: "创建并执行", exact: true }).click();
    await expect(page.locator(".source-panel")).toBeVisible();
    await expect(page.locator(".selection-bar")).toContainText("已选 0 项");
  }
  const plans = await page.evaluate(() => (window as any).__plans);
  expect(plans.map((p: any) => p.state)).toEqual(["running", "queued"]);
  await page
    .getByRole("navigation")
    .getByRole("button", { name: "任务", exact: true })
    .click();
  await expect(
    page.locator('.job-list [data-slot="badge"]').filter({ hasText: "处理中" }),
  ).toHaveAttribute("data-tone", "blue");
  await expect(
    page.locator('.job-list [data-slot="badge"]').filter({ hasText: "排队中" }),
  ).toHaveAttribute("data-tone", "amber");
  await page.evaluate(() => (window as any).__finishNext(true));
  await expect(
    page.locator('.floating-notice[data-tone="error"]').last(),
  ).toContainText("后续队列已暂停");
  expect(await page.evaluate(() => (window as any).__plans[1].state)).toBe(
    "queued",
  );
  await page.getByRole("button", { name: "继续队列", exact: true }).click();
  await expect(
    page.locator('.job-list [data-slot="badge"]').filter({ hasText: "处理中" }),
  ).toHaveAttribute("data-tone", "blue");
  await page.evaluate(() => (window as any).__finishNext());
  await expect(
    page.locator('.job-list [data-slot="badge"]').filter({ hasText: "已完成" }),
  ).toHaveAttribute("data-tone", "green");
  await expect(
    page.locator('.floating-notice[data-tone="success"]').last(),
  ).toContainText("整理完成");
  await page.screenshot({
    path: "test-results/queue-notifications.png",
    animations: "disabled",
    fullPage: true,
  });
});

test("create and execute preserves a reviewable draft when enqueue fails", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await pick(page, page.getByLabel("整理到本地条目"), "w1");
  await pick(page, page.getByLabel("数字位置", { exact: false }), "0");
  await page.getByRole("button", { name: "应用批量匹配" }).click();
  await pick(page, page.getByLabel("切换创建任务操作"), "execute");
  await page.evaluate(() => {
    (window as any).__failEnqueue = true;
  });
  await page.getByRole("button", { name: "创建并执行", exact: true }).click();
  await expect(
    page.locator('.floating-notice[data-tone="error"]'),
  ).toContainText("任务已创建，未加入执行队列");
  expect(await page.evaluate(() => (window as any).__plans.length)).toBe(1);
  expect(await page.evaluate(() => (window as any).__plans[0].state)).toBe(
    "draft",
  );
  await page.getByRole("button", { name: "检查任务", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "检查整理计划", exact: true }),
  ).toBeVisible();
});

test("catalog header categories and poster title footer tracks align at desktop widths", async ({
  page,
}) => {
  await fixture(page, 2, false, "catalog-layout");
  await page
    .getByRole("navigation", { name: "主导航" })
    .getByRole("button", { name: "剧集", exact: true })
    .click();
  await expect(
    page.locator(".topbar").getByRole("tablist", { name: "资料库类型" }),
  ).toBeVisible();
  await expect(page.locator("main").getByRole("tablist")).toHaveCount(0);
  for (const width of [1440, 1050]) {
    await page.setViewportSize({ width, height: 900 });
    const grid = await page.locator(".work-grid").evaluate((el) => ({
      columns: getComputedStyle(el).gridTemplateColumns.split(" ").length,
      rows: [...el.querySelectorAll(".work-card")].map((card) =>
        Object.fromEntries(
          [".work-cover", ".work-card-title", ".work-card-meta"].map(
            (selector) => {
              const box = card.querySelector(selector)!.getBoundingClientRect();
              return [
                selector,
                {
                  y: box.y,
                  bottom: box.bottom,
                  height: box.height,
                  width: box.width,
                },
              ];
            },
          ),
        ),
      ),
    }));
    expect(grid.rows.some((r) => r[".work-card-title"].height > 30)).toBe(true);
    for (let i = 0; i < grid.rows.length; i += grid.columns) {
      const row = grid.rows.slice(i, i + grid.columns);
      for (const item of row)
        for (const part of [
          ".work-cover",
          ".work-card-title",
          ".work-card-meta",
        ]) {
          expect(Math.abs(item[part].y - row[0][part].y)).toBeLessThan(1);
          if (part !== ".work-card-title")
            expect(
              Math.abs(item[part].bottom - row[0][part].bottom),
            ).toBeLessThan(1);
        }
    }
    await page.screenshot({
      path: `test-results/catalog-aligned-${width}.png`,
      fullPage: true,
    });
  }
  await page.getByRole("tab", { name: "电影", exact: true }).click();
  await page.locator(".work-card").filter({ hasText: "电影示例" }).click();
  await expect(page.getByRole("heading", { name: "电影信息" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "影片资源" })).toBeVisible();
  await expect(page.locator(".season-menu-item")).toHaveCount(0);
  await page.getByRole("tab", { name: "音乐", exact: true }).click();
  await expect(page.getByLabel("搜索已入库作品")).toBeVisible();
  await page.locator(".work-card").filter({ hasText: "音乐专辑" }).click();
  await page.locator('.season-menu-item[data-season="2"]').click();
  await expect(page.getByRole("heading", { name: "Disc 02" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "音轨与资源" })).toBeVisible();
});

test("season sidebar deduplicates completeness and preserves edits while changing sections", async ({
  page,
}) => {
  await fixture(page, 2, false, "catalog-seasons");
  await page
    .getByRole("navigation", { name: "主导航" })
    .getByRole("button", { name: "剧集", exact: true })
    .click();
  await page.locator(".work-card").click();
  await expect(page.getByRole("heading", { name: "剧集信息" })).toBeVisible();
  await expect(page.locator(".episode-table")).toHaveCount(0);
  await expect(page.locator(".season-menu-item")).toHaveCount(5);
  const second = page.locator('.season-menu-item[data-season="2"]');
  await expect(second.locator(".season-completeness")).toHaveText("2/3");
  await expect(
    page.locator('.season-menu-item[data-season="3"] .season-completeness'),
  ).toHaveText("0/0");
  await expect(
    page.locator('.season-menu-item[data-season="4"] .season-completeness'),
  ).toHaveText("0/1");
  await second.click();
  await expect(page.locator("[data-drop-episode]")).toHaveCount(4);
  const asset = page
    .locator(".episode-asset")
    .filter({ hasText: "D:\\Library\\S02E01-E02.mkv" })
    .first();
  await expect(asset).toContainText("123 B");
  await asset
    .getByRole("button", {
      name: "在文件管理器中打开：D:\\Library\\S02E01-E02.mkv",
      exact: true,
    })
    .click();
  await expect(
    page.getByRole("button", { name: "保存修改", exact: true }),
  ).toBeDisabled();
  const seasonName = page.getByLabel("第 2 季名称 · bangumi #100", {
    exact: true,
  });
  await seasonName.fill("新的可读季名");
  await expect(second).toContainText("新的可读季名");
  await page
    .getByRole("navigation", { name: "作品与季度" })
    .getByRole("button", { name: "剧集信息", exact: true })
    .click();
  await second.click();
  await expect(seasonName).toHaveValue("新的可读季名");
  await page.getByRole("button", { name: "保存修改", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "保存修改", exact: true }),
  ).toBeDisabled();
  await page.getByLabel("仅看尚未入库", { exact: true }).check();
  await expect(page.locator("[data-drop-episode]")).toHaveCount(1);
  await expect(page.locator('[data-drop-episode="e3"]')).toBeVisible();
  await page.getByLabel("仅看尚未入库", { exact: true }).uncheck();
  await page
    .getByRole("button", { name: "同步本季", exact: true })
    .nth(1)
    .click();
  expect(
    await page.evaluate(
      () =>
        (window as any).__calls
          .filter((c: any) => c.command === "sync_work")
          .at(-1).args,
    ),
  ).toMatchObject({ workId: "w1", bindingId: "tmdb", season: 2 });
  for (const width of [1440, 1050]) {
    await page.setViewportSize({ width, height: 800 });
    const menu = await page.locator(".work-detail-menu").boundingBox();
    const detail = await page.locator(".work-detail-content").boundingBox();
    expect(menu!.x + menu!.width).toBeLessThan(detail!.x);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: `test-results/season-menu-${width}.png`,
      fullPage: true,
    });
  }
  await page.locator('.season-menu-item[data-season="3"]').click();
  await expect(page.getByRole("heading", { name: "暂无集数" })).toBeVisible();
});

for (const width of [1050, 1440]) {
  test(`settings save remains reachable while editing at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 700 });
    await fixture(page);
    await page.getByRole("button", { name: "设置", exact: true }).click();
    const save = page.getByRole("button", { name: "保存全部设置", exact: true });
    await expect(save).toBeInViewport();
    await expect(page.locator(".settings-actions")).toContainText("所有设置已保存");
    await page.getByRole("button", {name: "分别指定可执行文件", exact: true}).click();
    const input = page.getByRole("textbox", { name: "FFprobe 可执行文件", exact: true });
    await input.fill("D:\\Tools\\ffprobe.exe");
    await expect(page.locator(".settings-actions")).toContainText("有未保存的修改");
    await expect(save).toBeInViewport();
    const toolbar = await page.locator(".settings-actions").boundingBox();
    const field = await input.boundingBox();
    expect(toolbar!.y).toBeGreaterThanOrEqual(0);
    expect(field!.y).toBeGreaterThanOrEqual(toolbar!.y + toolbar!.height);
    await save.click();
    await expect.poll(() => page.evaluate(() => (window as any).__calls.find((c: any) => c.command === "save_settings")?.args.settings.ffprobe)).toBe("D:\\Tools\\ffprobe.exe");
    await page.screenshot({ path: `test-results/settings-save-${width}.png` });
  });
}


test("stored extras support tile and list views, filtering and image preview", async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => {
    (window as any).__storedExtras = [
      { path: "D:\\Extras\\sample\\Book A\\page.jpg", relative_path: "Book A\\page.jpg", signature: {size: 1024, modified: 100} },
      { path: "D:\\Extras\\sample\\Book B\\page.jpg", relative_path: "Book B\\page.jpg", signature: {size: 2048, modified: 100} },
    ];
  });
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await page.getByRole("button", { name: "附加文件", exact: true }).click();
  const files = page.getByRole("region", { name: "已入库附加文件", exact: true });
  await expect(files.locator(".extra-file-tile")).toHaveCount(2);
  await files.getByRole("button", { name: "列表", exact: true }).click();
  await expect(files.locator("tbody tr")).toHaveCount(2);
  await files.getByRole("textbox", { name: "筛选文件路径" }).fill("Book B");
  await expect(files.locator("tbody tr")).toHaveCount(1);
  await files.getByRole("button", { name: "Book B\\page.jpg", exact: true }).click();
  await expect(page.getByRole("dialog")).toContainText("Book B\\page.jpg");
  await page.keyboard.press("Escape");
  await files.getByRole("button", { name: "平铺", exact: true }).click();
  await expect(files.locator(".extra-file-tile")).toHaveCount(1);
});


test("large extra import keeps operation and submit controls visible", async ({ page }) => {
  await page.setViewportSize({width: 1000, height: 650});
  await fixture(page);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await page.evaluate(() => {
    (window as any).__extraScan = { files: Array.from({length: 35}, (_, i) => ({path: `D:\\Incoming\\Bonus\\page-${i}.jpg`})), errors: [] };
  });
  await dropInto(page, page.getByRole("button", { name: "附加文件", exact: true }), ["D:\\Incoming\\Bonus"]);
  const dialog = page.getByRole("dialog", { name: "待提交文件", exact: true });
  const content = dialog.locator(".library-import-content");
  await expect(dialog.locator(".library-import-row")).toHaveCount(35);
  const submit = dialog.getByRole("button", {name: "提交并查看", exact: true});
  await expect(submit).toBeInViewport();
  await content.evaluate((e) => {e.scrollTop = e.scrollHeight});
  await expect(submit).toBeInViewport();
  await expect(dialog.getByRole("combobox", {name: "预期操作", exact: true})).toBeInViewport();
  await pick(page, dialog.getByRole("combobox", {name: "预期操作", exact: true}), "move");
  await page.screenshot({path: "test-results/import-cart-compact.png", animations: "disabled"});
  await submit.click();
  expect(await page.evaluate(() => (window as any).__plans[0].rows.every((r: any) => r.operation === "move"))).toBe(true);
});

test("long remote episode identifiers stay within their table column", async ({ page }) => {
  await page.setViewportSize({width:1000,height:700});
  await fixture(page);
  await page.getByRole("button",{name:"剧集",exact:true}).click();
  await page.getByRole("button",{name:/FICTIONAL VOYAGE.*2011/}).click();
  await page.locator('.season-menu-item[data-season="2"]').click();
  const source = page.locator('.episode-source').first();
  await source.evaluate(e => {e.textContent = "musicbrainz #00000000-0000-0000-0000-000000000000"});
  expect(await source.evaluate(e => e.scrollWidth <= e.clientWidth + 1)).toBe(true);
});


test("resource tabs share the title bar and navigation reuses the session list", async ({ page }) => {
  await fixture(page, 2, false, "resources");
  const listCalls = () => page.evaluate(() => (window as any).__calls.filter((c: any) => c.command === "list_resources").length);
  expect(await listCalls()).toBe(0);
  await page.getByRole("button", {name: "资源", exact: true}).click();
  await expect(page.locator(".resource-table tbody tr")).toHaveCount(2);
  await expect.poll(listCalls).toBe(1);
  await expect(page.locator(".topbar").getByRole("tab", {name: "媒体文件"})).toBeVisible();
  await expect(page.locator(".topbar").getByRole("tab", {name: "旧版本备份"})).toBeVisible();
  expect(await page.getByRole("tab", {name: "媒体文件"}).evaluate(e => getComputedStyle(e, "::after").height)).toBe("2px");
  await page.getByRole("tab", {name: "旧版本备份"}).click();
  await page.getByRole("button", {name: "剧集", exact: true}).click();
  await page.getByRole("button", {name: "资源", exact: true}).click();
  await expect(page.getByRole("tab", {name: "旧版本备份"})).toHaveAttribute("data-state", "active");
  expect(await listCalls()).toBe(1);
  await page.getByRole("button", {name: "刷新列表", exact: true}).click();
  await expect.poll(listCalls).toBe(2);
  await page.getByRole("tab", {name: "媒体文件"}).click();
  await page.setViewportSize({width: 1050, height: 800});
  await page.screenshot({path: "test-results/resources-header.png", fullPage: true});
  await page.reload();
  expect(await listCalls()).toBe(0);
  await page.getByRole("button", {name: "资源", exact: true}).click();
  await expect.poll(listCalls).toBe(1);
});


test("audio tools share a folder while file selection and opening remain distinct", async ({page}) => {
  await fixture(page);
  await page.getByRole("button", {name: "设置", exact: true}).click();
  const directory = page.getByRole("textbox", {name: "FFmpeg / FFprobe 目录", exact: true});
  await directory.pressSequentially("D:/Audio Tools/bin");
  await expect(directory).toHaveValue("D:/Audio Tools/bin");
  await page.getByRole("button", {name: "分别指定可执行文件", exact: true}).click();
  await expect(page.getByRole("textbox", {name: "FFmpeg 可执行文件", exact: true})).toHaveValue("D:/Audio Tools/bin/ffmpeg.exe");
  await expect(page.getByRole("textbox", {name: "FFprobe 可执行文件", exact: true})).toHaveValue("D:/Audio Tools/bin/ffprobe.exe");
  await expect(page.getByRole("button", {name: "选择文件", exact: true})).toHaveCount(2);
  await page.getByRole("button", {name: "保存全部设置", exact: true}).click();
  await expect.poll(() => page.evaluate(() => (window as any).__calls.find((c: any) => c.command === "save_settings")?.args.settings.ffmpeg)).toBe("D:/Audio Tools/bin/ffmpeg.exe");
  await directory.scrollIntoViewIfNeeded();
  await page.screenshot({path:"test-results/settings-audio-tools.png"});
});


for (const mode of ['create', 'view', 'execute']) {
  test(`partially matched selections require confirmation and preserve unmatched files (${mode})`, async ({page}) => {
    await fixture(page, 2);
    await pick(page, page.getByLabel("整理到本地条目"), "w1");
    const first = "[FICTIONAL_VOYAGE_2][01].mkv";
    const second = "[FICTIONAL_VOYAGE_2][02].mkv";
    await pick(page, page.getByLabel(`${first} 对应集数`, {exact: true}), "e1");
    await page.getByLabel(`选择 ${first}`, {exact: true}).check();
    await page.getByLabel(`选择 ${second}`, {exact: true}).check();
    await pick(page, page.getByLabel("切换创建任务操作"), mode);
    const action = page.locator('.create-split-button > button').first();
    await action.click();
    const dialog = page.getByRole('dialog', {name: '仅提交已匹配的项目？'});
    await expect(dialog).toContainText('1 项已匹配、1 项未匹配');
    if (mode === 'create') await page.screenshot({path:'test-results/partial-match-confirmation.png', animations:'disabled'});
    await dialog.getByRole('button', {name: '取消', exact: true}).click();
    expect(await page.evaluate(() => (window as any).__calls.filter((c:any) => c.command === 'create_plan').length)).toBe(0);
    await expect(page.getByLabel(`选择 ${second}`, {exact:true})).toBeChecked();
    await action.click();
    await dialog.getByRole('button', {name: '仅提交已匹配项目', exact:true}).click();
    await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c:any) => c.command === 'create_plan').length)).toBe(1);
    const mappings = await page.evaluate(() => (window as any).__calls.find((c:any) => c.command === 'create_plan').args.mappings);
    expect(mappings).toHaveLength(1);
    expect(mappings[0].source).toContain(first);
    if (mode === 'view') await page.getByRole('button', {name:'整理',exact:true}).click();
    await expect(page.getByLabel(`选择 ${second}`, {exact:true})).toBeVisible();
    if (mode !== 'view') await expect(page.getByLabel(`选择 ${second}`, {exact:true})).toBeChecked();
    await expect(page.getByLabel(`选择 ${first}`, {exact:true})).not.toBeChecked();
  });
}


test("settings persist the default CUE original handling", async ({page}) => {
  await fixture(page);
  await page.getByRole('button', {name:'设置',exact:true}).click();
  const field = page.getByRole('combobox', {name:'CUE 原文件默认处理方式'});
  await expect(field).toHaveText('保留原文件');
  await field.click();
  await page.getByRole('option', {name:'移动到备份',exact:true}).click();
  await page.getByRole('button', {name:'保存全部设置',exact:true}).click();
  await expect.poll(() => page.evaluate(() => (window as any).__calls.find((c:any) => c.command === 'save_settings')?.args.settings.cue_source_disposition)).toBe('backup');
  await page.getByRole('button', {name:'整理',exact:true}).click();
  await page.getByRole('button', {name:'设置',exact:true}).click();
  await expect(field).toHaveText('移动到备份');
});


test("extras can be staged with unsaved title edits and saved from the cart", async ({ page }) => {
  await fixture(page, 2, false, "catalog-seasons");
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.locator(".work-card").click();
  await page.locator('.season-menu-item[data-season="2"]').click();
  await page.getByLabel("第 2 季名称 · bangumi #100", { exact: true }).fill("新的可读季名");
  await page.evaluate(() => {
    (window as any).__extraScan = { files: [{ path: "D:\\Incoming\\Bonus\\cover.jpg" }], errors: [] };
  });
  await dropInto(page, page.getByRole("button", { name: "附加文件", exact: true }), ["D:\\Incoming\\Bonus"]);
  const dialog = page.getByRole("dialog", { name: "待提交文件", exact: true });
  await expect(dialog).toContainText("cover.jpg");
  await expect(dialog.getByRole("button", { name: "提交并查看", exact: true })).toBeDisabled();
  await dialog.getByRole("button", { name: "保存作品并更新预览", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "提交并查看", exact: true })).toBeEnabled();
  const saves = await page.evaluate(() => (window as any).__calls.filter((c: any) => c.command === "save_work"));
  expect(saves).toHaveLength(1);
  expect(JSON.stringify(saves[0])).toContain("新的可读季名");
  await dialog.getByRole("button", { name: "提交并查看", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__plans.length)).toBe(1);
});


test("extras can be staged while another task is running", async ({ page }) => {
  await fixture(page);
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await page.evaluate(() => {
    (window as any).__emitPlan({ id: "active-task", state: "running", rows: [], works: [], metadata_errors: [] });
    (window as any).__extraScan = { files: [{ path: "D:\\Incoming\\Bonus\\cover.jpg" }], errors: [] };
  });
  await dropInto(page, page.getByRole("button", { name: "附加文件", exact: true }), ["D:\\Incoming\\Bonus"]);
  const dialog = page.getByRole("dialog", { name: "待提交文件", exact: true });
  await expect(dialog).toContainText("cover.jpg");
  await expect(dialog.getByRole("button", { name: "提交并查看", exact: true })).toBeEnabled();
});
