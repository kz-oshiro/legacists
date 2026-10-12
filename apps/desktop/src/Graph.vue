<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from 'vue';
import cytoscape from 'cytoscape';
import type { Resource, Relation } from './state';
const props = defineProps<{ resources: Resource[]; relations: Relation[]; root?: Resource; groups?: any[] }>();
const emit = defineEmits<{ relation: [id: string]; resource: [id: string]; group: [name: string] }>();
const container = ref<HTMLDivElement>();
const omitted = ref(0), zoom = ref(100), inspection = ref('');
let graph: cytoscape.Core | undefined, observer: ResizeObserver | undefined;
let initialView = true;
const statusName = (value: string) => ({ confirmed: '確定', candidate: '候補', unresolved: '未解決' }[value] ?? value);
const measure = document.createElement('canvas').getContext('2d')!;
measure.font = '14px "Segoe UI", "Yu Gothic UI", sans-serif';
function shorten(value: string) {
  if (measure.measureText(value).width <= 190) return value;
  const chars = Array.from(value);
  let left = Math.ceil(chars.length / 2), right = chars.length - left;
  while (left + right > 2) {
    const text = `${chars.slice(0, left).join('')}…${right ? chars.slice(-right).join('') : ''}`;
    if (measure.measureText(text).width <= 190) return text;
    if (left > right) left--; else right--;
  }
  return `${chars[0]}…${chars.slice(-1)[0]}`;
}
function readableView() {
  if (!graph?.nodes().length || !container.value?.clientWidth) return;
  initialView = true;
  graph.resize();
  graph.zoom(1);
  const node = props.root ? graph.getElementById(props.root.id) : graph.nodes().first();
  const point = (node.length ? node : graph.nodes().first()).position();
  graph.pan({ x: Math.min(130, container.value.clientWidth / 2) - point.x, y: 54 - point.y });
}
function scale(factor: number) {
  if (!graph) return;
  initialView = false;
  graph.zoom({ level: Math.max(graph.minZoom(), Math.min(graph.maxZoom(), graph.zoom() * factor)), renderedPosition: { x: graph.width() / 2, y: graph.height() / 2 } });
}
function fit() { if (graph?.nodes().length) { initialView = false; graph.fit(undefined, 28); } }
function render() {
  if (!graph) return;
  const elements: cytoscape.ElementDefinition[] = [], seen = new Set<string>();
  const byId = new Map(props.resources.map(resource => [resource.id, resource]));
  if (props.root) byId.set(props.root.id, props.root);
  const add = (id: string, path: string, unresolved = false, count?: number) => {
    if (seen.has(id)) return;
    seen.add(id);
    elements.push({ data: { id, path, unresolved, count, isRoot: id === props.root?.id } });
  };
  if (props.groups) {
    for (const group of props.groups) add(group.id, group.id, false, group.members.length);
    for (const group of props.groups) for (const [target, ids] of Object.entries(group.outgoing)) {
      add(target, target);
      elements.push({ data: { id: `group-edge-${group.id}->${target}`, source: group.id, target, label: `${(ids as string[]).length} 関係`, status: 'confirmed' } });
    }
  } else {
    if (props.root) add(props.root.id, props.root.path);
    for (const relation of props.relations) {
      add(relation.from, byId.get(relation.from)?.path ?? relation.from);
      for (const target of relation.targets.length ? relation.targets : [`unresolved-${relation.id}`]) {
        add(target, byId.get(target)?.path ?? (relation.targets.length ? target : relation.reference), !relation.targets.length);
        elements.push({ data: { id: `edge-${relation.id}-${target}`, relationId: relation.id, source: relation.from, target, label: `${relation.kind} · ${statusName(relation.status)}`, status: relation.status } });
      }
    }
  }
  const nodes = elements.filter(element => !element.data.source).slice(0, 200);
  const ids = new Set(nodes.map(node => node.data.id));
  const edges = elements.filter(element => element.data.source && ids.has(element.data.source) && ids.has(element.data.target)).slice(0, 400);
  const names = new Map<string, typeof nodes>();
  for (const node of nodes) {
    const name = node.data.path.split('/').slice(-1)[0];
    names.set(name, [...(names.get(name) ?? []), node]);
  }
  const used = new Set<string>();
  for (const node of nodes) {
    const parts = node.data.path.split('/'), name = parts.pop()!;
    const peers = names.get(name)!;
    let length = 1;
    while (length < parts.length && peers.some(peer => peer !== node && peer.data.path.split('/').slice(0, -1).slice(-length).join('/') === parts.slice(-length).join('/'))) length++;
    let context = node.data.unresolved ? '未解決の参照' : props.groups ? `${node.data.count ?? '?'} ファイル` : parts.slice(-length).join('/') || '(ルート)';
    let label = `${shorten(name)}\n${shorten(context)}`;
    if (used.has(label)) { context = `${shorten(context).slice(0, 14)} [${used.size + 1}]`; label = `${shorten(name)}\n${context}`; }
    used.add(label);
    node.data.label = label;
  }
  omitted.value = elements.length - nodes.length - edges.length;
  graph.elements().remove();
  graph.add([...nodes, ...edges]);
  const columns = Math.max(1, Math.min(4, Math.floor((container.value?.clientWidth ?? 0) / 270)));
  // Fixed card dimensions include both label lines; the viewport never squeezes cards.
  graph.nodes().forEach((node, index) => { node.position({ x: index % columns * 270, y: Math.floor(index / columns) * 112 }); });
  inspection.value = props.root ? `起点: ${props.root.path}` : '';
  readableView();
  container.value?.setAttribute('data-layout', 'ready');
}
onMounted(() => {
  graph = cytoscape({ container: container.value, layout: { name: 'preset' }, minZoom: 0.08, maxZoom: 2.5, style: [
    { selector: 'node', style: { label: 'data(label)', shape: 'round-rectangle', width: 214, height: 64, color: '#e5edf7', 'background-color': '#293e52', 'border-width': 1.5, 'border-color': '#6f8da8', 'font-family': '"Segoe UI", "Yu Gothic UI", sans-serif', 'font-size': 14, 'text-wrap': 'wrap', 'text-max-width': '194px', 'text-valign': 'center', 'text-halign': 'center', 'line-height': 1.4 } },
    { selector: 'node[?isRoot]', style: { 'background-color': '#21586b', 'border-color': '#63d5e2', 'border-width': 3 } },
    { selector: 'node[?unresolved]', style: { 'border-style': 'dotted', 'border-color': '#e0a0a0' } },
    { selector: 'edge', style: { label: '', 'font-size': 12, color: '#e7edf5', width: 1.6, 'line-color': '#7a94ad', 'target-arrow-color': '#9cb9d2', 'target-arrow-shape': 'triangle', 'curve-style': 'bezier', 'control-point-step-size': 48, 'text-background-color': '#18212e', 'text-background-opacity': 1, 'text-background-padding': '4px', 'text-rotation': 'autorotate' } },
    { selector: 'edge:selected, edge.inspect', style: { label: 'data(label)', width: 3, 'line-color': '#77d5e4' } },
    { selector: 'edge[status="candidate"]', style: { 'line-style': 'dashed', 'line-color': '#d5bd78' } },
    { selector: 'edge[status="unresolved"]', style: { 'line-style': 'dotted', 'line-color': '#df9999' } }
  ] });
  graph.on('zoom', () => { zoom.value = Math.round(graph!.zoom() * 100); });
  graph.on('dragpan scrollzoom pinchzoom', () => { initialView = false; });
  graph.on('mouseover tap', 'node', event => { inspection.value = `${event.target.data('unresolved') ? '未解決: ' : ''}${event.target.data('path')}`; });
  graph.on('mouseover', 'edge', event => {
    graph!.edges().removeClass('inspect'); event.target.addClass('inspect');
    inspection.value = `${event.target.source().data('path')} → ${event.target.target().data('path')} · ${event.target.data('label')}`;
  });
  graph.on('mouseout', 'edge', event => event.target.removeClass('inspect'));
  graph.on('tap', 'edge', event => { const id = event.target.data('relationId'); if (id) emit('relation', id); });
  graph.on('tap', 'node', event => { const node = event.target; if (props.groups) emit('group', node.id()); else if (!node.data('unresolved')) emit('resource', node.id()); });
  observer = new ResizeObserver(() => { graph?.resize(); if (initialView) readableView(); });
  observer.observe(container.value!);
  render();
});
watch(() => [props.resources, props.relations, props.groups, props.root], render);
onBeforeUnmount(() => { observer?.disconnect(); graph?.destroy(); });
</script>
<template>
  <div class="graph-canvas">
    <div class="graph-controls" aria-label="グラフ表示操作"><button aria-label="縮小" @click="scale(1/1.25)">−</button><output aria-label="表示倍率">{{zoom}}%</output><button aria-label="拡大" @click="scale(1.25)">＋</button><button @click="fit">全体表示</button><button @click="readableView">{{groups?'先頭へ':'起点へ'}}</button><span class="muted">ドラッグで移動 · 線を選ぶと根拠</span></div>
    <div ref="container" class="canvas" aria-label="関係グラフ"></div>
    <div class="graph-inspection" :title="inspection" aria-live="polite">{{inspection || 'ファイルや線にポインターを置くと詳細を表示します'}}</div>
    <p v-if="omitted" class="limit">グラフ上限（200資源・400線）により{{omitted}}要素を省略しています。関係一覧とフィルターで確認できます。</p>
  </div>
</template>
<style scoped>
.graph-canvas{display:flex;flex-direction:column;height:100%;min-height:120px;width:100%;overflow:hidden}.canvas{width:100%;flex:1;min-height:0}.graph-controls{display:flex;align-items:center;flex-wrap:wrap;gap:6px;padding:6px 8px;border-bottom:1px solid #323e50;flex-shrink:0}.graph-controls button{padding:3px 7px;font-size:12px}.graph-controls output{min-width:40px;text-align:center;font-size:12px}.graph-controls span{font-size:11px}.graph-inspection{padding:5px 8px;min-height:28px;max-height:64px;overflow:auto;overflow-wrap:anywhere;font-size:12px;background:#141b25;flex-shrink:0}.limit{margin:0;padding:6px;background:#473d29;color:#e3c88e;font-size:11px;flex-shrink:0}
</style>
