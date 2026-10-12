<script setup lang="ts">
import { computed,onMounted,onBeforeUnmount,ref } from 'vue';
import { DockviewVue,themeDark,type DockviewReadyEvent,type DockviewApi } from 'dockview-vue';
import 'dockview-vue/dist/styles/dockview.css';
import {createState} from './state';
import Graph from './Graph.vue';
import Source from './Source.vue';
import Settings from './Settings.vue';
import Explorer from './Explorer.vue';
import {panelDefinitions,showPanel,showAllPanels as restorePanels,type PanelId} from './panels';
import './style.css';
const s=createState();const {projectId,snapshotId,stage,running,stale,error,version,ready,resources,resourceTotal,resourceCursor,search,selected,skipped,root,graphResources,relations,relationTotal,graphCursor,direction,kind,resolution,depth,overview,overviewCursor,grouping,details,source,sourcePosition,editorPosition,diagnostics,diagnosticTotal,diagnosticCursor,report,settings,plan,part,preview,previewOffset,success,canExport}=s;
let docking:DockviewApi|undefined;const modal=ref<'settings'|'help'|null>(null);const licenses=ref(''),explorerReveal=ref(0);
const viewMenu=ref<HTMLDetailsElement>(),openPanels=ref<string[]>([]);
const panelListeners:{dispose:()=>void}[]=[];
const rootName=computed(()=>settings.value?.settings.project?.split(/[\\/]/).filter(Boolean).slice(-1)[0]??'プロジェクト');
const pathFor=(id:string)=>graphResources.value.find(r=>r.id===id)?.path??id;
const relationTitle=(r:typeof relations.value[number])=>`${pathFor(r.from)} → ${r.targets.length?r.targets.map(pathFor).join('、'):r.reference} · ${r.kind} · ${r.status}`;
function panel(name:PanelId){if(docking)showPanel(docking,name);}
function closeViewMenu(focus=false){if(viewMenu.value){viewMenu.value.open=false;if(focus)viewMenu.value.querySelector('summary')?.focus();}}
function dismissViewMenu(event:MouseEvent){if(event.target instanceof Node&&!viewMenu.value?.contains(event.target))closeViewMenu();}
function showAllPanels(){if(docking)restorePanels(docking);closeViewMenu(true);}
function onReady(event:DockviewReadyEvent){
  docking=event.api;
  panelListeners.push(
    docking.onDidAddPanel(added=>{openPanels.value=[...new Set([...openPanels.value,added.id])];}),
    docking.onDidRemovePanel(removed=>{openPanels.value=openPanels.value.filter(id=>id!==removed.id);}),
  );
  restorePanels(docking);
}
async function resource(id:string){await s.focusId(id);panel('source');}
async function showGroup(name:string){await s.group(name);explorerReveal.value++;panel('explorer');}
async function showRelation(id:string){await s.relation(id);panel('source');}
async function help(){modal.value='help';licenses.value=await (await fetch('/licenses.txt')).text();}
async function saveReport(){const result=await s.request<{cancelled?:boolean;code?:string}>('diagnostic_report_save',{},s.context());if(result.code)throw Error(result.code);if(!result.cancelled)success.value='エラーレポートを保存しました';}
onMounted(()=>{document.addEventListener('click',dismissViewMenu);void s.guard(async()=>{if(!globalThis.Proxy||!globalThis.Promise||!globalThis.ResizeObserver||!globalThis.Worker||!globalThis.Blob||!document.createElement('canvas').getContext('2d'))throw Error('既存WebView2に必要な機能がありません');await s.init();});});
onBeforeUnmount(()=>{document.removeEventListener('click',dismissViewMenu);for(const listener of panelListeners)listener.dispose();s.dispose();});
</script>

