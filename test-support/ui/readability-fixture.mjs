import {readFile} from 'node:fs/promises';
const mocks=(await readFile(new URL('../../apps/desktop/node_modules/@tauri-apps/api/mocks.js',import.meta.url),'utf8')).replace(/^export \{[^\n]+\};?$/m,'');
export const entryPath='modules/長い共通フォルダ/業務/画面/入口.jsp';
export const laterPath='modules/長い共通フォルダ/業務/画面/後半.jsp';
export const duplicatePaths=['modules/長い共通フォルダ/顧客/common/Service.java','modules/長い共通フォルダ/請求/common/Service.java'];
const paths=['Root.java',entryPath,...duplicatePaths,'web/断片/header.jspf','web/画面/日本語.xhtml','web/js/main.js','config/web.xml','config/app.properties','misc/unknown.bin',...Array.from({length:6},(_,i)=>`modules/長い共通フォルダ/業務/担当${i}/非常に長い日本語ファイル名による参照先の識別確認${i}.java`),...Array.from({length:90},(_,i)=>`src/generated/Item${String(i).padStart(3,'0')}.java`),laterPath,'最後/追加.properties'];
const resources=paths.map((path,i)=>({id:`file-${i}`,path,language:path.split('.').pop(),encoding:'UTF-8',encodingReason:'合成例',read:'ok',decode:'ok',parse:i===9?'partial':'ok'}));
const relations=resources.slice(2,16).map((resource,i)=>({id:`relation-${i}`,from:'file-1',kind:['import','include','configuration'][i%3],status:i===1?'candidate':'confirmed',targets:[resource.id],reference:resource.path,reason:'synthetic',evidenceIds:[`e-${i}`]}));
relations.push({id:'unresolved',from:'file-1',kind:'module',status:'unresolved',targets:[],reference:'未解決/参照先.js',reason:'missing',evidenceIds:['missing']});
function bootstrap(fixture){
  const {resources,relations}=fixture;
  const settings={schemaVersion:1,revision:0,project:'C:\\Synthetic\\ReadableProject',selected:[],analysis:{exclusions:['.git','target'],webRoot:'web',encodings:{}},maxBytes:800000};
  const summary=()=>({projectId:'project',snapshotId:'snapshot',selection:[...settings.selected],selectionRevision:settings.revision,stale:false,resources:resources.length,diagnostics:0,skipped:Array.from({length:50},(_,i)=>({path:`target/excluded-${i}`,reason:'excluded'})),webRoots:['web']});
  const info=()=>({settings:structuredClone(settings),defaultRoot:'C:\\Synthetic\\data',currentRoot:'C:\\Synthetic\\data',bootstrap:'C:\\Synthetic\\bootstrap.json',notice:null,pending:false});
  window.__readability={requests:[],holdPage:false};
  mockIPC((command,payload)=>{
    const request=structuredClone(payload.request);window.__readability.requests.push(request);
    let data;
    const page=(items,extra={})=>({items,total:items.length,nextCursor:null,omitted:false,...extra});
    switch(request.operation){
      case 'startup_check':data={version:'test',settings:info()};break;
      case 'settings_read':data=info();break;
      case 'project_open':case 'project_status':data=summary();break;
      case 'analysis_start':data={jobId:'job'};break;
      case 'analysis_status':data={projectId:'project',jobId:'job',sequence:1,stage:'ready',terminal:true,snapshotId:'snapshot'};break;
      case 'project_close':data={closed:true};break;
      case 'diagnostics_report':data={text:'合成診断'};break;
      case 'selection_update':{
        const path=resources.find(resource=>resource.id===request.args.resourceId).path;
        settings.selected=settings.selected.filter(value=>value!==path);
        if(request.args.selected)settings.selected.push(path);
        settings.revision++;data=summary();break;
      }
      case 'query':{
        const query=request.args.query;
        switch(query.type){
          case 'resources':{
            const matching=resources.filter(resource=>resource.path.toLowerCase().includes(query.search.toLowerCase()));
            const start=request.args.cursor?Number(request.args.cursor):0, end=start+request.args.limit;
            data=page(matching.slice(start,end),{total:matching.length,nextCursor:end<matching.length?String(end):null,omitted:end<matching.length});break;
          }
          case 'source':{const resource=resources.find(resource=>resource.id===query.id);data=page([{resource,text:'// synthetic source',start:0,end:19,totalBytes:19}]);break;}
          case 'resource':data=page(resources.filter(resource=>resource.id===query.id));break;
          case 'neighbors':{
            const matching=relations.filter(relation=>(!query.kinds.length||query.kinds.includes(relation.kind))&&(!query.statuses.length||query.statuses.includes(relation.status)));
            data=page(matching,{resources:resources.slice(0,16)});break;
          }
          case 'overview':data=query.group?page(resources.slice(0,4)):page([{id:'modules/長い共通フォルダ',members:resources.slice(0,4).map(resource=>resource.id),internal:[],outgoing:{config:['group-relation']}},{id:'config',members:['file-7','file-8'],internal:[],outgoing:{}}]);break;
          case 'relation':{const relation=relations.find(relation=>relation.id===query.id);data=page([{relation,evidence:[1,2].map(n=>({path:resources[1].path,evidence:{id:`evidence-${n}`,resourceId:'file-1',rule:`SYNTHETIC-${n}`,start:{byte:0,line:1,column:1},end:{byte:5,line:1,column:6}},excerpt:'synthetic'}))}]);break;}
          case 'diagnostics':data=page([]);break;
          default:throw Error(query.type);
        }break;
      }
      default:throw Error(request.operation);
    }
    const response={requestId:request.requestId,data:structuredClone(data)};
    return request.operation==='query'&&request.args.query.type==='resources'&&request.args.cursor&&window.__readability.holdPage?new Promise(resolve=>setTimeout(()=>resolve(response),250)):response;
  },{shouldMockEvents:true});
}
export async function install(page){
  await page.addInitScript({content:`${mocks}\n(${bootstrap.toString()})(${JSON.stringify({resources,relations})});`});
}
