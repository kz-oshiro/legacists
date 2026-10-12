<script setup lang="ts">
import { onMounted,onBeforeUnmount,ref,watch } from 'vue';
import cytoscape from 'cytoscape';
import type {Resource,Relation} from './state';
const props=defineProps<{resources:Resource[];relations:Relation[];root?:Resource;groups?:any[]}>();
const emit=defineEmits<{relation:[id:string];resource:[id:string];group:[name:string]}>();
const container=ref<HTMLDivElement>();const omitted=ref(0);let graph:cytoscape.Core|undefined;let observer:ResizeObserver|undefined;
function render(){if(!graph)return;const elements:cytoscape.ElementDefinition[]=[];const seen=new Set<string>();
  const add=(id:string,label:string)=>{if(!seen.has(id)){seen.add(id);elements.push({data:{id,label}});}};
  if(props.groups){for(const g of props.groups){add(g.id,`${g.id}\n${g.members.length} ファイル`);}for(const g of props.groups){for(const [target,ids]of Object.entries(g.outgoing)){add(target,target);elements.push({data:{id:`${g.id}->${target}`,source:g.id,target,label:`${(ids as string[]).length} 関係`,status:'confirmed'}});}}}
  else{if(props.root)add(props.root.id,props.root.path);for(const r of props.relations){add(r.from,props.resources.find(a=>a.id===r.from)?.path??r.from.slice(0,10));const targets=r.targets.length?r.targets:[`unresolved-${r.id}`];for(const target of targets){add(target,props.resources.find(a=>a.id===target)?.path??(r.targets.length?target.slice(0,10):r.reference));elements.push({data:{id:`edge-${r.id}-${target}`,relationId:r.id,source:r.from,target,label:`${r.kind} · ${r.status}`,status:r.status}});}}}
  const nodes=elements.filter(e=>!e.data.source).slice(0,200),ids=new Set(nodes.map(e=>e.data.id));
  const edges=elements.filter(e=>e.data.source&&ids.has(e.data.source)&&ids.has(e.data.target)).slice(0,400);
  omitted.value=elements.length-nodes.length-edges.length;
  graph.elements().remove();graph.add([...nodes,...edges]);if(nodes.length&&container.value?.clientWidth&&container.value.clientHeight){graph.resize();graph.layout({name:'breadthfirst',directed:true,spacingFactor:1.35,animate:false}).run();}container.value?.setAttribute('data-layout','ready');}
onMounted(()=>{graph=cytoscape({container:container.value,layout:{name:'preset'},style:[{selector:'node',style:{label:'data(label)',color:'#dce6f3','background-color':'#31536b','font-size':11,'text-wrap':'wrap','text-max-width':'160px'}},{selector:'edge',style:{label:'data(label)','font-size':10,color:'#b9c9dc',width:1.5,'line-color':'#6b879f','target-arrow-color':'#6b879f','target-arrow-shape':'triangle','curve-style':'bezier'}},{selector:'edge[status="candidate"]',style:{'line-style':'dashed'}},{selector:'edge[status="unresolved"]',style:{'line-style':'dotted'}}]});graph.on('tap','edge',event=>{const id=event.target.data('relationId');if(id)emit('relation',id);});graph.on('tap','node',event=>{const id=event.target.id();if(props.groups)emit('group',id);else if(!id.startsWith('unresolved-'))emit('resource',id);});observer=new ResizeObserver(render);observer.observe(container.value!);render();});
watch(()=>[props.resources,props.relations,props.groups,props.root],render);onBeforeUnmount(()=>{observer?.disconnect();graph?.destroy();});
</script>
<template><div class="graph-canvas"><div ref="container" class="canvas" aria-label="関係グラフ"></div><p v-if="omitted" class="limit">グラフ上限（200資源・400線）により{{omitted}}要素を省略しています。関係一覧とフィルターで確認できます。</p></div></template>
<style scoped>.graph-canvas{position:relative;height:100%;min-height:120px;width:100%}.canvas{width:100%;height:100%}.limit{position:absolute;bottom:0;left:0;right:0;margin:0;padding:6px;background:#473d29;color:#e3c88e;font-size:10px}</style>
