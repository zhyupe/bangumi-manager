import { expect, test, type Page } from '@playwright/test'
import { sourceFixture } from './source-fixture'

async function fixture(page: Page, defaultDisposition = 'keep') {
  await sourceFixture(page, ['Album-A.cue', 'Album-B.cue', 'source.flac'], 'D:\\Incoming', 1, {}, {cue_source_disposition: defaultDisposition})
  await expect(page.getByRole('button', { name: /^(解析 CUE|Analyze CUE)$/, exact: true })).toHaveCount(0)
  await page.getByRole('checkbox', { name: /^(选择|Select) Album-A.cue$/ }).click()
  await page.getByRole('checkbox', { name: /^(选择|Select) Album-B.cue$/ }).click()
  await page.evaluate(async () => {
    const runtime = (window as any).__TAURI_INTERNALS__
    const original = runtime.invoke
    const state = await original('snapshot')
    state.sources.forEach((source: any) => { if (/\.cue$/i.test(source.path)) source.media = false })
    const sheet = { title: 'Fictional Album', performer: 'Example Ensemble', files: ['source.flac'], references: [{ name: 'source.flac', path: 'D:\\Incoming\\source.flac', error: null }], tracks: [1,2].map(number => ({ number, title: `Track ${number}`, performer: 'Example Ensemble', index01: (number - 1) * 4500 })), rem: {} }
    const tracks = [1,2].map(number => ({ number, title: `Track ${number}`, performer: 'Example Ensemble', duration: 60, extension: 'flac', segments: [{ source: 0, start: (number-1)*44100*60, end: number*44100*60 }] }))
    runtime.invoke = async (command: string, args: any = {}) => {
      if (command === 'snapshot') return structuredClone(state)
      if (command === 'inspect_cue') return sheet
      if (command === 'analyze_cue') return { id: 'analysis', sheet, tracks, warnings: [], sources: [{ file: { path: 'D:\\Incoming\\source.flac' }, codec: 'flac', rate: 44100, channels: 2, bits: 16 }] }
      if (command === 'cue_candidates') return { method: 'text', items: [], next_offset: null }
      if (command === 'preview_cue') {
        ;(window as any).__cueRequest = args.request
        return { id: 'cue-plan', kind: 'cue_split', state: 'draft', created_at: '', rows: tracks.map(t => ({ id: String(t.number), work_id: 'music', episode_ids: [`music-${t.number}`], source: 'D:\\Incoming\\source.flac', target: `D:\\Library\\Music\\${t.number}.flac`, action: 'copy', operation: 'split', warnings: [], conflicts: [], reviewed: false, target_signature: null, signature: { size: 100, modified: 100 }, state: 'pending', error: '', backups: [] })), works: [], metadata_errors: [], tv_root: '', movie_root: '', extras_root: '', backup_root: 'D:/Backups', verify_hash: true }
      }
      if (command === 'submit_cue') {
        ;(window as any).__cueSubmitted = args
        const plan = await runtime.invoke('preview_cue', { request: (window as any).__cueRequest })
        state.plans.push(plan)
        return plan
      }
      if (command === 'run_plan') { (window as any).__cueExecuted = args.id; return state.plans[0] }
      if (command === 'cancel_cue_analysis') return
      return original(command,args)
    }
  })
}

