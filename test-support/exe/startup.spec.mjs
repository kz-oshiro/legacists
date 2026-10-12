import {test,expect} from '../../apps/desktop/node_modules/@playwright/test/index.mjs';
import {readdir,readFile} from 'node:fs/promises';
import {join,basename} from 'node:path';
import {start} from './harness.mjs';
test('T-19 @minimum standalone EXE startup real IPC and isolated WebView2 lifecycle',async({},testInfo)=>{
  const app=await start(testInfo);
  try {
    await expect(app.page.getByRole('button',{name:'フォルダを開く',exact:true})).toBeEnabled();
    const version=(await readFile(new URL('../../../apps/desktop/src-tauri/Cargo.toml',import.meta.url),'utf8')).match(/^version = "([^"]+)"/m)[1];
    await expect(app.page.locator('.version')).toContainText(version);
    await app.page.getByRole('button',{name:'設定',exact:true}).click();
    await expect(app.page.getByRole('dialog',{name:'設定',exact:true})).toContainText('案内ファイル');
    expect(basename(app.executable)).toBe(`legacists-desktop-v${version}.exe`);
    expect(await readdir(join(app.directory,'standalone'))).toEqual([basename(app.executable)]);
    expect(app.external).toEqual([]);
  }finally{await app.stop();}
});
