<script setup lang="ts">
import {ref} from 'vue';
import type {AppState} from './state';
const {state:s}=defineProps<{state:AppState}>();const emit=defineEmits<{close:[]}>();
const {source,settings,running}=s;const draft=ref(structuredClone(settings.value!.settings));const exclusions=ref(draft.value.analysis.exclusions.join('\n'));const resetConfirm=ref(false);
async function apply(){draft.value.analysis.exclusions=exclusions.value.split('\n').map(p=>p.trim()).filter(Boolean);await s.updateSettings(draft.value);emit('close');}
function encoding(event:Event){if(!source.value)return;const value=(event.target as HTMLSelectElement).value;if(value==='auto')delete draft.value.analysis.encodings[source.value.resource.path];else if(value==='utf8'||value==='sjis')draft.value.analysis.encodings[source.value.resource.path]=value;}
</script>
<template>
  <p>解析条件を変更した後は再解析が必要です。元ファイルは変更しません。</p>
  <label>TXTの容量上限（bytes）<input v-model.number="draft.maxBytes" type="number" min="16384" max="1000000000"></label>
  <label>除外するフォルダ名・相対パス（1行に一つ）<textarea v-model="exclusions" rows="5"></textarea></label>
  <label>Webルート（プロジェクト内の相対フォルダ、未指定は自動候補）<input :value="draft.analysis.webRoot??''" @input="draft.analysis.webRoot=($event.target as HTMLInputElement).value||null"></label>
  <label v-if="source">{{source.resource.path}} の文字コード<select :aria-label="`${source.resource.path} の文字コード`" :value="draft.analysis.encodings[source.resource.path]??'auto'" @change="encoding"><option value="auto">自動判定</option><option value="utf8">UTF-8</option><option value="sjis">SJIS系（CP932を含む）</option></select></label>
  <button class="primary" :disabled="running||settings?.pending" @click="s.guard(apply)">設定を保存</button>
  <hr><h3>保存データ</h3><dl><dt>既定</dt><dd>{{settings?.defaultRoot}}</dd><dt>現在</dt><dd>{{settings?.currentRoot}}</dd><dt>案内ファイル</dt><dd>{{settings?.bootstrap}}</dd></dl>
  <p>選択・文字コード指定・除外などの設定とWebView2作業データを管理します。ソース本文と解析結果は永続保存しません。移行・削除は次回起動時に行います。</p>
  <button :disabled="running||settings?.pending" @click="s.guard(async()=>{await s.request('data_location_change');await s.readSettings();s.invalidate();})">保存先を変更</button>
  <button :disabled="running||settings?.pending" @click="resetConfirm=!resetConfirm">保存データを削除</button>
  <div v-if="resetConfirm" class="notification warning"><p>現在の管理領域の設定・WebView2作業データと案内ファイルを削除します。解析元ファイルと保存済みTXTは削除しません。再起動後は初期設定になります。</p><button @click="s.guard(async()=>{await s.request('data_reset');await s.readSettings();resetConfirm=false;s.invalidate();})">削除予定を記録</button></div>
</template>