test('CUE selection happens in the sidebar and submitting closes the single-file dialog', async ({ page }) => {
  await fixture(page)
  await page.getByRole('button', { name: '解析 CUE', exact: true }).click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByText('D:\\Incoming\\source.flac', { exact: true })).toBeVisible()
  await expect(dialog.getByText('已匹配音源', { exact: true })).toBeVisible()
  await expect(dialog.getByText('已读取 CUE · 2 首音轨', { exact: true })).toBeVisible()
  await dialog.getByRole('button', { name: '解析 CUE', exact: true }).click()
  await expect(dialog.getByRole('button', { name: '解析 CUE', exact: true })).toHaveCount(0)
  await expect(dialog.getByRole('button', { name: '创建并查看', exact: true })).toHaveCount(0)
  await expect(dialog.locator('footer').getByRole('button', { name: '使用 CUE 建立本地专辑' })).toBeInViewport()
  await expect(dialog.getByRole('columnheader', {name: 'CUE 曲目', exact: true})).toBeVisible()
  await expect(dialog.getByText('实际时长 01:00', {exact: false})).toHaveCount(2)
  await expect(dialog.getByText('00:00–01:00', {exact: true})).toBeVisible()
  await dialog.getByRole('button', { name: '使用 CUE 建立本地专辑' }).click()
  await expect(dialog.getByRole('textbox', { name: '曲名 1', exact: true })).toHaveValue('Track 1')
  await dialog.getByRole('button', { name: '预览拆轨' }).click()
  await expect(dialog.getByText('D:\\Library\\Music\\1.flac', { exact: true })).toBeVisible()
  await expect(dialog.getByRole('button', { name: '解析 CUE', exact: true })).toHaveCount(0)
  await expect(dialog.getByRole('button', { name: '使用 CUE 建立本地专辑' })).toHaveCount(0)
  await dialog.getByRole('button', { name: '上一步', exact: true }).click()
  await expect(dialog.getByRole('textbox', {name: '曲名 1', exact: true})).toHaveValue('Track 1')
  await dialog.getByRole('button', {name: '预览拆轨', exact: true}).click()

  await dialog.getByRole('combobox', { name: '创建方式' }).click()
  await page.getByRole('option', { name: '创建并执行', exact: true }).click()
  await dialog.getByRole('button', { name: '创建并执行', exact: true }).click()
  await expect.poll(() => page.evaluate(() => (window as any).__cueExecuted)).toBe('cue-plan')
  await expect(dialog).toBeHidden()
  await page.getByRole('combobox', {name: 'CUE 文件', exact: true}).click()
  await page.getByRole('option', {name: 'Album-B.cue', exact: true}).click()
  await page.getByRole('button', {name: '解析 CUE', exact: true}).click()
  await expect(page.getByRole('dialog').getByText('D:\\Incoming\\Album-B.cue', {exact: true})).toBeVisible()
  await expect(page.getByRole('dialog').getByRole('combobox', {name: 'CUE 文件', exact: true})).toHaveCount(0)

})

test('CUE review is keyboard accessible at the minimum window width', async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 650 })
  await fixture(page)
  await page.getByRole('button', { name: '解析 CUE', exact: true }).click()
  const dialog=page.getByRole('dialog')
  await dialog.getByRole('button', { name: '解析 CUE', exact: true }).click()
  await expect(dialog.locator('footer').getByRole('button', { name: '使用 CUE 建立本地专辑' })).toBeInViewport()
  const columns = await dialog.evaluate(e => ({
    sidebar: e.querySelector('.cue-sidebar')!.getBoundingClientRect().left,
    review: e.querySelector('.cue-review')!.getBoundingClientRect().right,
  }))
  expect(columns.sidebar).toBeGreaterThanOrEqual(columns.review)
  await dialog.getByRole('button', { name: '使用 CUE 建立本地专辑' }).click()
  await expect(dialog.getByRole('textbox', { name: '曲名 2', exact: true })).toBeVisible()
  const size=await dialog.evaluate(e => ({ width:e.getBoundingClientRect().width, viewport:window.innerWidth, scroll:e.scrollWidth, client:e.clientWidth }))
  expect(size.width).toBeLessThan(size.viewport)
  expect(size.scroll).toBeLessThanOrEqual(size.client)
  const bounds=await dialog.evaluate(e => ({top:e.getBoundingClientRect().top,bottom:e.getBoundingClientRect().bottom,header:e.querySelector('header')!.getBoundingClientRect().top,footer:e.querySelector('footer')!.getBoundingClientRect().bottom}))
  expect(bounds.header).toBeGreaterThanOrEqual(bounds.top)
  expect(bounds.footer).toBeLessThanOrEqual(bounds.bottom)
  await page.screenshot({ path: 'test-results/cue-review.png', animations: 'disabled' })
  await page.keyboard.press('Escape')
  await expect(dialog).toBeHidden()
})

