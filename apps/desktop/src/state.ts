import { computed, ref, shallowRef } from 'vue';
import {request,subscribe,type Progress} from './ipc';
export interface Resource {id:string;path:string;language:string;encoding:string|null;encodingReason:string;read:string;decode:string;parse:string}
export interface Relation {id:string;from:string;kind:string;status:string;targets:string[];reference:string;reason:string;evidenceIds:string[]}
interface Page<T>{items:T[];resources?:Resource[];total:number;nextCursor:string|null;omitted:boolean}
interface Settings{schemaVersion:number;revision:number;project:string|null;selected:string[];analysis:{exclusions:string[];webRoot:string|null;encodings:Record<string,'utf8'|'sjis'>};maxBytes:number}
interface SettingsInfo{settings:Settings;defaultRoot:string;currentRoot:string;bootstrap:string;notice:string|null;pending:boolean}
interface Summary{projectId:string;snapshotId?:string;selection:string[];selectionRevision:number;stale:boolean;resources:number;diagnostics:number;skipped:{path:string;reason:string}[];webRoots:string[]}
interface Position {line:number;column:number}
interface SourcePage{text:string;resource:Resource;start:number;startPosition?:Position;end:number;totalBytes:number}
export function createState(){
  const projectId=ref<string>(),snapshotId=ref<string>(),jobId=ref<string>(),stage=ref('フォルダを開いてください'),running=ref(false),stale=ref(false),error=ref(''),version=ref(''),ready=ref(false);
  const resources=shallowRef<Resource[]>([]),resourceTotal=ref(0),resourceCursor=ref<string|null>(null),search=ref(''),selected=shallowRef<string[]>([]),skipped=shallowRef<Summary['skipped']>([]);
  const root=shallowRef<Resource>(),graphResources=shallowRef<Resource[]>([]),relations=shallowRef<Relation[]>([]),relationTotal=ref(0),graphCursor=ref<string|null>(null),direction=ref('both'),kind=ref(''),resolution=ref(''),depth=ref(1);
  const overview=shallowRef<any[]>([]),overviewCursor=ref<string|null>(null),grouping=ref('folder'),details=shallowRef<any>(),source=shallowRef<SourcePage>(),sourcePosition=ref<Position>(),sourceEnd=ref<Position>();
  const diagnostics=shallowRef<any[]>([]),diagnosticTotal=ref(0),diagnosticCursor=ref<string|null>(null),report=ref(''),settings=shallowRef<SettingsInfo>();
  const plan=shallowRef<{planId:string;parts:{index:number;bytes:number}[]}>(),part=ref(0),preview=shallowRef<{text:string;end:number;totalBytes:number}>(),previewOffset=ref(0),success=ref('');
  const context=()=>({projectId:projectId.value,snapshotId:snapshotId.value});
  let eventSequence=0,disposed=false,unlisten:(()=>void)|undefined;
  const generations=new Map<string,number>();
  function ticket(channel:string){const generation=(generations.get(channel)??0)+1;generations.set(channel,generation);const captured=JSON.stringify(context());return ()=>!disposed&&generations.get(channel)===generation&&JSON.stringify(context())===captured;}
  const guard=async(action:()=>Promise<unknown>)=>{error.value='';success.value='';try{await action();}catch(e){error.value=e instanceof Error?e.message:'LEG-9001';if(error.value==='LEG-5001'){stale.value=true;invalidate();}try{report.value=(await request<{text:string}>('diagnostics_report',{},context())).text;}catch{}}};
  function invalidate(){plan.value=undefined;preview.value=undefined;generations.set('preview',(generations.get('preview')??0)+1);}
  async function readSettings(){settings.value=await request<SettingsInfo>('settings_read');}
  function summary(value:Summary){selected.value=value.selection;stale.value=value.stale;skipped.value=value.skipped??[];}
  let resourceQuery:Record<string,unknown>={type:'resources',search:'',parent:null};
  async function loadResources(more=false){
    if(!snapshotId.value)return;
    const valid=ticket('resources');if(!more)resourceQuery={type:'resources',search:search.value,parent:null};
    const result=await request<Page<Resource>>('query',{query:resourceQuery,cursor:more?resourceCursor.value:null,limit:100},context());
    if(!valid())return;resources.value=more?[...resources.value,...result.items]:result.items;resourceTotal.value=result.total;resourceCursor.value=result.nextCursor;
  }
  async function loadGraph(more=false){
    if(!root.value||!snapshotId.value)return;
    const valid=ticket('graph'),rootId=root.value.id;
    const result=await request<Page<Relation>>('query',{query:{type:'neighbors',root:rootId,direction:direction.value,kinds:kind.value?[kind.value]:[],statuses:resolution.value?[resolution.value]:[],depth:depth.value},cursor:more?graphCursor.value:null,limit:400},context());
    if(!valid()||root.value?.id!==rootId)return;
    relations.value=more?[...relations.value,...result.items]:result.items;
    graphResources.value=[...new Map([...(more?graphResources.value:[]),...(result.resources??resources.value)].map(r=>[r.id,r])).values()];
    relationTotal.value=result.total;graphCursor.value=result.nextCursor;
  }
  async function loadOverview(more=false){if(!snapshotId.value)return;const valid=ticket('overview');const result=await request<Page<any>>('query',{query:{type:'overview',grouping:grouping.value,group:null},cursor:more?overviewCursor.value:null,limit:100},context());if(valid()){overview.value=more?[...overview.value,...result.items]:result.items;overviewCursor.value=result.nextCursor;}}
  async function loadDiagnostics(more=false){if(!snapshotId.value)return;const valid=ticket('diagnostics');const result=await request<Page<any>>('query',{query:{type:'diagnostics'},cursor:more?diagnosticCursor.value:null,limit:100},context());const rendered=await request<{text:string}>('diagnostics_report',{},context());if(valid()){diagnostics.value=more?[...diagnostics.value,...result.items]:result.items;diagnosticTotal.value=result.total;diagnosticCursor.value=result.nextCursor;report.value=rendered.text;}}
  async function finish(){
    const state=await request<Summary>('project_status');if(state.projectId!==projectId.value)return;
    snapshotId.value=state.snapshotId;summary(state);running.value=false;invalidate();
    await Promise.all([loadResources(),loadOverview(),loadDiagnostics(),readSettings()]);
    if(root.value){const existing=resources.value.find(r=>r.path===root.value?.path);root.value=existing;if(existing)await focus(existing);}
  }
  async function event(p:Progress){
    if(p.projectId!==projectId.value||p.jobId!==jobId.value||p.sequence<=eventSequence)return;
    eventSequence=p.sequence;stage.value=`${p.stage}${p.completed===undefined?'':` ${p.completed}${p.total==null?'':` / ${p.total}`}`}`;
    if(p.terminal){running.value=false;if(p.error){error.value=p.error;if(p.error!=='CANCELLED')report.value=(await request<{text:string}>('diagnostics_report',{},context())).text;}else if(p.stage==='ready')await finish();}
  }
  async function start(){
    await readSettings();eventSequence=0;invalidate();running.value=true;stale.value=!!snapshotId.value;
    try{const result=await request<{jobId:string}>('analysis_start',{settingsRevision:settings.value!.settings.revision},context());jobId.value=result.jobId;const currentJob=result.jobId;
      while(running.value&&!disposed&&jobId.value===currentJob){await new Promise(r=>setTimeout(r,120));if(!running.value)break;await event(await request<Progress>('analysis_status',{jobId:currentJob},context()));}
    }finally{running.value=false;}
  }
  function clear(){resources.value=[];graphResources.value=[];relations.value=[];overview.value=[];root.value=undefined;source.value=undefined;details.value=undefined;diagnostics.value=[];report.value='';invalidate();}
  async function open(){const result=await request<Summary&{cancelled?:boolean}>('project_open');if(result.cancelled)return;projectId.value=result.projectId;snapshotId.value=undefined;clear();summary(result);await start();}
  async function focus(resource:Resource){root.value=resource;const valid=ticket('source');const result=await request<Page<SourcePage>>('query',{query:{type:'source',id:resource.id,offset:0,length:1048576}},context());if(!valid()||root.value?.id!==resource.id)return;source.value=result.items[0];sourcePosition.value=undefined;sourceEnd.value=undefined;details.value=undefined;await loadGraph();}
  async function focusId(id:string){const known=[...resources.value,...graphResources.value].find(r=>r.id===id);if(known)return focus(known);const valid=ticket('resource');const result=await request<Page<Resource>>('query',{query:{type:'resource',id},limit:1},context());if(valid())await focus(result.items[0]);}
  async function sourcePage(offset:number){if(!source.value)return;const valid=ticket('source');const result=await request<Page<SourcePage>>('query',{query:{type:'source',id:source.value.resource.id,offset,length:1048576}},context());if(valid()){source.value=result.items[0];sourcePosition.value=undefined;}}
  async function select(resource:Resource,checked:boolean){const result=await request<Summary>('selection_update',{resourceId:resource.id,selected:checked},context());summary(result);invalidate();await readSettings();}
  async function relation(id:string){const valid=ticket('relation');const result=await request<Page<any>>('query',{query:{type:'relation',id}},context());if(!valid())return;details.value=result.items[0];if(details.value.evidence.length)await evidence(details.value.evidence[0]);}
  async function evidence(item:any){const valid=ticket('source'),e=item.evidence;const result=await request<Page<SourcePage>>('query',{query:{type:'source',id:e.resourceId,offset:e.start.byte<1048576?0:e.start.byte,length:1048576}},context());if(valid()){source.value=result.items[0];sourcePosition.value=e.start;sourceEnd.value=e.end;}}
  async function prepare(){invalidate();const valid=ticket('preview');const result=await request<typeof plan.value>('export_prepare',{},context());if(valid()){plan.value=result;part.value=0;previewOffset.value=0;await readPreview();}}
  async function readPreview(offset=0){if(!plan.value)return;const valid=ticket('preview');const result=await request<typeof preview.value>('export_read',{planId:plan.value.planId,part:part.value,offset,length:65536},context());if(valid()){previewOffset.value=offset;preview.value=result;}}
  async function save(){if(!plan.value)return;const result=await request<{cancelled?:boolean;saved:number;code?:string}>('export_save',{planId:plan.value.planId},context());if(result.cancelled)return;if(result.code){success.value=result.saved?`TXTを${result.saved}件保存しました。残りの保存に失敗しました`:'';throw Error(result.code);}success.value=`TXTを${result.saved}件保存しました`;}
  async function updateSettings(value:Settings){const changed=JSON.stringify(value.analysis)!==JSON.stringify(settings.value!.settings.analysis);settings.value=await request('settings_update',{settings:value,revision:settings.value!.settings.revision});stale.value ||= !!snapshotId.value&&changed;invalidate();}
  async function group(name:string){resourceQuery={type:'overview',grouping:grouping.value,group:name};resourceCursor.value=null;resources.value=[];await loadResources(true);}
  async function init(){const startup=await request<{version:string;settings:SettingsInfo}>('startup_check');version.value=startup.version;settings.value=startup.settings;unlisten=await subscribe(p=>void event(p).catch(()=>{error.value='LEG-9001';}));ready.value=true;}
  const editorPosition=computed(()=>{if(!sourcePosition.value)return;const base=source.value?.startPosition??{line:1,column:1},p=sourcePosition.value;const end=sourceEnd.value;return {line:p.line-base.line+1,column:p.line===base.line?p.column-base.column+1:p.column,endLine:end?end.line-base.line+1:undefined,endColumn:end?(end.line===base.line?end.column-base.column+1:end.column):undefined};});
  return {projectId,snapshotId,jobId,stage,running,stale,error,version,ready,resources,resourceTotal,resourceCursor,search,selected,skipped,root,graphResources,relations,relationTotal,graphCursor,direction,kind,resolution,depth,overview,overviewCursor,grouping,details,source,sourcePosition,editorPosition,diagnostics,diagnosticTotal,diagnosticCursor,report,settings,plan,part,preview,previewOffset,success,canExport:computed(()=>!!snapshotId.value&&!running.value&&!stale.value&&!settings.value?.pending),guard,open,start,loadResources,loadGraph,loadOverview,loadDiagnostics,focus,focusId,sourcePage,select,relation,evidence,prepare,readPreview,save,updateSettings,group,init,readSettings,request,context,invalidate,dispose(){disposed=true;unlisten?.();},async cancel(){await request('analysis_cancel',{jobId:jobId.value},context());},async close(){await request('project_close',{},context());projectId.value=undefined;snapshotId.value=undefined;clear();}};
}
export type AppState=ReturnType<typeof createState>;
