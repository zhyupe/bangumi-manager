import { expect, test, type Page } from "@playwright/test";
import { sourceFixture } from "./source-fixture";

async function fixture(page: Page, mode = "ready") {
  await sourceFixture(page);
  await page.evaluate((mode) => {
    const state = window as any;
    state.__diagnosticsMode = mode;
    const event = (operation: string, index: number, status = "ok") => ({
      timestamp: "2026-10-06T17:32:15.273786100+00:00",
      trace_id: "request-00000000-0000-0000-0000-000000000001",
      span_id: `span-${index}`,
      parent_id: index === 0 ? "span-parent" : null,
      operation,
      status,
      elapsed_ms: index === 0 ? 1480 : 506.4,
      fields:
        index === 0
          ? {
              works: 1,
              folders: 2,
              resources: 12,
              path: `D:\\Library\\${"Fictional folder ".repeat(18)}`,
              nested: { phase: "collect" },
            }
          : {},
    });
    state.__timingReport = {
      directory: "D:\\AppData\\logs",
      session_id: "test-session",
      write_error: null,
      active: [
        event("task.preview", 100, "running"),
        event("library.scan_related", 101, "running"),
      ],
      recent: Array.from({ length: 25 }, (_, i) =>
        event(
          i === 0
            ? "library.scan_related"
            : i === 1
              ? "ui.action.create_plan"
              : i === 4
                ? `ipc.${"long_operation_segment_".repeat(12)}`
                : `ipc.operation_${i}`,
          i,
          i === 2 ? "error" : i === 3 ? "cancelled" : "ok",
        ),
      ),
      statistics: [
        {
          operation: "library.scan_related",
          count: 2,
          failures: 0,
          total_ms: 2000,
          max_ms: 1480,
          last_ms: 520,
        },
      ],
    };
    const original = state.__TAURI_INTERNALS__.invoke;
    state.__TAURI_INTERNALS__.invoke = async (
      command: string,
      args: any = {},
    ) => {
      if (command === "diagnostics_report") {
        if (state.__diagnosticsMode === "pending")
          return new Promise((resolve) => {
            state.__finishDiagnostics = () => resolve(state.__timingReport);
          });
        if (state.__diagnosticsMode === "error")
          throw new Error("模拟日志读取失败");
        return state.__timingReport;
      }
      if (command === "open_log_directory") {
        state.__openedLogs = true;
        return;
      }
      return original(command, args);
    };
  }, mode);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  return page.locator(".diagnostics-panel");
}

test("timings separate activity, aligned rows and expandable technical details", async ({
  page,
}) => {
  const panel = await fixture(page);
  await expect(
    panel.locator(".diagnostics-active .diagnostics-event"),
  ).toHaveCount(2);
  const recent = panel.locator(".diagnostics-section").last();
  await expect(recent.locator(".diagnostics-event")).toHaveCount(20);
  const row = recent.locator(".diagnostics-event").first();
  await expect(row.getByText("1.48 s", { exact: true })).toBeVisible();
  await row.getByRole("button").click();
  await expect(row.getByText("上级操作", { exact: true })).toBeVisible();
  await expect(row.locator("time")).not.toContainText("T17:32");
  await expect(row.locator(".diagnostics-fields")).toContainText("folders");
  const emptyFields = recent.locator(".diagnostics-event").nth(1);
  await emptyFields.getByRole("button").click();
  await expect(emptyFields.locator(".diagnostics-metadata")).toBeVisible();
  await expect(emptyFields.locator("pre")).toHaveCount(0);
  await panel.getByRole("button", { name: "操作统计 · 1 项" }).click();
  await expect(
    panel.getByRole("cell", { name: "1.00 s", exact: true }),
  ).toBeVisible();
  for (const width of [1440, 1050, 700]) {
    await page.setViewportSize({ width, height: 1000 });
    const issues = await panel.evaluate((el) => {
      const issues: string[] = [];
      for (const node of el.querySelectorAll<HTMLElement>(
        ".diagnostics-event-trigger, .diagnostics-detail, .diagnostics-location",
      )) {
        if (node.scrollWidth > node.clientWidth + 1)
          issues.push(`overflow:${node.className}`);
      }
      for (const node of el.querySelectorAll<HTMLElement>(
        "button, h2, h3, p, dt, dd, pre, span, code, th, td",
      )) {
        if (
          node.getClientRects().length &&
          Number.parseFloat(getComputedStyle(node).fontSize) < 12
        )
          issues.push(`font:${node.tagName}`);
      }
      const rows = [
        ...el.querySelectorAll<HTMLElement>(".diagnostics-event-trigger"),
      ];
      const rightEdges = rows.map(
        (node) =>
          node.querySelector(".diagnostics-duration")!.getBoundingClientRect()
            .right,
      );
      if (Math.max(...rightEdges) - Math.min(...rightEdges) > 1)
        issues.push("unaligned durations");
      return issues;
    });
    expect(issues).toEqual([]);
    await panel.screenshot({
      path: `test-results/diagnostics-layout-${width}.png`,
    });
  }
  await recent.getByRole("button", { name: "下一页" }).click();
  await expect(recent.locator(".diagnostics-event")).toHaveCount(5);
  await expect(
    recent.getByText("ipc.operation_24", { exact: true }),
  ).toBeVisible();
  await panel.getByRole("button", { name: "刷新耗时统计" }).click();
  await expect(recent.locator(".diagnostics-event")).toHaveCount(20);
  await panel.getByRole("button", { name: "打开日志目录" }).click();
  expect(await page.evaluate(() => (window as any).__openedLogs)).toBe(true);
  await expect(panel.locator(".open-path-button")).toHaveCount(0);
});

