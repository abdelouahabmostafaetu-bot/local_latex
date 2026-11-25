<script setup lang="ts">
import { computed, ref } from 'vue';
import { relatedOccurrences, type ProjectIndex, type SymbolOccurrence } from '../lib/observatory';
import { gotoLocation, state } from '../lib/store';
import { basename } from '../lib/latex';
import Icon from './Icon.vue';

const props = defineProps<{ index: ProjectIndex }>();
defineEmits<{ close: [] }>();
const query = ref('');
const selected = computed(() => props.index.occurrences.find((item) => item.file === state.openPath && item.line === state.cursor.line && state.cursor.col >= item.column && state.cursor.col <= item.column + item.key.length) ?? null);
const relations = computed(() => selected.value ? relatedOccurrences(props.index, selected.value) : []);
const issues = computed(() => {
  const q = query.value.trim().toLowerCase();
  return props.index.missing.filter(item => !q || `${item.key} ${item.file}`.toLowerCase().includes(q));
});
const duplicates = computed(() => {
  const q = query.value.trim().toLowerCase();
  return props.index.duplicates.filter(group => !q || group.some(item => `${item.key} ${item.file}`.toLowerCase().includes(q)));
});
const references = computed(() => props.index.occurrences.filter(o => o.kind === 'reference' || o.kind === 'citation').length);
const buildLabel = computed(() => state.compiling ? 'Compiling…' : state.compileError ? 'Build could not start' : !state.result ? 'No build yet' : state.result.success ? 'Last build succeeded' : 'Last build has errors');
const buildCount = computed(() => (state.result?.errors.length ?? 0) + (state.compileError ? 1 : 0));
function go(item: SymbolOccurrence) {
  if (state.ui.layout === 'pdf') state.ui.layout = 'split';
  void gotoLocation(item.file, item.line, item.column);
}
function label(item: SymbolOccurrence) {
  return item.kind === 'label' ? 'Definition' : item.kind === 'bibentry' ? 'Bibliography' : item.kind === 'citation' ? 'Citation' : 'Reference';
}
function openLogs() {
  if (state.ui.layout === 'editor') state.ui.layout = 'split';
  state.ui.showLogs = true;
}
</script>

<template>
  <aside class="atlas" aria-label="Reference Atlas">
    <div class="atlas-top">
      <div class="atlas-brand"><span class="atlas-glyph"><Icon name="hub" :size="19" /></span><div><span class="atlas-overline">LOCALLEAF / STUDIO</span><h2>Reference Atlas</h2></div></div>
      <button class="atlas-icon" aria-label="Close Reference Atlas" title="Close" @click="$emit('close')"><Icon name="close" :size="19" /></button>
    </div>
    <div class="atlas-scroll">
      <div class="atlas-intro"><div class="atlas-live"><span class="live-dot"></span> LIVE PROJECT INDEX</div><p>See how your document connects.</p><span>References & citations, mapped as you write.</span></div>
      <div class="atlas-stats" aria-label="Project reference statistics">
        <div><strong>{{ index.definitions.size }}</strong><span>LABELS</span></div>
        <div><strong>{{ references }}</strong><span>USES</span></div>
        <div><strong :class="index.missing.length || index.duplicates.length ? 'warm' : ''">{{ index.missing.length + index.duplicates.length }}</strong><span>TO REVIEW</span></div>
      </div>
      <section class="atlas-section">
        <div class="section-heading"><span>AT CURSOR</span><span class="subtle">{{ state.openPath ? basename(state.openPath) : 'No file' }}</span></div>
        <template v-if="selected">
          <div class="selected-card"><div class="selected-kind">{{ label(selected) }}</div><div class="selected-key">{{ selected.key }}</div><div class="selected-count">{{ relations.length }} {{ relations.length === 1 ? 'location' : 'locations' }} in indexed files</div></div>
          <div class="relation-list">
            <button v-for="item in relations" :key="`${item.file}:${item.from}:${item.kind}`" class="relation" @click="go(item)">
              <span class="relation-dot" :class="item.kind === 'label' || item.kind === 'bibentry' ? 'definition' : ''"></span>
              <span class="relation-main"><strong>{{ label(item) }}</strong><small>{{ item.file }} · line {{ item.line }}</small></span>
              <Icon name="arrow_outward" :size="15" />
            </button>
          </div>
        </template>
        <div v-else class="cursor-empty"><Icon name="near_me" :size="19" /><span>Place your cursor on a label, reference, or citation key to follow its connections.</span></div>
      </section>
      <section class="atlas-section">
        <div class="section-heading"><span>NEEDS ATTENTION</span><span class="count-badge">{{ index.missing.length + index.duplicates.length }}</span></div>
        <label class="atlas-search"><Icon name="search" :size="17" /><input v-model="query" aria-label="Filter references" placeholder="Filter keys or files" /></label>
        <div v-if="!issues.length && !duplicates.length" class="empty-state"><span class="empty-check">✓</span><strong>{{ query ? 'No matching items' : 'No missing keys found' }}</strong><p>{{ query ? 'Try another key or file name.' : 'Your indexed literal references are connected.' }}</p></div>
        <div v-else class="issue-list">
          <button v-for="item in issues" :key="`${item.file}:${item.from}:${item.kind}`" class="issue" @click="go(item)">
            <span class="issue-mark">!</span><span class="issue-body"><strong>{{ item.key }}</strong><small>{{ item.kind === 'citation' ? 'Citation' : 'Reference' }} not found in indexed files</small><em>{{ item.file }} : {{ item.line }}</em></span><Icon name="arrow_outward" :size="15" />
          </button>
          <div v-for="group in duplicates" :key="group[0].key" class="duplicate-group"><div class="duplicate-head"><span class="issue-mark">≠</span><span><strong>{{ group[0].key }}</strong><small>Duplicate label · {{ group.length }} definitions</small></span></div><button v-for="item in group" :key="`${item.file}:${item.from}`" class="duplicate-place" @click="go(item)">{{ item.file }} : {{ item.line }} <Icon name="arrow_outward" :size="14" /></button></div>
        </div>
      </section>
      <p class="atlas-note">Scans {{ index.fileCount }} indexed TeX/Bib files. Literal keys only; macros, external bibliographies and TeX conditionals may change the result. Compile for authoritative diagnostics.</p>
    </div>
    <button class="build-strip" @click="openLogs"><span class="build-dot" :class="{ failed: buildCount > 0 || !!state.compileError, working: state.compiling }"></span><span><strong>{{ buildLabel }}</strong><small>{{ state.result ? `${state.result.engine} · ${buildCount} errors · last compile` : 'Open compile logs' }}</small></span><Icon name="open_in_new" :size="17" /></button>
  </aside>