test('English CUE import supports create and view', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('bangumi.uiLanguage', 'en'))
  await fixture(page)
  await page.getByRole('button', { name: 'Analyze CUE', exact: true }).click()
  const dialog=page.getByRole('dialog')
  await dialog.getByRole('button', { name: 'Analyze CUE', exact: true }).click()
  await dialog.getByRole('button', { name: 'Create local album from CUE', exact: true }).click()
  await dialog.getByRole('button', { name: 'Preview split', exact: true }).click()
  await dialog.getByRole('button', { name: 'Create and view', exact: true }).click()
  await expect.poll(() => page.evaluate(() => (window as any).__cueSubmitted?.choices[0].operation)).toBe('split')
  await expect(dialog).toBeHidden()
})

test('different disc counts require manual CUE track mapping', async ({ page }) => {
  await fixture(page)
  await page.evaluate(() => {
    const runtime=(window as any).__TAURI_INTERNALS__, original=runtime.invoke
    const s={ provider:'musicbrainz',id:'example-release',kind:'music',title:'Example Release',info:{artist:'Example Ensemble'},episodes:[1,2,3].map(sort=>({id:`track${sort}`,season:2,sort,title:`Remote ${sort}`,info:{duration_ms:null}})),seasons:[{number:2,title:'Disc 2'}] }
    runtime.invoke=async(command:string,args:any)=> command==='cue_candidates' ? {method:'toc',items:[s],next_offset:null} : command==='media_detail' ? s : original(command,args)
  })
  await page.getByRole('button', { name: '解析 CUE', exact: true }).click()
  const dialog=page.getByRole('dialog')
  await dialog.getByRole('button', { name: '解析 CUE', exact: true }).click()
  await dialog.getByRole('button', { name: /Example Release/ }).click()
  await expect(dialog.getByRole('button', { name: '预览拆轨' })).toBeDisabled()
  for(const n of [1,2]) {
    await dialog.getByRole('combobox', { name: `数据库曲目 ${n}`, exact: true }).click()
    await page.getByRole('option', { name: new RegExp(`^${n}\\. Remote ${n}`) }).click()
  }
  await expect(dialog.getByRole('button', { name: '预览拆轨' })).toBeEnabled()
  await dialog.getByRole('button', { name: '预览拆轨' }).click()
  await expect.poll(() => page.evaluate(() => (window as any).__cueRequest.disc)).toBe(2)
})

test('CUE action follows the selection and stays in the matching sidebar', async ({ page }) => {
  await fixture(page)
  const sidebar = page.getByRole('complementary', { name: '匹配设置' })
  await expect(sidebar.getByRole('heading', { name: '匹配设置' })).toHaveCount(0)
  await expect(sidebar.getByRole('button', { name: '解析 CUE', exact: true })).toBeVisible()
  await page.getByRole('checkbox', { name: '选择 Album-A.cue', exact: true }).click()
  await page.getByRole('checkbox', { name: '选择 Album-B.cue', exact: true }).click()
  await expect(page.getByRole('button', { name: '解析 CUE', exact: true })).toHaveCount(0)
})

test('CUE passes selected audio and displays missing sources without allowing analysis', async ({ page }) => {
  await fixture(page)
  await page.getByRole('checkbox', { name: '选择 source.flac', exact: true }).click()
  await page.evaluate(() => {
    const runtime = (window as any).__TAURI_INTERNALS__, original = runtime.invoke
    runtime.invoke = async (command: string, args: any) => {
      if (command === 'plugin:dialog|open') {
        ;(window as any).__cueAudioPicker = args.options
        return null
      }
      if (command === 'inspect_cue') {
        ;(window as any).__cueInspection = args.request
        return { ...(await original(command, args)), references: [{name: 'source.flac', path: null, error: 'Audio missing'}] }
      }
      return original(command, args)
    }
  })
  await page.getByRole('button', { name: '解析 CUE', exact: true }).click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByText('未找到音源', { exact: true })).toBeVisible()
  await expect(dialog.getByRole('button', { name: '解析 CUE', exact: true })).toBeDisabled()
  await expect.poll(() => page.evaluate(() => (window as any).__cueInspection?.selected_paths)).toEqual(expect.arrayContaining(['D:\\Incoming\\source.flac']))
  await dialog.getByRole('button', {name:'选择音源',exact:true}).click()
  await expect.poll(() => page.evaluate(() => (window as any).__cueAudioPicker?.defaultPath)).toBe('D:\\Incoming\\')
  await expect(dialog.getByText('未找到音源', {exact:true})).toBeVisible()
})

