import {test,expect} from '../../apps/desktop/node_modules/@playwright/test/index.mjs';
import {install,entryPath} from './readability-fixture.mjs';

const titles=['Explorer','関連グラフ','概要グラフ','Source / 根拠','AI Export','Diagnostics'];
const tab=(page,title)=>page.getByRole('tab',{name:title,exact:true});
const menu=page=>page.getByRole('group',{name:'パネルの表示',exact:true});
const graph=page=>page.getByRole('region',{name:'関連グラフ',exact:true});
const source=page=>page.getByRole('region',{name:'ソースと根拠',exact:true});
const entry=page=>page.locator(`[data-path="${entryPath}"]`);

async function openMenu(page){await page.locator('.view-menu summary').click();await expect(menu(page)).toBeVisible();}
async function show(page,title){await openMenu(page);await menu(page).getByRole('button',{name:title,exact:true}).click();await expect(tab(page,title)).toHaveAttribute('aria-selected','true');}
async function close(page,title){await tab(page,title).click();await tab(page,title).locator('.dv-default-tab-action').click();await expect(tab(page,title)).toHaveCount(0);}
async function closeAll(page){for(const title of titles)await close(page,title);await expect(page.getByRole('tab')).toHaveCount(0);}
async function showAll(page){await openMenu(page);await menu(page).getByRole('button',{name:'すべてのパネルを表示',exact:true}).click();await expect(page.getByRole('tab')).toHaveCount(6);}
async function openProject(page){await page.getByRole('button',{name:'フォルダを開く',exact:true}).click();await expect(page.getByTestId('analysis-status')).toHaveText('ready');await expect(entry(page)).toBeVisible();}
async function checkIds(page){expect((await page.getByRole('tab').evaluateAll(tabs=>tabs.map(tab=>tab.dataset.tabPanelId))).sort()).toEqual(['diagnostics','explorer','export','graph','overview','source']);}
async function requests(page){return page.evaluate(()=>window.__readability.requests);}
async function prepare(page){await entry(page).getByRole('button').click();await entry(page).getByRole('checkbox').check();await page.getByRole('button',{name:'プレビューを作成',exact:true}).click();await expect(page.locator('.preview')).toContainText('// synthetic source');}
async function resizeExplorer(page,width){
  const pane=page.getByRole('region',{name:'ファイル一覧',exact:true}),bounds=await pane.boundingBox();
  const sashes=await page.locator('.dv-sash.dv-enabled').evaluateAll(elements=>elements.map(element=>{const b=element.getBoundingClientRect();return {x:b.x,y:b.y,width:b.width,height:b.height};}));
  const sash=sashes.find(b=>b.height>100&&Math.abs(b.x-bounds.x-bounds.width)<8);expect(sash).toBeTruthy();
  const x=sash.x+sash.width/2,y=sash.y+80;
  await page.mouse.move(x,y);await page.mouse.down();await page.mouse.move(x+width-bounds.width,y,{steps:5});await page.mouse.up();
  await expect.poll(()=>pane.evaluate(element=>element.clientWidth)).toBe(width);
}

test.beforeEach(async({page})=>{
  const failures=[];page.on('pageerror',error=>failures.push(error.message));
  await install(page);await page.goto('/');await expect(page.getByRole('button',{name:'フォルダを開く',exact:true})).toBeEnabled();
  page.__panelFailures=failures;
});
test.afterEach(async({page})=>{expect(page.__panelFailures).toEqual([]);});

test('T-26 each panel reopens repeatedly, reports presence and activates without duplicates',async({page})=>{
  const before=await requests(page);
  for(const title of titles){
    await openMenu(page);await expect(menu(page).getByRole('button',{name:title,exact:true})).toHaveAccessibleDescription('表示中');await page.keyboard.press('Escape');
    for(let repeat=0;repeat<2;repeat++){
      await close(page,title);await openMenu(page);
      await expect(menu(page).getByRole('button',{name:title,exact:true})).toHaveAccessibleDescription('閉じています');
      await menu(page).getByRole('button',{name:title,exact:true}).click();await expect(tab(page,title)).toHaveAttribute('aria-selected','true');
      await show(page,title);await checkIds(page);
    }
  }
  expect(await requests(page)).toEqual(before);
});