</template>

<style scoped>
.atlas { --ink:#e8f2ee; --muted:#aabdb3; --line:#32483d; --mint:#82dca8; display:flex; flex:0 0 clamp(280px, 26vw, 350px); min-width:0; flex-direction:column; overflow:hidden; background:#14231d; color:var(--ink); border-left:1px solid #3a5144; font-family:var(--font-sans); }
.atlas-top { min-height:68px; display:flex; align-items:center; justify-content:space-between; padding:12px 16px; border-bottom:1px solid var(--line); }
.atlas-brand { display:flex; align-items:center; gap:10px; min-width:0; }
.atlas-glyph { width:36px; height:36px; display:grid; place-items:center; color:#b5f1cc; background:#28513a; border:1px solid #427457; border-radius:9px; }
.atlas-overline { display:block; color:#a2bbac; font-size:10px; letter-spacing:.11em; font-weight:700; }
.atlas h2 { font-size:15px; line-height:1.35; margin:2px 0 0; font-weight:700; }
.atlas-icon { display:grid; place-items:center; width:36px; height:36px; color:var(--muted); border-radius:7px; cursor:pointer; }.atlas-icon:hover { color:white; background:#30493b; }
.atlas-scroll { overflow:auto; min-height:0; flex:1; }.atlas-intro { padding:22px 17px 16px; }.atlas-live { color:#8fd9ad; font-size:10px; letter-spacing:.1em; font-weight:700; }.live-dot,.build-dot { display:inline-block; width:7px;height:7px;background:#7ed6a1;border-radius:50%;margin-right:5px; }.atlas-intro p { margin:9px 0 3px; font-size:17px;font-weight:700;letter-spacing:-.02em;line-height:1.3; }.atlas-intro>span { font-size:12px;color:var(--muted); }
.atlas-stats { display:grid;grid-template-columns:repeat(3,1fr);margin:0 16px 6px;border:1px solid var(--line);border-radius:9px;background:#1d3025; }.atlas-stats div { padding:12px 10px; }.atlas-stats div+div { border-left:1px solid var(--line); }.atlas-stats strong { display:block;font-size:20px;line-height:1.2; }.atlas-stats strong.warm { color:#f2ba7b; }.atlas-stats span { display:block;margin-top:5px;font-size:9px;letter-spacing:.08em;color:var(--muted);font-weight:700; }
.atlas-section { padding:19px 16px 0; }.section-heading { display:flex;align-items:center;justify-content:space-between;gap:8px;color:#a8c0b2;font-size:10px;letter-spacing:.1em;font-weight:700; }.subtle { color:var(--muted);text-transform:none;letter-spacing:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:130px;font-size:11px;font-weight:400; }.count-badge { color:#b7e8cb;background:#294b37;border-radius:5px;min-width:22px;text-align:center;padding:2px 5px;letter-spacing:0; }
.cursor-empty { display:flex;gap:10px;margin-top:10px;padding:14px 12px;border:1px dashed #496150;border-radius:8px;color:#b6c8bc;font-size:12px;line-height:1.5; }.cursor-empty .material-symbols-outlined { color:#89c9a0;flex-shrink:0; }.selected-card { margin-top:10px;padding:13px;border:1px solid #42664f;border-radius:8px;background:#233c2c; }.selected-kind { color:#9fd9b6;font-size:10px;letter-spacing:.09em;text-transform:uppercase;font-weight:700; }.selected-key { font-family:var(--font-mono);font-size:15px;color:#f1f9f3;word-break:break-all;margin-top:5px; }.selected-count { color:#b2caba;font-size:11px;margin-top:5px; }.relation-list { margin-top:6px; }.relation { width:100%;display:flex;align-items:center;gap:9px;text-align:left;padding:9px 7px;color:#b7cbbd;border-radius:6px;cursor:pointer; }.relation:hover,.issue:hover,.duplicate-place:hover { background:#273e30; }.relation-dot { width:8px;height:8px;flex:none;border:1px solid #98b9a4;border-radius:50%; }.relation-dot.definition { background:#7ed6a1;border-color:#7ed6a1; }.relation-main { flex:1;min-width:0; }.relation-main strong,.relation-main small { display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }.relation-main strong { font-size:12px;color:#e3eee7; }.relation-main small { font-size:11px;margin-top:2px; }
.atlas-search { display:flex;align-items:center;gap:7px;margin:11px 0 8px;padding:0 10px;min-height:38px;border:1px solid #45604e;border-radius:7px;color:#a7c2af; }.atlas-search:focus-within { outline:2px solid #78c895;outline-offset:1px; }.atlas-search input { min-width:0;width:100%;outline:0;background:transparent;color:white;font-size:12px; }.atlas-search input::placeholder { color:#a2b9aa; }.empty-state { padding:17px 8px;text-align:center;color:#c6d6cb; }.empty-check { display:grid;place-items:center;margin:0 auto 9px;border:1px solid #50775b;color:#a4e2b7;border-radius:50%;width:27px;height:27px; }.empty-state strong { font-size:12px; }.empty-state p { margin:5px 0 0;color:var(--muted);font-size:11px;line-height:1.5; }
.issue,.duplicate-head { display:flex;align-items:flex-start;gap:9px;text-align:left;padding:10px 7px;width:100%;border-radius:7px; }.issue { color:#b4c8b8;cursor:pointer; }.issue-mark { display:grid;place-items:center;flex:none;width:20px;height:20px;border-radius:5px;background:#5c4428;color:#f6cb92;font-size:12px;font-weight:700; }.issue-body { flex:1;min-width:0; }.issue-body strong,.duplicate-head strong { display:block;font-family:var(--font-mono);font-size:12px;color:#f5f7ec;word-break:break-all; }.issue-body small,.duplicate-head small { display:block;color:#f0c795;font-size:11px;line-height:1.35;margin-top:3px; }.issue-body em { display:block;color:#aec3b5;font-style:normal;font-size:11px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;margin-top:5px; }.duplicate-group { margin-top:3px;border-top:1px solid var(--line); }.duplicate-place { display:flex;justify-content:space-between;width:100%;padding:7px 10px 7px 36px;text-align:left;color:#c3d6c8;font-size:11px;cursor:pointer; }
.atlas-note { margin:16px;color:#a5b9aa;font-size:11px;line-height:1.5; }.build-strip { display:flex;align-items:center;gap:9px;text-align:left;min-height:58px;padding:10px 16px;border-top:1px solid var(--line);background:#1d3025;color:#e2f0e5;cursor:pointer; }.build-strip:hover { background:#294635; }.build-strip>span:nth-child(2) { flex:1;min-width:0; }.build-strip strong,.build-strip small { display:block; }.build-strip strong { font-size:12px; }.build-strip small { margin-top:2px;font-size:10px;color:#b3c7b8; }.build-dot.failed { background:#f1a568; }.build-dot.working { background:#e5c777; }
.atlas button:focus-visible { outline:2px solid #94e1ae;outline-offset:-2px; }@media(max-width:850px){.atlas{position:absolute;z-index:35;right:0;top:0;bottom:0;width:min(350px, 100%);box-shadow:-12px 0 32px #0005}}
</style>
