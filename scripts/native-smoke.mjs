// Use a new isolated desktop data directory and WebView2 CDP port 9223.
import { chromium } from "@playwright/test";
import { promises as fs } from "node:fs";
import path from "node:path";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";

const browser = await chromium.connectOverCDP("http://127.0.0.1:9223");
try {
  const page = browser.contexts()[0].pages()[0];
  await page.waitForFunction(() => !!window.__TAURI_INTERNALS__);
  const call = (command, args = {}) =>
    page.evaluate(
      ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
      { command, args },
    );
  const initial = await call("snapshot");
  const root = path.dirname(initial.data_dir);
  assert.ok(
    root.startsWith(path.join(process.cwd(), ".test-data", "native-smoke-")),
    "Only an isolated test data directory is allowed",
  );
  assert.equal(initial.works.length, 0, "Test database must be empty");
  const settings = {
    ...initial.settings,
    output_root: path.join(root, "Output"),
    tv_root: path.join(root, "TV Shows"),
    movie_root: path.join(root, "Movies"),
    extras_root: path.join(root, "Extras"),
    backup_root: path.join(root, "Backups"),
    verify_hash: true,
  };
  for (const folder of [
    settings.tv_root,
    settings.movie_root,
    settings.extras_root,
    settings.backup_root,
    path.join(root, "Incoming"),
  ])
    await fs.mkdir(folder, { recursive: true });
  await call("save_settings", { settings });
  const source = path.join(root, "Incoming", "[FICTIONAL_VOYAGE_2][01].mkv");
  execFileSync(
    path.join(path.dirname(settings.ffprobe), "ffmpeg.exe"),
    [
      "-hide_banner",
      "-loglevel",
      "error",
      "-f",
      "lavfi",
      "-i",
      "color=c=green:s=320x240:d=1",
      "-c:v",
      "libx264",
      "-y",
      source,
    ],
    { windowsHide: true },
  );
  await fs.writeFile(
    source.replace(/\.mkv$/, ".ja.srt"),
    "1\n00:00:00,000 --> 00:00:00,500\n字幕\n",
  );
  const scan = await call("scan_files", { paths: [path.dirname(source)] });
  assert.equal(scan.files.length, 2);
  const probe = await call("probe_file", { path: source });
  assert.equal(probe.width, 320);
  assert.equal(probe.height, 240);
  assert.equal(probe.error, null);
  const work = await call("bind_subject", {
    request: {
      work_id: null,
      subject: {
        info: {
          language: "zh-CN",
          original_language: "ja",
          original_title: "FICTIONAL VOYAGE",
          localized_title: "虚构远航",
          genres: [16],
          countries: ["JP"],
        },
        provider: "bangumi",
        id: "900000001",
        kind: "tv",
        title: "FICTIONAL VOYAGE",
        original_title: "FICTIONAL VOYAGE",
        year: "2011",
        overview: "Synthetic native smoke fixture",
        poster: "",
        episodes: [
          {
            info: {
              original_title: "始まり",
              language: "zh-CN",
              original_language: "ja",
            },
            id: "900000002",
            sort: 1,
            title: "测试章节",
            kind: 0,
            season: 1,
          },
        ],
        relations: [],
        cached: false,
      },
      title: "FICTIONAL VOYAGE",
      kind: "tv",
      season: 2,
      tmdb_id: "",
      parent_id: "",
    },
  });
  const poster = path.join(initial.data_dir, "artwork", "native-poster.jpg");
  execFileSync(
    path.join(path.dirname(settings.ffprobe), "ffmpeg.exe"),
    [
      "-hide_banner",
      "-loglevel",
      "error",
      "-f",
      "lavfi",
      "-i",
      "color=c=pink:s=200x300",
      "-frames:v",
      "1",
      "-y",
      poster,
    ],
    { windowsHide: true },
  );
  work.poster = "https://example.invalid/poster.jpg";
  work.info.local_poster = poster;
  await call("save_work", { work });
  let plan = await call("create_plan", {
    mappings: [
      {
        source,
        work_id: work.id,
        episode_ids: [work.episodes[0].id],
        extra: false,
      },
    ],
  });
  assert.equal(plan.rows.length, 2);
  const video = plan.rows.find((row) => !row.attachment);
  assert.ok(
    video.target.endsWith(
      path.join(
        "电视剧",
        "日番",
        "FICTIONAL VOYAGE (2011)",
        "Season 02",
        "FICTIONAL VOYAGE - S02E01 - 测试章节.mkv",
      ),
    ),
  );
  await assert.rejects(
    fs.stat(video.target),
    /ENOENT/,
    "Preview must not copy files",
  );
  await call("run_plan", { id: plan.id });
  for (let i = 0; i < 100; i++) {
    plan = (await call("snapshot")).plans.find((p) => p.id === plan.id);
    if (["complete", "failed", "metadata_failed"].includes(plan.state)) break;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  assert.equal(plan.state, "complete", JSON.stringify(plan));
  assert.deepEqual(await fs.readFile(source), await fs.readFile(video.target));
  await fs.stat(video.target.replace(/\.mkv$/, ".ja.srt"));
  await fs.stat(path.join(video.media_folder, "tvshow.nfo"));
  assert.ok(
    (
      await fs.readFile(video.target.replace(/\.mkv$/, ".nfo"), "utf8")
    ).includes("始まり"),
  );
  assert.deepEqual(
    await fs.readFile(poster),
    await fs.readFile(path.join(video.media_folder, "poster.jpg")),
  );
  const snapshot = await call("snapshot");
  assert.equal(snapshot.index.assets.length, 1);
  const report = path.join(root, "plan.json");
  await call("export_plan", { id: plan.id, path: report });
  assert.equal(
    JSON.parse(await fs.readFile(report, "utf8")).rows.find(
      (r) => !r.attachment,
    ).probe.width,
    320,
  );
  await page.reload();
  await page.getByRole("button", { name: "剧集", exact: true }).click();
  await page.getByRole("button", { name: /FICTIONAL VOYAGE.*2011/ }).click();
  await page.waitForFunction(
    () => document.querySelector(".work-poster")?.naturalWidth > 0,
  );
  await page.screenshot({
    path: path.join(root, "native-catalog.png"),
    fullPage: true,
  });
  console.log(
    JSON.stringify({
      nativeCopy: "passed",
      ffprobe: "passed",
      sqlite: "passed",
      assets: snapshot.index.assets.length,
      root,
    }),
  );
  if (process.argv.includes("--live"))
    try {
      const query = process.env.BANGUMI_TEST_QUERY;
      const subjectId = process.env.BANGUMI_TEST_SUBJECT_ID;
      assert.ok(
        query && subjectId,
        "BANGUMI_TEST_QUERY and BANGUMI_TEST_SUBJECT_ID are required for live requests",
      );
      const results = await call("search_media", {
        provider: "bangumi",
        query,
        kind: "tv",
      });
      const match = results.find((r) => r.id === subjectId);
      assert.ok(match, "Bangumi search should return the configured subject");
      const detail = await call("media_detail", {
        provider: "bangumi",
        id: match.id,
        kind: "tv",
        refresh: false,
      });
      assert.ok(detail.episodes.length > 0);
      console.log(
        JSON.stringify({
          liveBangumi: "passed",
          episodes: detail.episodes.length,
          relations: detail.relations.length,
        }),
      );
    } catch (error) {
      console.log(
        JSON.stringify({ liveBangumi: "unavailable", error: String(error) }),
      );
      process.exitCode = 2;
    }
} finally {
  await browser.close();
}
