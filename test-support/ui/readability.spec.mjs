import {test,expect} from '../../apps/desktop/node_modules/@playwright/test/index.mjs';
import {install,entryPath,laterPath,duplicatePaths} from './readability-fixture.mjs';
const explorer=page=>page.getByRole('region',{name:'ファイル一覧'});
const graphPane=page=>page.locator('section[aria-label="関連グラフ"]');
const row=(page,path)=>page.locator(`[data-path="${path}"]`);
async function resizeExplorer(page,width){
  const bounds=await explorer(page).boundingBox();
  const sashes=await page.locator('.dv-sash.dv-enabled').evaluateAll(elements=>elements.map(element=>{const box=element.getBoundingClientRect();return {x:box.x,y:box.y,width:box.width,height:box.height};}));
  const sash=sashes.find(box=>box.height>100&&Math.abs(box.x-bounds.x-bounds.width)<8);
  expect(sash).toBeTruthy();
  const start=sash.x+sash.width/2,y=sash.y+80;
  await page.mouse.move(start,y);await page.mouse.down();await page.mouse.move(start+width-bounds.width,y,{steps:5});await page.mouse.up();
  await expect.poll(()=>explorer(page).evaluate(element=>element.clientWidth)).toBe(width);
}
async function graphData(page){return graphPane(page).locator('.canvas').evaluate(element=>{const graph=element._cyreg.cy;return {zoom:graph.zoom(),nodes:graph.nodes().map(node=>({id:node.id(),label:node.data('label'),path:node.data('path'),box:node.boundingBox({includeLabels:true,includeOverlays:false}),font:node.style('font-size'),position:node.renderedPosition()})),edges:graph.edges().map(edge=>({id:edge.id(),label:edge.style('label'),status:edge.data('status'),line:edge.style('line-style')}))};});}
test.beforeEach(async({page})=>{await install(page);await page.goto('/');await page.getByRole('button',{name:'フォルダを開く',exact:true}).click();await expect(page.getByTestId('analysis-status')).toHaveText('ready');await expect(row(page,entryPath)).toBeVisible();});

test('T-23 Explorer hierarchy, independent selection, search and folder state',async({page})=>{
  await expect(page.locator('[data-folder=""]')).toContainText('ReadableProject');
  await expect(row(page,'Root.java')).toHaveAttribute('aria-level','2');
  const folder=page.locator('[data-folder="modules/長い共通フォルダ/業務/画面"]');
  await expect(folder.getByRole('button')).toHaveText('⌄画面');
  await folder.getByRole('button').click();await expect(row(page,entryPath)).toHaveCount(0);
  expect(await page.evaluate(()=>window.__readability.requests.filter(r=>r.operation==='selection_update'))).toHaveLength(0);
  await page.getByPlaceholder('名前・相対パス').fill('入口');await expect(row(page,entryPath)).toBeVisible();
  await page.getByPlaceholder('名前・相対パス').fill('');await expect(row(page,entryPath)).toHaveCount(0);
  await folder.getByRole('button').click();await row(page,entryPath).getByRole('button').click();
  await expect(page.locator('.source-info')).toContainText(entryPath);
  await expect(graphPane(page).locator('.relation-row')).toHaveCount(15);
  await expect(row(page,entryPath).getByRole('checkbox')).not.toBeChecked();
  await row(page,entryPath).getByRole('checkbox').check();await expect(row(page,entryPath).getByRole('checkbox')).toBeChecked();
  await row(page,entryPath).getByRole('checkbox').uncheck();await expect(row(page,entryPath).getByRole('checkbox')).not.toBeChecked();
  await page.getByPlaceholder('名前・相対パス').fill('請求/common');await expect(page.locator('.file-row')).toHaveCount(1);
  await expect(row(page,duplicatePaths[1]).getByRole('button')).toHaveAttribute('title',duplicatePaths[1]);
  await page.getByRole('button',{name:'閉じる',exact:true}).click();await expect(page.locator('.file-row')).toHaveCount(0);await expect(explorer(page).getByRole('button',{name:'さらに表示',exact:true})).toHaveCount(0);
});

