import {test,expect} from '../../apps/desktop/node_modules/@playwright/test/index.mjs';
import {readFile} from 'node:fs/promises';
const mocks=(await readFile(new URL('../../apps/desktop/node_modules/@tauri-apps/api/mocks.js',import.meta.url),'utf8')).replace(/^export \{[^\n]+\};?$/m,'');
const index=await readFile(new URL('../../crates/legacists-core/tests/fixtures/mixed/web/index.jsp',import.meta.url),'utf8');
function bootstrap(source){
  const resources=[{id:'index',path:'web/index.jsp',language:'jsp',encoding:'UTF-8',encodingReason:'共通文字／UTF-8扱い',read:'ok',decode:'ok',parse:'ok'},{id:'target',path:'src/demo/web/Entry.java',language:'java',encoding:'UTF-8',encodingReason:'共通文字／UTF-8扱い',read:'ok',decode:'ok',parse:'ok'}];
  const settings={schemaVersion:1,revision:0,project:null,selected:[],analysis:{exclusions:['.git','target'],webRoot:'web',encodings:{}},maxBytes:800000};
  const summary=()=>({projectId:'project',snapshotId:'snapshot',selection:settings.selected,selectionRevision:settings.revision,stale:false,resources:2,diagnostics:1,skipped:[{path:'target',reason:'excluded'}],webRoots:['web']});
  const info=()=>({settings,defaultRoot:'C:\\Synthetic\\legacists\\data',currentRoot:'C:\\Synthetic\\legacists\\data',bootstrap:'C:\\Synthetic\\legacists\\bootstrap.json',notice:null,pending:false});
  const relation={id:'relation',from:'index',kind:'navigation',status:'confirmed',targets:['target'],reference:'/entry',reason:'unique',evidenceIds:['e1']};
  window.__mock={requests:[],cancelOpen:false,changed:false,cancelSave:false,holdAnalysis:false};
  mockIPC((command,payload)=>{
    if(command!=='ipc')throw Error(command);
    const r=JSON.parse(JSON.stringify(payload.request));window.__mock.requests.push(r);let data;
    const page=items=>({items,total:items.length,nextCursor:null,omitted:false});
    switch(r.operation){
      case 'startup_check':data={version:'test',settings:info()};break;
      case 'settings_read':data=info();break;
      case 'project_open':data=window.__mock.cancelOpen?{cancelled:true}:summary();break;
      case 'analysis_start':window.__mock.cancelled=false;data={jobId:'job'};break;
      case 'analysis_status':data={projectId:'project',jobId:'job',sequence:window.__mock.holdAnalysis?1:2,stage:window.__mock.cancelled?'cancelled':window.__mock.holdAnalysis?'extract':'ready',terminal:!window.__mock.holdAnalysis,error:window.__mock.cancelled?'CANCELLED':null,snapshotId:'snapshot'};break;
      case 'analysis_cancel':window.__mock.holdAnalysis=false;window.__mock.cancelled=true;data={terminal:true,stage:'cancelled'};break;
      case 'project_status':data=summary();break;
      case 'project_close':data={closed:true};break;
      case 'selection_update':{const path=resources.find(a=>a.id===r.args.resourceId).path;settings.selected=settings.selected.filter(p=>p!==path);if(r.args.selected)settings.selected.push(path);settings.revision++;data=summary();break;}
      case 'settings_update':Object.assign(settings,r.args.settings,{revision:settings.revision+1});data=info();break;
      case 'export_prepare':if(window.__mock.changed)return {requestId:r.requestId,error:{code:'LEG-5001'}};data={planId:'plan',parts:[{index:0,bytes:source.length+200}]};break;
      case 'export_read':data={text:`LEGACISTS-TXT/1\nSOURCE path="web/index.jsp"\n${source}`,end:source.length+50,totalBytes:source.length+50};break;
      case 'export_save':data=window.__mock.cancelSave?{cancelled:true}:{saved:1};break;
      case 'diagnostics_report':data={text:'LEG-3001\n影響: 一部未解析\n確認: 行・列と記法\n対処: 再解析\n実ソースの提出は不要です'};break;
      case 'diagnostic_report_save':data={saved:1};break;
      case 'data_location_change':case 'data_reset':data={...info(),pending:true};break;
      case 'query':{
        const q=r.args.query;
        switch(q.type){
          case 'resources':data=page(resources.filter(a=>a.path.toLowerCase().includes(q.search.toLowerCase())));break;
          case 'source':{const resource=resources.find(a=>a.id===q.id);data=page([{resource,text:q.id==='index'?source:'class Entry {}',start:0,end:source.length,totalBytes:source.length}]);break;}
          case 'neighbors':data=page(q.kinds.length&&!q.kinds.includes('navigation')?[]:[relation]);break;
          case 'overview':data=q.group?page([resources[0]]):page([{id:'web',members:['index'],internal:[],outgoing:{}}]);break;
          case 'relation':data=page([{relation,evidence:[{path:'web/index.jsp',evidence:{id:'e1',resourceId:'index',rule:'MARKUP-REF',start:{byte:0,line:1,column:1},end:{byte:5,line:1,column:6}},excerpt:source}]}]);break;
          case 'diagnostics':data=page([{id:'d1',code:'LEG-3001',path:'bad.java',stage:'extract',reason:'syntax'}]);break;
          default:throw Error(q.type);
        }break;
      }
      default:throw Error(r.operation);
    }
    const response={requestId:r.requestId,data:structuredClone(data)};
    return r.operation==='query'&&r.args.query.type==='resources'&&r.args.query.search==='Entry'?new Promise(resolve=>setTimeout(()=>resolve(response),300)):response;
  },{shouldMockEvents:true});
}

