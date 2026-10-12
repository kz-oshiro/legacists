import {test,expect} from '../../apps/desktop/node_modules/@playwright/test/index.mjs';
import {readFile,readdir} from 'node:fs/promises';
import {join,basename} from 'node:path';
import {start} from './harness.mjs';
test('T-19 T-22 @minimum standalone EXE real dialogs analysis evidence export settings restart',async({},testInfo)=>{
  let app=await start(testInfo);
  try{
    const {page}=app;await expect(page.getByRole('button',{name:'フォルダを開く',exact:true})).toBeEnabled();
    await page.getByRole('button',{name:'フォルダを開く',exact:true}).click({noWaitAfter:true});await app.dialog('select',app.input);
    await expect(page.getByTestId('analysis-status')).toHaveText('ready',{timeout:20000});
    const row=page.locator('[data-path="web/index.jsp"]');await row.getByRole('button').click();await expect(page.locator('.source-info')).toContainText('web/index.jsp');
    await expect(page.locator('.relation-row')).toHaveCount(3);await page.locator('.relation-row').filter({hasText:'navigation'}).click();await expect(page.locator('.evidence-list')).toContainText('WEBXML-ROUTE');
    await page.getByLabel('出力対象 web/index.jsp',{exact:true}).check();await page.getByRole('button',{name:'プレビューを作成',exact:true}).click();await expect(page.locator('.preview')).toContainText('LEGACISTS-TXT/1');
    const destination=join(app.directory,'saved.txt');await page.getByRole('button',{name:'TXTを保存',exact:true}).click({noWaitAfter:true});await app.dialog('select',destination);await expect(page.getByRole('status').filter({hasText:'TXTを1件保存しました'})).toBeVisible();
    const text=await readFile(destination,'utf8');const original=await readFile(join(app.input,'web/index.jsp'),'utf8');expect(text).toContain(original);expect(text).not.toContain('public class Service');
    expect(await readdir(join(app.directory,'standalone'))).toEqual([basename(app.executable)]);expect(app.external).toEqual([]);
    const settingsPath=join(app.local,'legacists/data/settings.json');const settings=JSON.parse(await readFile(settingsPath,'utf8'));expect(settings.selected).toEqual(['web/index.jsp']);expect(JSON.stringify(settings)).not.toContain(original);
    const existing={executable:app.executable,local:app.local};await app.stop();app=await start(testInfo,{existing});await app.page.getByRole('button',{name:'設定',exact:true}).click();await expect(app.page.getByRole('dialog',{name:'設定',exact:true})).toContainText('案内ファイル');
    const restored=JSON.parse(await readFile(settingsPath,'utf8'));expect(restored.selected).toEqual(['web/index.jsp']);
  }finally{await app.stop();}
});