test('T-23 Explorer paging merges existing folders without duplicate or lost files',async({page})=>{
  await expect(page.locator('.file-row')).toHaveCount(100);
  await expect(row(page,laterPath)).toHaveCount(0);
  await page.evaluate(()=>window.__readability.holdPage=true);
  const more=explorer(page).getByRole('button',{name:'さらに表示',exact:true});await more.click();await expect(more).toBeDisabled();
  await expect(row(page,laterPath)).toBeVisible();await expect(page.locator('.file-row')).toHaveCount(108);
  await expect(page.locator('[data-folder="modules/長い共通フォルダ/業務/画面"]')).toHaveCount(1);
  await expect(more).toHaveCount(0);
  const paths=await page.locator('.file-row').evaluateAll(rows=>rows.map(row=>row.dataset.path));expect(new Set(paths).size).toBe(108);
  expect(await page.evaluate(()=>window.__readability.requests.filter(r=>r.operation==='query'&&r.args.query.type==='resources'&&r.args.cursor))).toHaveLength(1);
});

test('T-23 Explorer narrow layout, local icons and exclusions',async({page})=>{
  const requests=[];page.on('request',request=>requests.push(request.url()));
  await resizeExplorer(page,220);
  const field=page.getByPlaceholder('名前・相対パス');await field.fill('web/');
  for(const path of ['web/断片/header.jspf','web/画面/日本語.xhtml','web/js/main.js'])await expect(row(page,path).getByRole('img')).toBeVisible();
  await field.fill('');
  for(const kind of ['java','jsp','jspf','xhtml','js','xml','properties','file'])await expect(explorer(page).locator(`[data-icon="${kind}"]`).first()).toBeVisible();
  await expect(explorer(page).locator('[data-icon="folder-open"]').first()).toBeVisible();
  const excluded=explorer(page).locator('.exclusions');await expect(excluded).not.toHaveAttribute('open','');
  await excluded.getByText('適用中の除外・読めない資源').click();await expect(excluded.locator('li')).toHaveCount(50);
  const geometry=await explorer(page).evaluate(element=>{const input=element.querySelector('.search-field input'),label=element.querySelector('.search-field span');return {width:element.clientWidth,scrollWidth:element.scrollWidth,labelHeight:label.getBoundingClientRect().height,inputWidth:input.getBoundingClientRect().width};});
  expect(geometry.scrollWidth).toBeLessThanOrEqual(geometry.width);expect(geometry.labelHeight).toBeLessThan(22);expect(geometry.inputWidth).toBeGreaterThan(100);
  expect(requests.filter(url=>/^https?:/.test(url)&&!url.startsWith('http://127.0.0.1:1420'))).toEqual([]);
  await row(page,'Root.java').getByRole('button').click();await row(page,'Root.java').getByRole('checkbox').check();
  await page.locator('[data-folder="config"]').getByRole('button').click();await expect(row(page,'config/app.properties')).toHaveCount(0);
  await resizeExplorer(page,320);await page.locator('[data-folder="config"]').getByRole('button').click();await expect(row(page,'config/app.properties')).toBeVisible();
});

test('T-24 graph compact unique labels, no overlap and explicit zoom controls',async({page})=>{
  await row(page,entryPath).getByRole('button').click();const pane=graphPane(page);
  await expect(pane.locator('.canvas')).toHaveAttribute('data-layout','ready');
  const data=await graphData(page);expect(data.nodes).toHaveLength(16);expect(data.zoom).toBe(1);
  const viewport=await pane.locator('.canvas').boundingBox();
  for(const node of data.nodes.slice(0,2)){expect(node.position.x-107).toBeGreaterThanOrEqual(0);expect(node.position.x+107).toBeLessThanOrEqual(viewport.width);expect(node.position.y-32).toBeGreaterThanOrEqual(0);expect(node.position.y+32).toBeLessThanOrEqual(viewport.height);}
  expect(new Set(data.nodes.map(node=>node.label)).size).toBe(data.nodes.length);
  const duplicate=data.nodes.filter(node=>node.path.endsWith('/Service.java'));expect(duplicate.map(node=>node.label)).toEqual(['Service.java\n顧客/common','Service.java\n請求/common']);
  for(const node of data.nodes){expect(node.font).toBe('14px');for(const other of data.nodes.filter(other=>other.id!==node.id)){const overlap=node.box.x1<other.box.x2&&node.box.x2>other.box.x1&&node.box.y1<other.box.y2&&node.box.y2>other.box.y1;expect(overlap).toBe(false);}}
  expect(data.edges.every(edge=>edge.label==='')).toBe(true);
  expect(data.edges.find(edge=>edge.status==='candidate').line).toBe('dashed');expect(data.edges.find(edge=>edge.status==='unresolved').line).toBe('dotted');
  await pane.getByRole('button',{name:'縮小',exact:true}).click();await expect(pane.getByLabel('表示倍率')).toHaveText('80%');
  await page.setViewportSize({width:1300,height:850});await expect(pane.getByLabel('表示倍率')).toHaveText('80%');
  await pane.getByRole('button',{name:'拡大',exact:true}).click();await expect(pane.getByLabel('表示倍率')).toHaveText('100%');
  await pane.getByRole('button',{name:'全体表示',exact:true}).click();expect((await graphData(page)).zoom).toBeLessThan(1);
  await pane.getByRole('button',{name:'起点へ',exact:true}).click();await expect(pane.getByLabel('表示倍率')).toHaveText('100%');
  await pane.locator('.canvas').evaluate(element=>element._cyreg.cy.getElementById('file-2').emit('mouseover'));
  await expect(pane.locator('.graph-inspection')).toHaveText(duplicatePaths[0]);
});