test('T-26 all closed, missing reference, moved layout and existing panels stay usable',async({page})=>{
  await closeAll(page);
  // Each component can be the sole panel, with no placement reference available.
  for(const title of titles){await show(page,title);await expect(page.getByRole('tab')).toHaveCount(1);await close(page,title);}
  await showAll(page);await checkIds(page);
  await expect.poll(()=>page.getByRole('region',{name:'ファイル一覧',exact:true}).evaluate(element=>element.clientWidth)).toBe(280);
  // With graph absent, Source falls back into the active Explorer group.
  await close(page,'Source / 根拠');await close(page,'関連グラフ');await show(page,'Explorer');await show(page,'Source / 根拠');
  expect(await tab(page,'Source / 根拠').getAttribute('aria-controls')).toBe(await tab(page,'Explorer').getAttribute('aria-controls'));
  await showAll(page);await checkIds(page);
  await show(page,'Explorer');await resizeExplorer(page,320);
  const controls=await tab(page,'Explorer').getAttribute('aria-controls');
  const target=page.getByRole('tablist').filter({has:tab(page,'Diagnostics')});
  await tab(page,'概要グラフ').dragTo(target);
  expect(await tab(page,'概要グラフ').getAttribute('aria-controls')).toBe(await tab(page,'Diagnostics').getAttribute('aria-controls'));
  await close(page,'Source / 根拠');await showAll(page);
  expect(await tab(page,'Explorer').getAttribute('aria-controls')).toBe(controls);
  expect(await tab(page,'概要グラフ').getAttribute('aria-controls')).toBe(await tab(page,'Diagnostics').getAttribute('aria-controls'));
  await show(page,'Explorer');await expect.poll(()=>page.getByRole('region',{name:'ファイル一覧',exact:true}).evaluate(element=>element.clientWidth)).toBe(320);
  // Showing all with nothing missing must leave active tabs and geometry alone.
  const active=await page.locator('[role="tab"][aria-selected="true"]').evaluateAll(tabs=>tabs.map(tab=>tab.dataset.tabPanelId).sort());
  await showAll(page);expect(await page.locator('[role="tab"][aria-selected="true"]').evaluateAll(tabs=>tabs.map(tab=>tab.dataset.tabPanelId).sort())).toEqual(active);
});

test('T-26 search, graph filters, selections and exact preview survive individual and all-panel recovery',async({page})=>{
  await openProject(page);await prepare(page);
  await page.getByPlaceholder('名前・相対パス').fill('入口');
  await graph(page).getByLabel('方向',{exact:true}).selectOption('out');
  await graph(page).getByLabel('種類',{exact:true}).selectOption('configuration');
  await graph(page).getByLabel('解決',{exact:true}).selectOption('confirmed');
  await graph(page).getByLabel('深度',{exact:true}).fill('3');
  await graph(page).getByLabel('深度',{exact:true}).press('Tab');
  await expect.poll(async()=> (await requests(page)).filter(r=>r.operation==='query'&&r.args.query.type==='neighbors').at(-1).args.query.depth).toBe(3);
  const preview=await page.locator('.preview').textContent(),before=await requests(page);
  for(const title of titles){await close(page,title);await show(page,title);}
  await closeAll(page);await showAll(page);
  expect(await requests(page)).toEqual(before);
  await show(page,'Explorer');await expect(page.getByPlaceholder('名前・相対パス')).toHaveValue('入口');await expect(entry(page).getByRole('checkbox')).toBeChecked();
  await show(page,'関連グラフ');await expect(graph(page).getByLabel('方向',{exact:true})).toHaveValue('out');await expect(graph(page).getByLabel('種類',{exact:true})).toHaveValue('configuration');await expect(graph(page).getByLabel('解決',{exact:true})).toHaveValue('confirmed');await expect(graph(page).getByLabel('深度',{exact:true})).toHaveValue('3');
  await expect(graph(page).locator('.relation-row')).toHaveCount(4);
  await show(page,'Source / 根拠');await expect(source(page).locator('.source-info')).toContainText(entryPath);
  await show(page,'AI Export');await expect(page.locator('.preview')).toHaveText(preview);await expect(page.getByRole('button',{name:'TXTを保存',exact:true})).toBeEnabled();
  await page.getByRole('button',{name:'TXTを保存',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'TXTを1件保存しました'})).toBeVisible();
});

test('T-26 menu works before project selection and during analysis without causing operations',async({page})=>{
  await openMenu(page);await page.keyboard.press('Escape');await expect(menu(page)).not.toBeVisible();await expect(page.locator('.view-menu summary')).toBeFocused();
  await openMenu(page);await page.locator('.brand').click();await expect(menu(page)).not.toBeVisible();
  await page.locator('.view-menu summary').focus();await page.keyboard.press('Enter');await page.keyboard.press('Tab');await expect(menu(page).getByRole('button',{name:'Explorer',exact:true})).toBeFocused();await page.keyboard.press('Enter');await expect(tab(page,'Explorer')).toHaveAttribute('aria-selected','true');
  const initial=await requests(page);await closeAll(page);await showAll(page);expect(await requests(page)).toEqual(initial);
  await page.evaluate(()=>window.__readability.holdAnalysis=true);
  await page.getByRole('button',{name:'フォルダを開く',exact:true}).click();await expect(page.getByTestId('analysis-status')).toHaveText('extract');
  const operations=before=>(before.filter(r=>r.operation!=='analysis_status'));
  const during=operations(await requests(page));await closeAll(page);await show(page,'Explorer');await showAll(page);
  expect(operations(await requests(page))).toEqual(during);await expect(page.getByRole('button',{name:'キャンセル',exact:true})).toBeVisible();
  await page.evaluate(()=>window.__readability.holdAnalysis=false);await expect(page.getByTestId('analysis-status')).toHaveText('ready');await expect(entry(page)).toBeVisible();
});