<template>
  <div class="application">
    <header class="toolbar"><div class="brand"><span class="brand-mark">L</span><strong>legacists</strong><span class="version">v{{version}}</span></div><div class="toolbar-actions">
      <details ref="viewMenu" class="view-menu" @keydown.esc.stop.prevent="closeViewMenu(true)"><summary>表示</summary><div class="view-menu-items" role="group" aria-label="パネルの表示">
        <button v-for="definition in panelDefinitions" :key="definition.id" :aria-label="definition.title" :aria-describedby="`panel-status-${definition.id}`" @click="panel(definition.id);closeViewMenu(true)"><span>{{definition.title}}</span><span :id="`panel-status-${definition.id}`" class="muted">{{openPanels.includes(definition.id)?'表示中':'閉じています'}}</span></button>
        <button class="show-all-panels" @click="showAllPanels">すべてのパネルを表示</button>
      </div></details>
      <button class="primary" :disabled="!ready||running||settings?.pending" @click="s.guard(s.open)">フォルダを開く</button>
      <button :disabled="!projectId||running||settings?.pending" @click="s.guard(s.start)">再解析</button><button v-if="running" @click="s.guard(s.cancel)">キャンセル</button>
      <button :disabled="!projectId||running" @click="s.guard(s.close)">閉じる</button><button :disabled="!settings" @click="modal='settings'">設定</button><button @click="s.guard(help)">使い方・ライセンス</button>
    </div></header>
    <div v-if="error" role="alert" class="notification error">{{error}} <span v-if="error==='LEG-5001'">入力が変わりました。再解析して選択・プレビューを確認してください。</span><span v-else-if="error==='LEG-5002'">保存先の権限・空き容量・同名ファイル・ロックを確認し、保存先を選び直してください。</span></div>
    <div v-if="success" role="status" class="notification">{{success}}</div>
    <div v-if="stale" class="notification warning">{{running?'更新前の解析結果を表示しています':'再解析が必要です。前回結果を表示しています'}}。TXT保存は停止しています。</div>
    <div v-if="settings?.notice" role="alert" class="notification warning">{{settings.notice}}</div>
    <div v-if="settings?.pending" class="notification warning">保存先の移行／データ削除を予定しました。アプリを終了して再起動してください。完了は再起動後に確認できます。</div>
    <DockviewVue class="workspace" :theme="themeDark" :disableFloatingGroups="true" @ready="onReady">
      <template #panel-explorer><section class="pane explorer" aria-label="ファイル一覧">
        <label class="search-field"><span>検索</span><input v-model="search" placeholder="名前・相対パス" @input="s.guard(()=>s.loadResources())"></label>
        <div class="muted counts">{{resources.length}} / {{resourceTotal}} ファイル · 出力対象 {{selected.length}}</div>
        <Explorer :resources="resources" :selected="selected" :active="root?.id" :running="running" :search="search" :project-id="projectId" :root-name="rootName" :reveal="explorerReveal" @focus="r=>s.guard(()=>resource(r.id))" @select="(r,checked)=>s.guard(()=>s.select(r,checked))"/>
        <button v-if="resourceCursor" class="load-more" :disabled="s.resourceLoading.value" @click="s.guard(()=>s.loadResources(true))">さらに表示</button>
        <p v-if="resourceCursor" class="muted counts">未取得のファイルがあります。続きを取得して確認できます。</p>
        <details class="exclusions"><summary>適用中の除外・読めない資源</summary><div class="exclusion-content"><p>{{settings?.settings.analysis.exclusions.join('、')}}</p><ul><li v-for="(skip,index) in skipped" :key="index">{{skip.path}} · {{skip.reason}}</li></ul></div></details>
        <p v-if="!projectId" class="empty">フォルダを開いて解析すると、ファイルと参照関係を調べられます。</p>
      </section></template>
      <template #panel-graph><section class="pane graph-pane" aria-label="関連グラフ">
        <div class="pane-toolbar wrap"><strong :title="root?.path">{{root?.path??'起点を選択'}}</strong><button v-if="root" :disabled="running" @click="s.guard(()=>s.select(root!,true))">出力対象に追加</button></div>
        <div class="pane-toolbar wrap"><label>方向<select aria-label="方向" v-model="direction" @change="s.guard(()=>s.loadGraph())"><option value="both">参照元・参照先</option><option value="in">参照元</option><option value="out">参照先</option></select></label><label>種類<select aria-label="種類" v-model="kind" @change="s.guard(()=>s.loadGraph())"><option value="">すべて</option><option v-for="k in ['import','extends','implements','type','include','script','navigation','module','configuration']" :key="k">{{k}}</option></select></label><label>解決<select aria-label="解決" v-model="resolution" @change="s.guard(()=>s.loadGraph())"><option value="">すべて</option><option value="confirmed">確定</option><option value="candidate">候補</option><option value="unresolved">未解決</option></select></label><label>深度<input v-model.number="depth" class="small-number" type="number" min="1" max="8" @change="s.guard(()=>s.loadGraph())"></label></div>
        <Graph :resources="graphResources" :relations="relations" :root="root" @relation="id=>s.guard(()=>showRelation(id))" @resource="id=>s.guard(()=>resource(id))"/>
        <div class="graph-relations" aria-label="関係一覧"><button v-for="r in relations" :key="r.id" class="relation-row" :title="relationTitle(r)" @click="s.guard(()=>showRelation(r.id))"><span class="badge" :class="r.status">{{r.status==='confirmed'?'確定':r.status==='candidate'?'候補':'未解決'}}</span> {{r.kind}} · {{pathFor(r.from).split('/').slice(-1)[0]}} → {{r.targets.length?r.targets.map(id=>pathFor(id).split('/').slice(-1)[0]).join('、'):r.reference}}</button></div>
        <div class="pane-toolbar muted">{{relations.length}} / {{relationTotal}} 関係 <span v-if="graphCursor">· 表示を省略しています</span><button v-if="graphCursor" @click="s.guard(()=>s.loadGraph(true))">さらに表示</button></div>
      </section></template>
      <template #panel-overview><section class="pane graph-pane" aria-label="概要グラフ"><div class="pane-toolbar"><label>集約<select aria-label="集約" v-model="grouping" @change="s.guard(()=>s.loadOverview())"><option value="folder">フォルダ</option><option value="package">Javaパッケージ</option></select></label><span class="muted">グループを選ぶと構成ファイルを表示します</span></div><Graph :resources="resources" :relations="[]" :groups="overview" @group="name=>s.guard(()=>showGroup(name))"/><div class="group-list"><button v-for="g in overview" :key="g.id" @click="s.guard(()=>showGroup(g.id))">{{g.id}} · {{g.members.length}} ファイル / 内部 {{g.internal.length}} 関係</button><button v-if="overviewCursor" @click="s.guard(()=>s.loadOverview(true))">さらに表示</button></div></section></template>
      <template #panel-source><section class="pane source-pane" aria-label="ソースと根拠">
        <div v-if="details" class="evidence-list"><strong>{{details.relation.kind}} · {{details.relation.status}}</strong><p>{{details.relation.reason}}</p><button v-for="e in details.evidence" :key="e.evidence.id" @click="s.guard(()=>s.evidence(e))">{{e.path}}:{{e.evidence.start.line}}:{{e.evidence.start.column}} · {{e.evidence.rule}}</button></div>
        <template v-if="source"><div class="source-info"><strong>{{source.resource.path}}</strong><span>{{source.resource.encoding}} · {{source.resource.encodingReason}} · {{source.resource.parse==='ok'?'解析済み':'一部未解析'}}</span><span v-if="sourcePosition">根拠 {{sourcePosition.line}}行 {{sourcePosition.column}}列</span><button @click="modal='settings'">文字コードを指定</button></div><Source :text="source.text" :language="source.resource.language" :position="editorPosition"/><div class="pane-toolbar muted">{{source.start}}–{{source.end}} / {{source.totalBytes}} UTF-8 bytes <button v-if="source.start" @click="s.guard(()=>s.sourcePage(0))">先頭</button><button v-if="source.end<source.totalBytes" @click="s.guard(()=>s.sourcePage(source!.end))">次の本文</button></div></template>
        <p v-else class="empty">ファイルまたは関係の根拠を選択してください。</p>
      </section></template>
      <template #panel-export><section class="pane export-pane" aria-label="調査TXT出力">
        <div class="pane-toolbar wrap"><strong>出力対象 {{selected.length}} ファイル</strong><span class="muted">手動選択したソース全文を出力します</span><button :disabled="!canExport||!selected.length" @click="s.guard(s.prepare)">プレビューを作成</button><button class="primary" :disabled="!canExport||!plan" @click="s.guard(s.save)">TXTを保存</button></div>
        <div class="selected-list"><span v-for="path in selected" :key="path">{{path}}</span></div>
        <template v-if="plan"><div class="pane-toolbar"><label>連番<select v-model.number="part" @change="s.guard(()=>s.readPreview())"><option v-for="p in plan.parts" :key="p.index" :value="p.index">{{p.index+1}} / {{plan.parts.length}} · {{p.bytes.toLocaleString()}} bytes</option></select></label><span>{{previewOffset}}–{{preview?.end}} / {{preview?.totalBytes}} bytes</span><button v-if="previewOffset" @click="s.guard(()=>s.readPreview())">先頭</button><button v-if="preview&&preview.end<preview.totalBytes" @click="s.guard(()=>s.readPreview(preview!.end))">次のプレビュー</button></div><pre class="preview" tabindex="0">{{preview?.text}}</pre></template>
        <p v-else class="empty">ツリーのチェック欄またはグラフの「出力対象に追加」で一件ずつ選択してください。閲覧だけでは追加されません。</p>
      </section></template>
      <template #panel-diagnostics><section class="pane diagnostics-pane" aria-label="解析診断"><div class="pane-toolbar"><strong>診断 {{diagnostics.length}} / {{diagnosticTotal}}</strong><button :disabled="!ready" @click="s.guard(saveReport)">エラーレポートを保存</button><button v-if="diagnosticCursor" @click="s.guard(()=>s.loadDiagnostics(true))">さらに表示</button></div><div class="diagnostic-list"><button v-for="d in diagnostics" :key="d.id" @click="s.guard(async()=>{if(d.resourceId)await s.focusId(d.resourceId);panel('source');})">{{d.code}} · {{d.path??'プロジェクト'}} · {{d.stage}} · {{d.reason}}</button></div><pre class="report">{{report}}</pre></section></template>
    </DockviewVue>
    <footer class="statusbar"><span role="status" data-testid="analysis-status">{{ready?stage:'起動しています'}}</span><span>{{snapshotId?'解析結果あり':'解析結果なし'}} · オフライン静的解析 · ソースは読み取り専用</span></footer>
    <div v-if="modal" class="modal-backdrop" @click.self="modal=null"><section class="modal" role="dialog" :aria-label="modal==='settings'?'設定':'使い方・ライセンス'"><header><h2>{{modal==='settings'?'設定':'使い方・ライセンス'}}</h2><button @click="modal=null">閉じる</button></header>
      <Settings v-if="modal==='settings'" :state="s" @close="modal=null"/>
      <template v-else><p>1. フォルダを開く → 2. ファイル・グラフから根拠を確認 → 3. 必要なファイルを一件ずつ手動選択 → 4. プレビューを確認してTXTを保存。</p><p>×で閉じたパネルは上部の「表示」で選んで戻せます。「すべてのパネルを表示」でまとめて戻せます。解析結果・検索条件・出力対象・プレビューは保持され、再起動や再解析は不要です。</p><p>ソース変更後は手動で再解析してください。TXTはUTF-8で、既定800,000バイトを超えると連番に分かれます。復号できないファイルは出力できません。未解析でも復号済みの本文は出力できます。</p><p>確定・候補・未解決を区別します。静的に明示された参照を扱い、動的処理や外部依存を完全に再現するものではありません。コードの実行・編集、外部AIへの自動送信は行いません。</p><p>エラーレポートに本文・抜粋・設定値は添付しません。実ソースを提出せず、番号・確認結果・公開可能な合成例を改修担当へ伝えてください。公開前に相対パス等を確認してください。</p><h3>依存ライセンス・NOTICE</h3><pre class="licenses">{{licenses}}</pre></template>
    </section></div>
  </div>
</template>