test("timing report loading, failures and empty results remain recoverable", async ({
  page,
}) => {
  const panel = await fixture(page, "pending");
  await expect(panel.getByRole("status")).toHaveText("正在读取耗时记录…");
  await expect(
    panel.getByRole("button", { name: "刷新耗时统计" }),
  ).toBeDisabled();
  await panel.getByRole("button", { name: "打开日志目录" }).click();
  expect(await page.evaluate(() => (window as any).__openedLogs)).toBe(true);
  await page.evaluate(() => (window as any).__finishDiagnostics());
  await expect(panel.getByRole("status")).toHaveCount(0);
  await page.evaluate(() => {
    (window as any).__diagnosticsMode = "error";
  });
  await panel.getByRole("button", { name: "刷新耗时统计" }).click();
  await expect(panel.getByRole("alert")).toHaveText("模拟日志读取失败");
  await expect(
    panel.getByRole("button", { name: "刷新耗时统计" }),
  ).toBeEnabled();
  await page.evaluate(() => {
    const state = window as any;
    state.__diagnosticsMode = "ready";
    state.__timingReport.active = [];
    state.__timingReport.recent = [];
    state.__timingReport.statistics = [];
  });
  await panel.getByRole("button", { name: "刷新耗时统计" }).click();
  await expect(panel.getByRole("alert")).toHaveCount(0);
  await expect(panel.getByText("当前没有进行中的操作")).toBeVisible();
  await expect(panel.getByText("暂无耗时记录")).toBeVisible();
});

test("timings retain readable statuses and details in English", async ({
  page,
}) => {
  const panel = await fixture(page);
  await page.getByRole("combobox", { name: "界面语言", exact: true }).click();
  await page.getByRole("option", { name: "English", exact: true }).click();
  await expect(
    panel.getByRole("heading", { name: "Timing and logs" }),
  ).toBeVisible();
  await expect(
    panel.getByRole("heading", { name: "Active operations 2" }),
  ).toBeVisible();
  const recent = panel.locator(".diagnostics-section").last();
  await recent
    .locator(".diagnostics-event")
    .first()
    .getByRole("button")
    .click();
  await expect(recent.getByText("Request ID", { exact: true })).toBeVisible();
  await page.setViewportSize({ width: 1050, height: 1000 });
  const clippedBadges = await panel
    .locator('[data-slot="badge"]')
    .evaluateAll((nodes) =>
      nodes
        .filter((node) => node.scrollWidth > node.clientWidth + 1)
        .map((node) => node.textContent),
    );
  expect(clippedBadges).toEqual([]);
  await panel.screenshot({ path: "test-results/diagnostics-english.png" });
});
