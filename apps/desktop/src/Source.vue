<script setup lang="ts">
import { ref,onMounted,onBeforeUnmount,watch } from 'vue';
import * as monaco from 'monaco-editor/editor/editor.api.js';
import EditorWorker from 'monaco-editor/editor/editor.worker.js?worker';
const props=defineProps<{text:string;language:string;position?:{line:number;column:number;endLine?:number;endColumn?:number}}>();
const container=ref<HTMLDivElement>();let editor:monaco.editor.IStandaloneCodeEditor|undefined;let model:monaco.editor.ITextModel|undefined;
(globalThis as any).MonacoEnvironment={getWorker:()=>new EditorWorker()};
function update(){model?.dispose();model=monaco.editor.createModel(props.text,props.language);editor?.setModel(model);jump();}
function jump(){if(props.position){const position={lineNumber:props.position.line,column:props.position.column};editor?.revealPositionInCenter(position);if(props.position.endLine&&props.position.endColumn)editor?.setSelection(new monaco.Range(position.lineNumber,position.column,props.position.endLine,props.position.endColumn));else editor?.setPosition(position);}}
onMounted(()=>{editor=monaco.editor.create(container.value!,{readOnly:true,domReadOnly:true,automaticLayout:true,minimap:{enabled:false},theme:'vs-dark',fontSize:13,renderValidationDecorations:'off',links:false,contextmenu:false,wordWrap:'off'});update();});
watch(()=>[props.text,props.language],update);watch(()=>props.position,jump);onBeforeUnmount(()=>{editor?.dispose();model?.dispose();});
</script>
<template><div ref="container" class="source-editor" aria-label="読み取り専用ソース"></div></template>
<style scoped>.source-editor{height:100%;min-height:120px}</style>
