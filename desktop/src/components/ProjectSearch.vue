<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { errorMessage, ipc, type SearchHit, type SearchReport } from "../lib/ipc";
import { gotoLocation, session, state } from "../lib/store";
import Modal from "./Modal.vue";

const emit = defineEmits<{ close: [] }>();
const query = ref("");
const regex = ref(false);
const caseSensitive = ref(false);
const busy = ref(false);
const error = ref("");
const report = ref<SearchReport | null>(null);
const field = ref<HTMLInputElement>();
let timer: ReturnType<typeof setTimeout> | undefined;
let sequence = 0;
let disposed = false;
let cancellation = Promise.resolve();
function cancelPrevious() {
  cancellation = cancellation.then(() => ipc.cancelProjectSearch()).catch(() => {});
  return cancellation;
}

function schedule() {
  const current = ++sequence;
  clearTimeout(timer);
  error.value = "";
  report.value = null;
  const cancelled = cancelPrevious();
  if (!query.value) { busy.value = false; return; }
  busy.value = true;
  const generation = session.generation;
  const text = query.value;
  const regexMode = regex.value;
  const sensitive = caseSensitive.value;
  const overlay = state.openPath && state.openKind === "text" ? { path: state.openPath, content: state.content } : null;
  timer = setTimeout(async () => {
    try {
      await cancelled;
      if (disposed || current !== sequence || generation !== session.generation) return;
      const found = await ipc.searchProject(text, regexMode, sensitive, overlay);
      if (!disposed && current === sequence && generation === session.generation) report.value = found;
    } catch (e) {
      if (!disposed && current === sequence && generation === session.generation) error.value = errorMessage(e);
    } finally {
      if (!disposed && current === sequence) busy.value = false;
    }
  }, 250);
}
watch(() => [query.value, regex.value, caseSensitive.value, state.openPath, state.content], schedule);
onMounted(async () => { await nextTick(); field.value?.focus(); });
onBeforeUnmount(() => { disposed = true; sequence++; clearTimeout(timer); void cancelPrevious(); });

async function openHit(hit: SearchHit) {
  emit("close");
  if (state.ui.layout === "pdf") state.ui.layout = "split";
  await nextTick();
  await gotoLocation(hit.file, hit.line, hit.column);
}
</script>

<template>
  <Modal title="Search project" width="46rem" @close="emit('close')">
    <div class="space-y-3 p-5">
      <label class="block text-sm font-semibold" for="project-search-query">Find in project</label>
      <input id="project-search-query" ref="field" v-model="query" class="search-input" placeholder="Search text or a regular expression" autocomplete="off" spellcheck="false" />
      <div class="flex flex-wrap items-center gap-x-5 gap-y-2 text-sm">
        <label class="flex items-center gap-2"><input v-model="regex" type="checkbox" /> Regular expression</label>
        <label class="flex items-center gap-2"><input v-model="caseSensitive" type="checkbox" /> Match case</label>
      </div>
      <p class="text-xs text-surface-muted">Local search · searches the active editor buffer and supported project text files. Internal folders and symlinks are excluded.</p>
      <p v-if="error" role="alert" class="rounded border border-danger/40 p-3 text-sm text-danger">{{ error }}</p>
      <p v-else-if="busy" role="status" class="text-sm text-surface-muted">Searching…</p>
      <template v-else-if="report">
        <p role="status" class="text-xs text-surface-muted">
          {{ report.hits.length }} matching lines in {{ report.filesSearched }} searched files<span v-if="report.truncated"> · limit reached; refine your search</span><span v-if="report.skipped"> · {{ report.skipped }} inaccessible, binary, or oversized entries skipped</span>
        </p>
        <p v-if="!report.hits.length" class="py-6 text-center text-sm text-surface-muted">No matches found.</p>
        <ol v-else class="search-results" aria-label="Search results">
          <li v-for="hit in report.hits" :key="`${hit.file}:${hit.line}`">
            <button class="result" @click="openHit(hit)">
              <span class="result-path">{{ hit.file }} <span class="text-surface-muted">: {{ hit.line }}:{{ hit.column }}</span></span>
              <span class="result-preview">{{ hit.preview }}</span>
            </button>
          </li>
        </ol>
      </template>
      <p v-else class="py-6 text-center text-sm text-surface-muted">Enter text to search the project.</p>
    </div>
  </Modal>
</template>

<style scoped>
@reference "../style.css";
.search-input { @apply w-full rounded border border-surface-border bg-surface px-3 py-2 text-sm text-surface-fg outline-none focus:border-accent; }
.search-results { @apply max-h-[45vh] overflow-auto rounded border border-surface-border; }
.result { @apply flex w-full flex-col gap-1 border-b border-surface-border px-3 py-2 text-left hover:bg-surface-hover focus-visible:outline-2 focus-visible:outline-accent; }
.result-path { @apply text-sm font-semibold text-accent; }
.result-preview { @apply w-full overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs text-surface-muted; }
</style>
