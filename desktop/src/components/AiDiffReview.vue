<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { EditorState, Prec } from '@codemirror/state';
import { EditorView, keymap, lineNumbers } from '@codemirror/view';
import { getChunks, getOriginalDoc, unifiedMergeView } from '@codemirror/merge';
import type { AiReviewSnapshot } from '../lib/state';
import Modal from './Modal.vue';

const props = defineProps<{ snapshot: AiReviewSnapshot }>();
const emit = defineEmits<{ close: []; apply: [result: string] }>();
const host = ref<HTMLElement>();
const remaining = ref(0);
const undoCount = ref(0);
const history: { base: string; doc: string }[] = [];
let editor: EditorView | null = null;

function makeState(base: string, doc: string): EditorState {
  return EditorState.create({ doc, extensions: [
    lineNumbers(), EditorState.readOnly.of(true), EditorView.editable.of(false), EditorView.contentAttributes.of({ tabindex: "0", "aria-label": "Suggested changes; press Control or Command Z to undo a review decision" }), EditorView.lineWrapping,
    Prec.highest(keymap.of([{ key: 'Mod-z', run: () => undoDecision() }])),
    unifiedMergeView({ original: base, mergeControls: (type, action) => {
      const button = document.createElement('button');
      button.type = 'button'; button.className = `review-decision review-${type}`;
      button.textContent = type === 'accept' ? 'Accept' : 'Reject';
      button.setAttribute('aria-label', `${type === 'accept' ? 'Accept' : 'Reject'} this change`);
      button.addEventListener('click', (e) => {
        if (!editor) return;
        const before = { base: getOriginalDoc(editor.state).toString(), doc: editor.state.doc.toString() };
        action(e);
        if (before.base !== getOriginalDoc(editor.state).toString() || before.doc !== editor.state.doc.toString()) {
          history.push(before); undoCount.value = history.length;
        }
      });
      return button;
    }}),
    EditorView.updateListener.of(update => { remaining.value = getChunks(update.state)?.chunks.length ?? 0; }),
  ] });
}
function undoDecision(): boolean {
  if (!editor || !history.length) return false;
  const previous = history.pop()!;
  editor.setState(makeState(previous.base, previous.doc));
  remaining.value = getChunks(editor.state)?.chunks.length ?? 0;
  undoCount.value = history.length; editor.focus(); return true;
}
onMounted(() => {
  editor = new EditorView({ parent: host.value!, state: makeState(props.snapshot.original, props.snapshot.proposed) });
  remaining.value = getChunks(editor.state)?.chunks.length ?? 0;
});
onBeforeUnmount(() => { editor?.destroy(); editor = null; });
</script>
<template>
  <Modal title="Review Copilot changes" width="56rem" @close="emit('close')">
    <div class="review-wrap">
      <p class="review-description">Review the suggested replacement for the selected text, or insertion at the cursor. Accept or reject every change before applying. The original file stays unchanged until you apply.</p>
      <p class="review-path">{{ snapshot.path }} · {{ remaining }} change{{ remaining === 1 ? '' : 's' }} left to review</p>
      <div ref="host" class="review-editor" aria-label="Suggested changes" />
      <p class="review-description">Applying updates the editor and may trigger your existing autosave. Undo in the editor restores the applied changes.</p>
    </div>
    <template #footer>
      <button class="review-button" :disabled="!undoCount" @click="undoDecision">Undo decision</button>
      <button class="review-button" @click="emit('close')">Cancel</button>
      <button class="review-button review-primary" :disabled="remaining !== 0" @click="editor && emit('apply', editor.state.doc.toString())">Apply reviewed changes</button>
    </template>
  </Modal>
</template>
<style scoped>
.review-wrap{padding:16px 20px}.review-description{font-size:13px;line-height:1.5;color:var(--color-surface-muted,#667085);margin:0 0 12px}.review-path{font-size:12px;font-weight:600;margin:0 0 8px;overflow-wrap:anywhere}.review-editor{border:1px solid #91a99a;border-radius:8px;overflow:hidden;margin-bottom:12px;min-height:100px}.review-editor :deep(.cm-editor){max-height:48vh;font-size:13px;background:#fff;color:#1d3327}.review-editor :deep(.cm-scroller){overflow:auto;font-family:monospace}.review-editor :deep(.review-decision){border:1px solid #97b7a0;border-radius:5px;padding:3px 9px;margin:3px;font:600 12px sans-serif;cursor:pointer;background:white;color:#1d5535}.review-editor :deep(.review-reject){color:#8e382e}.review-button{padding:8px 12px;border:1px solid #90aa98;border-radius:6px;font-size:12px;cursor:pointer}.review-button:disabled{opacity:.5;cursor:not-allowed}.review-primary{background:#12653d;color:white;border-color:#12653d}
</style>
