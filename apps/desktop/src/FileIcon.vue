<script setup lang="ts">
import { computed } from 'vue';
const props = defineProps<{ path?: string; folder?: boolean; open?: boolean }>();
// Original inline SVG artwork. No external assets, fonts, or icon dependencies.
const kind = computed(() => {
  if (props.folder) return 'folder';
  const extension = props.path?.split('.').pop()?.toLowerCase() ?? '';
  return ['java','jsp','jspf','jspx','xhtml','js','mjs','cjs','xml','properties'].includes(extension) ? extension : 'file';
});
const badge = computed(() => ({ java: 'J', jsp: '<>', jspf: '<>', jspx: '<>', xhtml: '<>', js: 'JS', mjs: 'JS', cjs: 'JS', xml: 'X', properties: '=' }[kind.value] ?? '·'));
const label = computed(() => props.folder ? (props.open ? '開いたフォルダ' : 'フォルダ') : ({ java: 'Java', jsp: 'JSP', jspf: 'JSP断片', jspx: 'JSP XML', xhtml: 'XHTML', js: 'JavaScript', mjs: 'JavaScript', cjs: 'JavaScript', xml: 'XML', properties: 'properties' }[kind.value] ?? 'ファイル'));
</script>
<template>
  <svg class="file-icon" :data-icon="folder ? (open ? 'folder-open' : 'folder') : kind" :class="kind" viewBox="0 0 24 24" role="img" :aria-label="label">
    <template v-if="folder"><path d="M2 6h8l2 2h10v12H2Z"/><path v-if="open" d="M2 11h20l-3 9H2Z" class="folder-front"/></template>
    <template v-else><path d="M5 2h10l4 4v16H5Z"/><path d="M15 2v5h4" fill="none"/><text x="12" y="17" text-anchor="middle">{{badge}}</text></template>
  </svg>
</template>
<style scoped>
.file-icon{width:18px;height:18px;flex:0 0 18px;fill:#283446;stroke:#98abc2;stroke-width:1.2}.file-icon text{stroke:none;fill:#dce5f0;font:bold 9px "Segoe UI",sans-serif}.folder{fill:#63522f;stroke:#dbb86c}.folder-front{fill:#96793b}.java text{fill:#f5b37b}.jsp text,.jspf text,.jspx text,.xhtml text{fill:#87c6ed}.js text,.mjs text,.cjs text{fill:#e9d677}.xml text{fill:#bba5ef}.properties text{fill:#91d5ae}
</style>
