import { expect, test } from '@playwright/test'
import { chooseWork, sourceFixture } from './source-fixture'

test('drag selection previews rows without probing or modifying episode mappings', async ({ page }) => {
  await sourceFixture(page, ['S01E01.mkv', 'S01E02.mkv', 'S01E03.mkv', 'clip.mkv'], 'D:\\Incoming', 3, { 'clip.mkv': 'Interview/clip.mkv' })
  await chooseWork(page, 'tv')
  const field = page.getByRole('combobox', { name: 'S01E01.mkv 对应集数', exact: true })
  await field.click()
  await page.locator('[role="option"][data-item-value="tv-1"]').click()
  const before = await page.evaluate(() => localStorage.getItem('bangumi.mappings'))
  await page.evaluate(() => { (window as any).__sourceCalls.length = 0 })
  const rows = page.locator('.file-table tbody tr')
  const start = (await rows.nth(0).locator('.file-name').boundingBox())!
  const end = (await rows.nth(2).locator('.file-name').boundingBox())!
  await page.mouse.move(start.x + 12, start.y + 8)
  await page.mouse.down()
  await page.mouse.move(end.x + 80, end.y + 8, { steps: 24 })
  await expect(page.locator('.selection-box')).toBeVisible()
  await expect(rows.nth(0)).toHaveAttribute('aria-selected', 'true')
  await expect(field).toContainText('S01E01')
  expect(await page.evaluate(() => localStorage.getItem('bangumi.mappings'))).toBe(before)
  expect(await page.evaluate(() => (window as any).__sourceCalls.filter((c: any) => ['inspect_source_nfos', 'probe_file'].includes(c.command)))).toEqual([])
  await page.mouse.up()
  await expect.poll(() => page.evaluate(() => (window as any).__sourceCalls.filter((c: any) => c.command === 'inspect_source_nfos').length)).toBe(1)
  expect(await page.evaluate(() => localStorage.getItem('bangumi.mappings'))).toBe(before)
  await expect(field).toContainText('S01E01')
})

test('cancelled drag does not inspect sources', async ({ page }) => {
  await sourceFixture(page, ['S01E01.mkv', 'S01E02.mkv'])
  const rows = page.locator('.file-table tbody tr')
  const start = (await rows.nth(0).locator('.file-name').boundingBox())!
  const end = (await rows.nth(1).locator('.file-name').boundingBox())!
  await page.mouse.move(start.x + 12, start.y + 8)
  await page.mouse.down()
  await page.mouse.move(end.x + 80, end.y + 8, { steps: 12 })
  await page.keyboard.press('Escape')
  await page.mouse.up()
  await expect(page.locator('.file-table tbody tr[aria-selected="true"]')).toHaveCount(0)
  expect(await page.evaluate(() => (window as any).__sourceCalls.filter((c: any) => c.command === 'inspect_source_nfos'))).toEqual([])
})