test.beforeEach(async({page})=>{await page.addInitScript({content:`${mocks}\n(${bootstrap.toString()})(${JSON.stringify(index)});`});await page.goto('/');await expect(page.getByRole('button',{name:'フォルダを開く',exact:true})).toBeEnabled();});
async function open(page){await page.getByRole('button',{name:'フォルダを開く',exact:true}).click();await expect(page.getByTestId('analysis-status')).toHaveText('ready');await expect(page.locator('[data-path="web/index.jsp"]')).toBeVisible();}

test('T-18 opening/search/viewing does not select; manual export preserves source',async({page})=>{await open(page);const row=page.locator('[data-path="web/index.jsp"]');await row.getByRole('button').click();await expect(page.getByLabel('出力対象 web/index.jsp',{exact:true})).not.toBeChecked();await expect(page.getByLabel('ソースと根拠')).toContainText('web/index.jsp');await page.getByLabel('出力対象 web/index.jsp',{exact:true}).check();await page.getByRole('button',{name:'プレビューを作成',exact:true}).click();await expect(page.locator('.preview')).toContainText(index.trim());await expect(page.locator('.preview')).not.toContainText('class Entry {}');await page.getByRole('button',{name:'TXTを保存',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'保存しました'})).toBeVisible();await page.getByPlaceholder('名前・相対パス').fill('Entry');await expect(page.locator('.file-row')).toHaveCount(1);});
test('T-18 relation evidence, graph filtering and overview drilldown',async({page})=>{await open(page);await page.locator('[data-path="web/index.jsp"]').getByRole('button').click();await page.locator('.relation-row').click();await expect(page.locator('.evidence-list')).toContainText('MARKUP-REF');await expect(page.locator('.source-info')).toContainText('根拠 1行 1列');await page.getByLabel('種類', {exact:true}).selectOption('import');await expect(page.locator('.relation-row')).toHaveCount(0);await page.getByText('概要グラフ',{exact:true}).click();await page.getByRole('button',{name:'web · 1 ファイル / 内部 0 関係'}).click();await expect(page.locator('.file-row')).toHaveCount(1);});
test('T-18 changed input blocks preview and requires reanalysis',async({page})=>{await open(page);await page.getByLabel('出力対象 web/index.jsp',{exact:true}).check();await page.evaluate(()=>window.__mock.changed=true);await page.getByRole('button',{name:'プレビューを作成',exact:true}).click();await expect(page.locator('.notification.error')).toContainText('LEG-5001');await expect(page.getByRole('button',{name:'TXTを保存',exact:true})).toBeDisabled();});
test('T-18 settings update invalidates export; saved paths and encoding visible',async({page})=>{await open(page);await page.locator('[data-path="web/index.jsp"]').getByRole('button').click();await page.getByRole('button',{name:'設定',exact:true}).click();const dialog=page.getByRole('dialog',{name:'設定',exact:true});await expect(dialog).toContainText('案内ファイル');await dialog.getByLabel('web/index.jsp の文字コード',{exact:true}).selectOption('sjis');await dialog.getByRole('button',{name:'設定を保存',exact:true}).click();await expect(page.getByText('再解析が必要です。前回結果を表示しています',{exact:false})).toBeVisible();const requests=await page.evaluate(()=>window.__mock.requests.filter(r=>r.operation==='settings_update'));expect(requests[0].args.settings.analysis.encodings['web/index.jsp']).toBe('sjis');});
test('T-18 diagnostics report is actionable and save is explicit',async({page})=>{await open(page);await page.getByText('Diagnostics',{exact:true}).click();await expect(page.locator('.report')).toContainText('実ソースの提出は不要');await page.getByRole('button',{name:'エラーレポートを保存',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'エラーレポートを保存しました'})).toBeVisible();});
test('T-18 cancelling folder or save does not claim completion',async({page})=>{await page.evaluate(()=>window.__mock.cancelOpen=true);await page.getByRole('button',{name:'フォルダを開く',exact:true}).click();await expect(page.locator('.file-row')).toHaveCount(0);expect(await page.evaluate(()=>window.__mock.requests.some(r=>r.operation==='analysis_start'))).toBe(false);await page.evaluate(()=>{window.__mock.cancelOpen=false;window.__mock.cancelSave=true;});await open(page);await page.getByLabel('出力対象 web/index.jsp',{exact:true}).check();await page.getByRole('button',{name:'プレビューを作成',exact:true}).click();await page.getByRole('button',{name:'TXTを保存',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'保存しました'})).toHaveCount(0);});

test('T-13 T-18 late search response cannot replace the latest condition',async({page})=>{await open(page);const search=page.getByPlaceholder('名前・相対パス');await search.fill('Entry');await search.fill('index');await expect(page.locator('.file-row')).toHaveCount(1);await expect(page.locator('.file-row')).toHaveAttribute('data-path','web/index.jsp');await page.waitForTimeout(350);await expect(page.locator('.file-row')).toHaveAttribute('data-path','web/index.jsp');});
test('T-13 T-18 cancelled update retains old source and blocks TXT',async({page})=>{await open(page);await page.locator('[data-path="web/index.jsp"]').getByRole('button').click();await page.getByLabel('出力対象 web/index.jsp',{exact:true}).check();await page.evaluate(()=>window.__mock.holdAnalysis=true);await page.getByRole('button',{name:'再解析',exact:true}).click({noWaitAfter:true});await page.getByRole('button',{name:'キャンセル',exact:true}).click();await expect(page.getByTestId('analysis-status')).toHaveText('cancelled');await expect(page.locator('.source-info')).toContainText('web/index.jsp');await expect(page.getByRole('button',{name:'プレビューを作成',exact:true})).toBeDisabled();});