test('T-27 linked navigation restores Source and Explorer; current renderers remain operable',async({page})=>{
  await openProject(page);await entry(page).getByRole('button').click();
  await close(page,'Source / 根拠');await entry(page).getByRole('button').click();await expect(source(page).locator('.source-info')).toContainText(entryPath);
  await close(page,'Source / 根拠');await graph(page).locator('.relation-row').nth(1).click();
  await expect(source(page).locator('.evidence-list button')).toHaveCount(2);await expect(source(page).locator('.source-info')).toContainText('根拠 1行 1列');
  await source(page).locator('.evidence-list button').nth(1).click();
  await close(page,'Source / 根拠');await show(page,'Diagnostics');await page.locator('.diagnostic-list button').click();await expect(source(page).locator('.source-info')).toContainText(entryPath);
  await close(page,'Explorer');await show(page,'概要グラフ');await page.getByRole('region',{name:'概要グラフ',exact:true}).locator('.group-list button').first().click();
  await expect(page.getByRole('region',{name:'ファイル一覧',exact:true})).toBeVisible();await expect(page.locator('.file-row')).toHaveCount(4);
  await page.getByPlaceholder('名前・相対パス').fill('入口');await expect(page.locator('.file-row')).toHaveCount(1);
  await close(page,'関連グラフ');await show(page,'関連グラフ');await expect(graph(page).locator('.canvas')).toHaveAttribute('data-layout','ready');
  await graph(page).getByRole('button',{name:'縮小',exact:true}).click();await expect(graph(page).getByLabel('表示倍率')).toHaveText('80%');
  await graph(page).locator('.relation-row').first().click();await expect(source(page).locator('.source-info')).toContainText('根拠 1行 1列');
  await close(page,'Source / 根拠');await show(page,'Source / 根拠');await expect(source(page).locator('.source-info')).toContainText('根拠 1行 1列');
  await expect(source(page).locator('.monaco-editor')).toBeVisible();await expect(source(page).locator('.view-lines')).toContainText('// synthetic source');
  await page.setViewportSize({width:1280,height:820});
  await expect.poll(()=>graph(page).locator('.canvas').evaluate(element=>({width:element._cyreg.cy.width(),height:element._cyreg.cy.height()}))).toEqual(await graph(page).locator('.canvas').evaluate(element=>({width:element.clientWidth,height:element.clientHeight})));
  await expect.poll(()=>source(page).locator('.source-editor').evaluate(element=>({width:element.querySelector('.monaco-editor').clientWidth,height:element.querySelector('.monaco-editor').clientHeight}))).toEqual(await source(page).locator('.source-editor').evaluate(element=>({width:element.clientWidth,height:element.clientHeight})));
  await show(page,'AI Export');await entry(page).getByRole('checkbox').check();await page.getByRole('button',{name:'プレビューを作成',exact:true}).click();await expect(page.locator('.preview')).toContainText(entryPath);
  await show(page,'Diagnostics');await page.getByRole('button',{name:'エラーレポートを保存',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'エラーレポートを保存しました'})).toBeVisible();
});

test('T-28 scoped appearance samples after recovering all panels',async({page},testInfo)=>{
  await page.setViewportSize({width:1600,height:1000});await openProject(page);await prepare(page);await closeAll(page);await showAll(page);
  await show(page,'Source / 根拠');await expect(source(page).locator('.view-lines')).toContainText('// synthetic source');await show(page,'AI Export');await show(page,'関連グラフ');await openMenu(page);
  const summaryBounds=await page.locator('.view-menu summary').boundingBox(),menuBounds=await menu(page).boundingBox();
  const menuPath=testInfo.outputPath('panel-menu.png');await page.screenshot({path:menuPath,clip:{x:menuBounds.x,y:summaryBounds.y,width:menuBounds.width,height:menuBounds.y+menuBounds.height-summaryBounds.y}});await testInfo.attach('panel-menu',{path:menuPath,contentType:'image/png'});
  await page.keyboard.press('Escape');
  for(const [name,locator] of [['panels-restored',page.locator('.workspace')]]){
    const path=testInfo.outputPath(`${name}.png`);await locator.screenshot({path});await testInfo.attach(name,{path,contentType:'image/png'});
  }
  await show(page,'Diagnostics');
  const path=testInfo.outputPath('diagnostics-restored.png');await page.getByRole('region',{name:'解析診断',exact:true}).screenshot({path});await testInfo.attach('diagnostics-restored',{path,contentType:'image/png'});
  await testInfo.attach('visual-conditions',{body:JSON.stringify(await page.evaluate(()=>({viewport:{width:innerWidth,height:innerHeight},dpr:devicePixelRatio,userAgent:navigator.userAgent}))),contentType:'application/json'});
});