test('CUE loading stays in the footer and long release names wrap within the sidebar', async ({ page }) => {
  await page.setViewportSize({width: 900, height: 650})
  await fixture(page)
  await page.evaluate(() => {
    const runtime = (window as any).__TAURI_INTERNALS__, original = runtime.invoke
    runtime.invoke = async (command: string, args: any) => {
      if (command === 'cue_candidates') return new Promise(resolve => {
        ;(window as any).__finishLookup = () => resolve({method:'text',items:[{id:'release',title:'FictionalAlbumTitle'.repeat(18),info:{artist:'ExampleArtist'.repeat(20)}}],next_offset:null})
      })
      return original(command,args)
    }
  })
  await page.getByRole('button',{name:'解析 CUE',exact:true}).click()
  const dialog = page.getByRole('dialog')
  await dialog.getByRole('button',{name:'解析 CUE',exact:true}).click()
  await expect(dialog.locator('footer').getByRole('status')).toHaveText('正在处理…')
  await expect(dialog.locator('main').getByText('正在处理…',{exact:true})).toHaveCount(0)
  await page.evaluate(() => (window as any).__finishLookup())
  const candidate = dialog.locator('.cue-candidate')
  await expect(candidate).toBeVisible()
  expect(await candidate.evaluate(e => e.scrollWidth <= e.clientWidth + 1)).toBe(true)
  expect(await dialog.locator('.cue-sidebar').evaluate(e => e.scrollWidth <= e.clientWidth + 1)).toBe(true)
  await page.screenshot({path:'test-results/cue-long-release.png',animations:'disabled'})
})


test('CUE preview distinguishes future outputs and backups from existing targets', async ({ page }) => {
  await fixture(page)
  await page.evaluate(() => {
    const runtime = (window as any).__TAURI_INTERNALS__, original = runtime.invoke
    runtime.invoke = async (command: string, args: any) => {
      const result = await original(command, args)
      if (command === 'preview_cue') {
        result.rows[1].target_signature = {size: 100, modified: 100}
        result.rows[1].action = 'replace'
        result.rows[1].backups = [{backup: 'D:/Backups/track.flac'}]
      }
      return result
    }
  })
  await page.getByRole('button', {name: '解析 CUE', exact: true}).click()
  const dialog = page.getByRole('dialog')
  await dialog.getByRole('button', {name: '解析 CUE', exact: true}).click()
  await dialog.getByRole('button', {name: '使用 CUE 建立本地专辑'}).click()
  await dialog.getByRole('button', {name: '预览拆轨'}).click()
  const rows = dialog.locator('.cue-output')
  await expect(rows.nth(0).getByText('待生成', {exact: true})).toBeVisible()
  await expect(rows.nth(0).locator('.open-path-button')).toHaveCount(0)
  await expect(rows.nth(0).getByText('1. Track 1', {exact: true})).toBeVisible()
  await expect(rows.nth(0).getByText('实际时长 01:00', {exact: true})).toBeVisible()
  await expect(rows.nth(1).getByText('已存在', {exact: true})).toBeVisible()
  await expect(rows.nth(1).locator('.open-path-button')).toHaveCount(1)
  await expect(rows.nth(1).getByText('计划备份位置', {exact: true})).toBeVisible()
  await expect(rows.nth(1).locator('.cue-output-backup .open-path-button')).toHaveCount(0)
  await page.screenshot({path: 'test-results/cue-output-paths.png', animations: 'disabled'})
})


