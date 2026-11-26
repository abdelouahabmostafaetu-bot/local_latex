<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage } from '../lib/ipc';
import { state } from '../lib/store';
import Icon from './Icon.vue';
defineEmits<{ close: [] }>();
type Change = { path: string; staged: boolean; workingTree: boolean; untracked: boolean };
type Overview = { isRepository: boolean; branch: string | null; detached: boolean; changes: Change[]; truncated: boolean };
const result = ref<Overview | null>(null), error = ref(''), loading = ref(false);
let sequence = 0;
async function refresh() {
  const id = ++sequence, root = state.info?.root;
  loading.value = true; error.value = ''; result.value = null;
  try { const data = await invoke<Overview>('git_overview'); if (id === sequence && root === state.info?.root) result.value = data; }
  catch (e) { if (id === sequence && root === state.info?.root) error.value = errorMessage(e); }
  finally { if (id === sequence) loading.value = false; }
}
onMounted(refresh); watch(() => state.info?.root, refresh); onBeforeUnmount(() => { sequence++; });
</script>
<template>
  <aside class="git-panel" aria-label="Git overview">
    <header class="git-head"><div><small>LOCALLEAF / VERSION CONTROL</small><h2>Git overview</h2></div><button class="icon" aria-label="Close Git overview" @click="$emit('close')"><Icon name="close" :size="20" /></button></header>
    <div class="git-body">
      <div class="actions"><span class="eyebrow">LOCAL REPOSITORY</span><button class="refresh" :disabled="loading" @click="refresh">{{ loading ? 'Checking…' : 'Refresh' }}</button></div>
      <p v-if="loading" role="status" class="notice">Reading repository…</p><p v-else-if="error" role="alert" class="notice error">{{ error }}</p>
      <template v-else-if="result">
        <div v-if="!result.isRepository" class="empty"><strong>No Git repository found</strong><p>Open a project whose root contains a Git repository.</p></div>
        <template v-else>
          <div class="branch"><Icon name="account_tree" :size="18" /><div><small>{{ result.detached ? 'DETACHED HEAD' : 'CURRENT BRANCH' }}</small><strong>{{ result.branch || 'Unknown' }}</strong></div></div>
          <div class="section-title"><span>CHANGES ON DISK</span><span>{{ result.changes.length }}{{ result.truncated ? '+' : '' }}</span></div>
          <p v-if="!result.changes.length" class="empty">No changes detected.</p>
          <ul v-else class="files"><li v-for="file in result.changes" :key="file.path"><span class="path" :title="file.path">{{ file.path }}</span><span class="badges"><span v-if="file.staged">Staged</span><span v-if="file.workingTree">Modified</span><span v-if="file.untracked">Untracked</span></span></li></ul>
          <p v-if="result.truncated" class="notice">Only the first 1,000 paths are shown.</p>
          <p class="note">Read-only overview of saved files. Unsaved editor changes and submodule changes are not included. Global Git configuration is not read; repositories with configured local filters are not scanned. Refresh after changes.</p>
        </template>
      </template>
    </div>
  </aside>
</template>
<style scoped>
.git-panel{display:flex;flex:0 0 clamp(280px,26vw,350px);min-width:0;flex-direction:column;background:#14231d;color:#e8f2ee;border-left:1px solid #3a5144;font-family:var(--font-sans)}.git-head{display:flex;align-items:center;justify-content:space-between;padding:17px;border-bottom:1px solid #32483d}.git-head small,.eyebrow,.branch small,.section-title{font-size:10px;letter-spacing:.1em;font-weight:700;color:#a8c0b2}.git-head h2{font-size:16px;font-weight:700;margin:3px 0 0}.icon{width:36px;height:36px;border-radius:6px;color:#abc1b2}.icon:hover{background:#30493b;color:white}.git-body{padding:18px 16px;overflow:auto}.actions{display:flex;align-items:center;justify-content:space-between;gap:8px}.refresh{border:1px solid #4c6b58;border-radius:6px;padding:6px 10px;font-size:12px}.refresh:hover{background:#294635}.refresh:disabled{opacity:.6}.branch{display:flex;align-items:center;gap:10px;padding:15px;margin:18px 0;background:#203a2c;border:1px solid #3f624c;border-radius:8px;color:#a5e0b7}.branch strong,.branch small{display:block;overflow-wrap:anywhere}.branch strong{margin-top:3px;font-size:14px;color:#eff9f2}.section-title{display:flex;justify-content:space-between}.files{margin-top:9px}.files li{padding:10px 0;border-bottom:1px solid #32483d}.path{display:block;overflow-wrap:anywhere;font-family:var(--font-mono);font-size:12px}.badges{display:flex;flex-wrap:wrap;gap:5px;margin-top:6px}.badges span{padding:2px 6px;background:#284c38;color:#b9e6c9;border-radius:4px;font-size:10px}.empty,.notice,.note{color:#b3c7b8;font-size:12px;line-height:1.55}.empty{padding:20px 0}.empty strong{color:#e8f2ee}.error{color:#ffc6a4}.note{border-top:1px solid #32483d;margin-top:20px;padding-top:14px}.git-panel button:focus-visible{outline:2px solid #94e1ae;outline-offset:2px}@media(max-width:850px){.git-panel{position:absolute;z-index:35;right:0;top:0;bottom:0;width:min(350px,100%);box-shadow:-12px 0 32px #0005}}
</style>
