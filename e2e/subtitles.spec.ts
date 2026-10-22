import { expect, test, type Page } from "@playwright/test";
import { chooseWork, sourceFixture } from "./source-fixture";

async function subtitlePlans(page: Page) {
  await page.evaluate(() => {
    const win = window as any;
    const invoke = win.__TAURI_INTERNALS__.invoke;
    let plan: any;
    win.__TAURI_INTERNALS__.invoke = async (
      command: string,
      args: any = {},
    ) => {
      if (command === "snapshot") {
        const state = await invoke(command, args);
        if (plan) state.plans = [structuredClone(plan)];
        return state;
      }
      if (command === "create_plan") {
        win.__sourceCalls.push({ command, args });
        const state = await invoke("snapshot");
        const work = state.works.find(
          (w: any) => w.id === args.mappings[0].work_id,
        );
        plan = {
          id: "subtitle-plan",
          kind: "organize",
          created_at: "2026-01-01T00:00:00Z",
          state: "draft",
          works: [work],
          tv_root: state.settings.tv_root,
          movie_root: state.settings.movie_root,
          extras_root: state.settings.extras_root,
          backup_root: state.settings.backup_root,
          verify_hash: false,
          metadata_errors: [],
          rows: args.mappings.map((m: any, i: number) => {
            const language =
              m.source.match(/(\.\w{2}(?:-\w+)?)\.\w+$/)?.[1] || "";
            const ext = m.source.split(".").at(-1);
            const video = `D:\\Library\\TV\\Fictional\\S01E0${i + 1}.mkv`;
            return {
              id: `sub-${i}`,
              source: m.source,
              target: video.replace(/\.mkv$/, `${language}.${ext}`),
              work_id: m.work_id,
              episode_ids: m.episode_ids,
              extra: false,
              attachment: true,
              parent_id: null,
              subtitle: {
                asset: { path: video },
                root: state.settings.tv_root,
              },
              signature: { size: 100, modified: 100 },
              target_signature: i === 0 ? { size: 50, modified: 1 } : null,
              probe: null,
              target_probe: null,
              conflicts: i === 0 ? ["目标文件已存在"] : [],
              warnings: [],
              reviewed: false,
              action: i === 0 ? "undecided" : "copy",
              operation: "copy",
              state: "pending",
              error: "",
              stage: "",
              digest: "",
              backups: [],
            };
          }),
        };
        return structuredClone(plan);
      }
      if (command === "save_decisions") {
        win.__sourceCalls.push({ command, args });
        for (const choice of args.choices)
          Object.assign(
            plan.rows.find((r: any) => r.id === choice.id),
            choice,
          );
        return structuredClone(plan);
      }
      if (command === "run_plan") {
        win.__sourceCalls.push({ command, args });
        plan.state = "queued";
        return structuredClone(plan);
      }
      return invoke(command, args);
    };
  });
}

test("standalone subtitles use batch matching and retain independent task decisions", async ({
  page,
}) => {
  const names = ["Fictional.E01.zh-Hans.ass", "Fictional.E02.en.srt"];
  await sourceFixture(page, names, "D:\\Incoming", 3);
  await subtitlePlans(page);
  await chooseWork(page, "tv");
  await page.getByRole("button", { name: "全选", exact: true }).click();
  await expect(
    page.getByText(
      "字幕按对应视频的实际文件名入库并保留语言后缀；请先完成视频入库。",
      { exact: true },
    ),
  ).toBeVisible();
  await page.getByRole("button", { name: "应用批量匹配", exact: true }).click();
  for (const [i, name] of names.entries()) {
    await expect(
      page.getByRole("combobox", { name: `${name} 对应集数`, exact: true }),
    ).toContainText(`S01E0${i + 1}`);
  }
  const summary = page.getByRole("region", {
    name: "季度匹配概况",
    exact: true,
  });
  await expect(summary).toContainText("按当前资料来源与待整理字幕统计");
  await expect(summary).toContainText("已匹配 2 集");
  await expect(summary).toContainText("未匹配 1 集：EP03");
  await page.screenshot({
    path: "test-results/subtitle-matching.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "创建并查看", exact: true }).click();
  await expect(page.getByText("检查整理计划", { exact: true })).toBeVisible();
  await expect(
    page.getByText("S01E01.zh-Hans.ass", { exact: false }),
  ).toBeVisible();
  const action = page.getByRole("combobox", {
    name: `${names[0]} 冲突处理`,
    exact: true,
  });
  await action.click();
  await page.locator('[role="option"][data-value="replace"]').click();
  const operation = page.getByRole("combobox", {
    name: `${names[0]} 文件操作`,
    exact: true,
  });
  await operation.click();
  await page.locator('[role="option"][data-value="move"]').click();
  await page.screenshot({
    path: "test-results/subtitle-plan.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "确认整理", exact: true }).click();
  await page.getByRole("button", { name: "确认并开始", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__sourceCalls
            .filter((c: any) => c.command === "save_decisions")
            .at(-1)?.args.choices,
      ),
    )
    .toMatchObject([
      { id: "sub-0", action: "replace", operation: "move" },
      { id: "sub-1", action: "copy", operation: "copy" },
    ]);
});

test("movie subtitles can be assigned manually and included in the created plan", async ({
  page,
}) => {
  const name = "Fictional Captions.ja.vtt";
  await sourceFixture(page, [name]);
  await subtitlePlans(page);
  await chooseWork(page, "movie");
  await page
    .getByRole("checkbox", { name: `选择 ${name}`, exact: true })
    .click();
  await page
    .getByRole("combobox", { name: `${name} 对应内容`, exact: true })
    .click();
  await page.locator('[role="option"][data-item-value="movie-1"]').click();
  await page.getByRole("button", { name: "创建并查看", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__sourceCalls.find(
            (c: any) => c.command === "create_plan",
          )?.args.mappings,
      ),
    )
    .toEqual([
      {
        source: `D:\\Incoming\\${name}`,
        work_id: "movie",
        part: 0,
        episode_ids: ["movie-1"],
        extra: false,
      },
    ]);
  await expect(
    page.getByRole("combobox", { name: `${name} 文件操作`, exact: true }),
  ).toBeVisible();
});


test("subtitle language patterns can be edited and invalid expressions are rejected", async ({ page }) => {
  await sourceFixture(page);
  await page.evaluate(() => {
    const win = window as any;
    const invoke = win.__TAURI_INTERNALS__.invoke;
    win.__TAURI_INTERNALS__.invoke = async (command: string, args: any = {}) => {
      if (command === "preview_naming") return {};
      if (command === "diagnostics_report") return { active: [], recent: [], statistics: [], directory: "D:\\Logs", write_error: "" };
      if (command === "save_settings") {
        win.__savedSubtitlePattern = args.settings.subtitle_language_pattern;
        return;
      }
      return invoke(command, args);
    };
  });
  await page.getByRole("button", { name: "设置", exact: true }).click();
  const input = page.getByRole("textbox", { name: "字幕语言正则", exact: true });
  await expect(input).toHaveValue("");
  await expect(page.locator("#subtitle-pattern-help")).toContainText("sc_xxxxxx → sc");
  await input.fill("(");
  await page.getByRole("button", { name: "保存全部设置", exact: true }).click();
  await expect(page.locator(".floating-notice").last()).toContainText("字幕语言正则无效");
  expect(await page.evaluate(() => (window as any).__savedSubtitlePattern)).toBeUndefined();
  await input.fill("([a-z]+)_[0-9]+");
  await page.getByRole("button", { name: "保存全部设置", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__savedSubtitlePattern)).toBe("([a-z]+)_[0-9]+");
});