test('CUE original handling defaults to keep and requires explicit confirmation for removal', async ({ page }) => {
  await fixture(page)
  await page.getByRole('button', {name: '解析 CUE', exact: true}).click()
  const dialog = page.getByRole('dialog')
  await dialog.getByRole('button', {name: '解析 CUE', exact: true}).click()
  await dialog.getByRole('button', {name: '使用 CUE 建立本地专辑'}).click()
  await dialog.getByRole('button', {name: '预览拆轨'}).click()
  const choice = dialog.getByRole('combobox', {name: '原始 CUE 与音频'})
  await expect(choice).toHaveText('保留原文件')
  await choice.click()
  await page.getByRole('option', {name: '删除原文件', exact: true}).click()
  await expect(dialog.getByRole('button', {name: '创建并查看', exact: true})).toBeDisabled()
  await dialog.getByRole('checkbox', {name: '确认删除列出的原文件，删除后不可恢复'}).click()
  await expect(dialog.getByRole('button', {name: '创建并查看', exact: true})).toBeEnabled()
  await choice.click()
  await page.getByRole('option', {name: '移动到备份', exact: true}).click()
  await expect(dialog.getByRole('button', {name: '创建并查看', exact: true})).toBeDisabled()
  await dialog.getByRole('checkbox', {name: '确认将列出的原文件移至备份目录'}).click()
  await page.screenshot({path:'test-results/cue-original-handling.png',animations:'disabled'})
  await dialog.getByRole('button', {name: '创建并查看', exact: true}).click()
  await expect.poll(() => page.evaluate(() => (window as any).__cueSubmitted?.sourceDisposition)).toBe('backup')
  await expect.poll(() => page.evaluate(() => (window as any).__cueSubmitted?.sourceDispositionConfirmed)).toBe(true)
})


test('CUE remembers the selected creation action across reopening and page reloads', async ({ page }) => {
  await fixture(page)
  const preview = async () => {
    await page.getByRole('button', {name: '解析 CUE', exact: true}).click()
    const dialog = page.getByRole('dialog')
    await dialog.getByRole('button', {name: '解析 CUE', exact: true}).click()
    await dialog.getByRole('button', {name: '使用 CUE 建立本地专辑'}).click()
    await dialog.getByRole('button', {name: '预览拆轨'}).click()
    return dialog
  }
  let dialog = await preview()
  await expect(dialog.getByRole('button', {name: '创建并查看', exact: true})).toBeVisible()
  await dialog.getByRole('combobox', {name: '创建方式'}).click()
  await page.getByRole('option', {name: '创建并执行', exact: true}).click()
  await dialog.getByRole('button', {name: '关闭', exact: true}).click()
  dialog = await preview()
  await expect(dialog.getByRole('button', {name: '创建并执行', exact: true})).toBeVisible()
  await fixture(page)
  dialog = await preview()
  await expect(dialog.getByRole('button', {name: '创建并执行', exact: true})).toBeVisible()
  await dialog.getByRole('button', {name: '创建并执行', exact: true}).click()
  await expect.poll(() => page.evaluate(() => (window as any).__cueExecuted)).toBe('cue-plan')
})


for (const [value, label] of [['delete', '删除原文件'], ['backup', '移动到备份']]) {
  test(`CUE uses the configured original handling default without preconfirming it (${value})`, async ({page}) => {
    await fixture(page, value)
    await page.getByRole('button', {name:'解析 CUE',exact:true}).click()
    const dialog = page.getByRole('dialog')
    await dialog.getByRole('button', {name:'解析 CUE',exact:true}).click()
    await dialog.getByRole('button', {name:'使用 CUE 建立本地专辑'}).click()
    await dialog.getByRole('button', {name:'预览拆轨'}).click()
    await expect(dialog.getByRole('combobox', {name:'原始 CUE 与音频'})).toHaveText(label)
    await expect(dialog.getByRole('button', {name:'创建并查看',exact:true})).toBeDisabled()
    await expect(dialog.locator('.cue-original-confirm').getByRole('checkbox')).not.toBeChecked()
    await dialog.getByRole('combobox', {name:'原始 CUE 与音频'}).click()
    await page.getByRole('option', {name:'保留原文件',exact:true}).click()
    await expect(dialog.getByRole('button', {name:'创建并查看',exact:true})).toBeEnabled()
  })
}