test('T-24 graph evidence, focus, manual export and overview survive resize and filters',async({page})=>{
  await row(page,entryPath).getByRole('button').click();const pane=graphPane(page);
  await pane.getByRole('button',{name:'出力対象に追加',exact:true}).click();await expect(row(page,entryPath).getByRole('checkbox')).toBeChecked();
  await pane.locator('.canvas').evaluate(element=>element._cyreg.cy.getElementById('edge-relation-1-file-3').emit('tap'));
  await expect(page.locator('.evidence-list')).toContainText('candidate');await expect(page.locator('.evidence-list button')).toHaveCount(2);
  await page.locator('.evidence-list button').nth(1).click();await expect(page.locator('.source-info')).toContainText('根拠 1行 1列');
  await pane.getByLabel('種類',{exact:true}).selectOption('configuration');await expect(pane.locator('.relation-row')).toHaveCount(4);
  await pane.getByLabel('種類',{exact:true}).selectOption('');await pane.getByLabel('解決',{exact:true}).selectOption('unresolved');await expect(pane.locator('.relation-row')).toHaveCount(1);
  await page.setViewportSize({width:1100,height:760});await pane.getByRole('button',{name:'全体表示',exact:true}).click();
  await pane.getByLabel('解決',{exact:true}).selectOption('');await pane.locator('.canvas').evaluate(element=>element._cyreg.cy.getElementById('file-2').emit('tap'));
  await expect(page.locator('.source-info')).toContainText(duplicatePaths[0]);await expect(row(page,duplicatePaths[0]).getByRole('checkbox')).not.toBeChecked();
  await page.getByText('概要グラフ',{exact:true}).click();const overview=page.getByRole('region',{name:'概要グラフ',exact:true});await overview.getByRole('button',{name:'全体表示',exact:true}).click();
  await overview.locator('.canvas').evaluate(element=>element._cyreg.cy.nodes().first().emit('tap'));await expect(page.locator('.file-row')).toHaveCount(4);
});

test('T-25 scoped visual samples for Explorer and graph readability',async({page},testInfo)=>{
  await page.setViewportSize({width:1600,height:1000});await row(page,entryPath).getByRole('button').click();
  for(const [name,locator] of [['explorer',explorer(page)],['graph',graphPane(page)]]){
    const path=testInfo.outputPath(`${name}.png`);await locator.screenshot({path});await testInfo.attach(name,{path,contentType:'image/png'});
  }
  await resizeExplorer(page,220);await page.getByPlaceholder('名前・相対パス').fill('Service');
  const path=testInfo.outputPath('explorer-narrow.png');await explorer(page).screenshot({path});await testInfo.attach('explorer-narrow',{path,contentType:'image/png'});
  await testInfo.attach('visual-conditions',{body:JSON.stringify(await page.evaluate(()=>({viewport:{width:innerWidth,height:innerHeight},dpr:devicePixelRatio,userAgent:navigator.userAgent}))),contentType:'application/json'});
});
