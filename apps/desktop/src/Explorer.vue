<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type { Resource } from './state';
import FileIcon from './FileIcon.vue';
const props = defineProps<{ resources: Resource[]; selected: string[]; active?: string; running: boolean; search: string; projectId?: string; rootName: string; reveal: number }>();
const emit = defineEmits<{ focus: [resource: Resource]; select: [resource: Resource, checked: boolean] }>();
interface Entry { path: string; name: string; resource?: Resource; children: Map<string, Entry> }
const collapsed = ref(new Set<string>());
const searchCollapsed = ref(new Set<string>());
watch(() => props.projectId, () => { collapsed.value = new Set(); searchCollapsed.value = new Set(); });
watch(() => props.search, () => { searchCollapsed.value = new Set(); });
watch(() => props.reveal, () => { collapsed.value = new Set(); searchCollapsed.value = new Set(); });
const closed = computed(() => props.search ? searchCollapsed.value : collapsed.value);
const tree = computed(() => {
  const root: Entry = { path: '', name: props.rootName, children: new Map() };
  for (const resource of props.resources) {
    const names = resource.path.split('/');
    let parent = root;
    names.forEach((name, index) => {
      const path = names.slice(0, index + 1).join('/');
      if (!parent.children.has(name)) parent.children.set(name, { path, name, children: new Map() });
      parent = parent.children.get(name)!;
      if (index === names.length - 1) parent.resource = resource;
    });
  }
  const rows: { entry: Entry; depth: number }[] = [];
  function visit(entry: Entry, depth: number) {
    rows.push({ entry, depth });
    if (closed.value.has(entry.path)) return;
    const children = [...entry.children.values()].sort((a, b) => Number(!!a.resource) - Number(!!b.resource) || a.name.localeCompare(b.name, 'ja', { numeric: true }));
    for (const child of children) visit(child, depth + 1);
  }
  visit(root, 0);
  return rows;
});
function toggle(path: string) {
  const next = new Set(closed.value);
  if (next.has(path)) next.delete(path); else next.add(path);
  if (props.search) searchCollapsed.value = next; else collapsed.value = next;
}
</script>
<template>
  <div class="tree" role="tree" aria-label="プロジェクトのファイル" v-if="projectId">
    <div v-for="{entry,depth} in tree" :key="entry.path" role="treeitem" :aria-level="depth+1" :aria-expanded="entry.resource ? undefined : !closed.has(entry.path)" :aria-current="entry.resource?.id === active ? 'true' : undefined" :class="['tree-row',entry.resource ? 'file-row' : 'folder-row',{active:entry.resource?.id===active}]" :style="{paddingLeft:`${8+depth*12}px`}" :data-path="entry.resource ? entry.path : undefined" :data-folder="entry.resource ? undefined : entry.path">
      <template v-if="entry.resource">
        <input type="checkbox" :aria-label="`出力対象 ${entry.path}`" :checked="selected.includes(entry.path)" :disabled="running" @change="emit('select',entry.resource,($event.target as HTMLInputElement).checked)">
        <button class="file-name" :title="entry.path" @click="emit('focus',entry.resource)"><FileIcon :path="entry.path"/><span>{{entry.name}}</span></button>
        <span v-if="entry.resource.parse!=='ok'||entry.resource.decode!=='ok'" class="badge warning" title="解析診断を確認してください">未解析</span>
      </template>
      <button v-else class="folder-name" :title="entry.path||rootName" :aria-label="`${closed.has(entry.path)?'展開':'折り畳み'} ${entry.path||rootName}`" :aria-expanded="!closed.has(entry.path)" @click="toggle(entry.path)"><span class="disclosure" aria-hidden="true">{{closed.has(entry.path)?'›':'⌄'}}</span><FileIcon folder :open="!closed.has(entry.path)"/><span>{{entry.name}}</span></button>
    </div>
  </div>
</template>
