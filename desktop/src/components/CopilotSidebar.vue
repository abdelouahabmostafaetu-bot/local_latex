<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { editorBridge, state } from '../lib/store';
import { readStored, writeStored } from '../lib/storage';
import Icon from './Icon.vue';
import AiDiffReview from './AiDiffReview.vue';
import type { AiReviewSnapshot } from '../lib/state';
defineEmits<{ close: [] }>();
interface Auth { signedIn: boolean; login: string | null; authType: string | null; message: string | null }
interface Model { id: string; name: string }
interface Device { userCode: string; verificationUri: string; expiresIn: number }
interface Message { role: 'user' | 'assistant'; text: string }
interface Segment { type: 'text' | 'code'; text: string; language?: string }
const auth = ref<Auth | null>(null), models = ref<Model[]>([]), model = ref('');
const clientId = ref(readStored('copilotClientId') ?? '');
const device = ref<Device | null>(null), error = ref(''), notice = ref('');
const busy = ref(false), checking = ref(false), includeFile = ref(false), includeSelection = ref(false), allowReadTools = ref(false), prompt = ref('');
const review = ref<AiReviewSnapshot | null>(null);
const messages = ref<Message[]>([]), unlisteners: UnlistenFn[] = [];
const canSend = computed(() => !!auth.value?.signedIn && !busy.value && !!prompt.value.trim());
function textOf(e: unknown) { return e instanceof Error ? e.message : String(e); }
async function refresh() {
  checking.value = true; error.value = '';
  try { auth.value = await invoke<Auth>('copilot_auth_status'); models.value = auth.value.signedIn ? await invoke<Model[]>('copilot_list_models') : []; if (model.value && !models.value.some(m => m.id === model.value)) model.value = ''; }
  catch (e) { auth.value = null; error.value = textOf(e); } finally { checking.value = false; }
}
async function login() {
  if (!clientId.value.trim()) { error.value = 'Enter a GitHub OAuth App client ID with device flow enabled, or use the Copilot CLI instructions below.'; return; }
  error.value = ''; writeStored('copilotClientId', clientId.value.trim());
  try { device.value = await invoke<Device>('copilot_github_login', { clientId: clientId.value.trim() }); } catch (e) { error.value = textOf(e); }
}
async function cancelLogin() { await invoke('copilot_cancel_login'); device.value = null; }
async function useCli() { try { await invoke('copilot_use_cli_credentials'); device.value = null; await refresh(); } catch(e) { error.value = textOf(e); } }
async function logout() { try { await invoke('copilot_logout'); auth.value = {signedIn:false,login:null,authType:null,message:null}; models.value = []; messages.value = []; device.value = null; busy.value = false; notice.value = 'Signed out of LocalLeaf. Environment and CLI credentials are not removed.'; } catch(e) { error.value = textOf(e); } }
async function reset() { busy.value = true; try { await invoke('copilot_reset_session'); messages.value = []; error.value = ''; } catch(e) { error.value = textOf(e); } finally { busy.value = false; } }
async function stop() { try { await invoke('copilot_stop'); } catch(e) { error.value = textOf(e); } }
async function send(raw?: string) {
  const text = (raw ?? prompt.value).trim(); if (!text || busy.value || !auth.value?.signedIn) return;
  const context: string[] = [];
  if (includeFile.value && state.openKind === 'text' && state.openPath) context.push(`Active file: ${state.openPath}\n\`\`\`latex\n${state.content}\n\`\`\``);
  const selection = editorBridge.selectedText?.() ?? '';
  if (includeSelection.value && selection) context.push(`Selected text in ${state.openPath}:\n\`\`\`latex\n${selection}\n\`\`\``);
  const full = [text, ...context].join('\n\n'); messages.value.push({role:'user',text},{role:'assistant',text:''}); prompt.value = ''; error.value = ''; busy.value = true;
  try { await invoke('copilot_send_message', { prompt: full, model: model.value || null, allowReadTools: allowReadTools.value }); }
  catch(e) { error.value = textOf(e); busy.value = false; if (!messages.value.at(-1)?.text) messages.value.pop(); }
}
function fixError() {
  const diagnostic = state.result?.errors[0]; if (!diagnostic && !state.compileError) { error.value = 'There is no compile error to send.'; return; }
  const path = diagnostic?.file ?? state.mainDocument ?? state.openPath, source = path === state.openPath ? state.content : path ? state.texts[path] : '', line = diagnostic?.line ?? null;
  const surrounding = source && line ? source.split('\n').slice(Math.max(0,line - 6), line + 5).map((s,i) => `${Math.max(1,line-5)+i}: ${s}`).join('\n') : '';
  void send(`Help fix this LaTeX compilation error. Explain the cause and suggest a minimal fix.\nFile: ${path ?? 'unknown'}${line ? `, line ${line}` : ''}\nError: ${diagnostic?.message ?? state.compileError}${surrounding ? `\nSurrounding lines:\n\`\`\`latex\n${surrounding}\n\`\`\`` : ''}`);
}
function segments(text: string): Segment[] {
  const parts: Segment[] = [], re = /```([^\n`]*)\n([\s\S]*?)```/g; let start = 0, match: RegExpExecArray | null;
  while ((match = re.exec(text))) { if (match.index > start) parts.push({ type:'text', text:text.slice(start,match.index) }); parts.push({type:'code',language:match[1].trim().toLowerCase(),text:match[2]}); start = re.lastIndex; }
  if (start < text.length) parts.push({type:'text',text:text.slice(start)}); return parts;
}
async function copy(text: string) {
  try {
    if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(text);
    else { const el = document.createElement('textarea'); el.value = text; el.style.position = 'fixed'; el.style.opacity = '0'; document.body.appendChild(el); el.select(); const ok = document.execCommand('copy'); el.remove(); if (!ok) throw new Error('Clipboard unavailable'); }
    notice.value = 'Code copied.';
  } catch { error.value = 'Could not access the clipboard.'; }
}
function startReview(text: string) {
  const snapshot = editorBridge.prepareAiReview?.(text);
  if (!snapshot) { error.value = 'Open a text file with a single selection before reviewing.'; return; }
  if (snapshot.original === snapshot.proposed) { notice.value = 'The suggestion makes no changes.'; return; }
  review.value = snapshot;
}
function applyReview(result: string) {
  if (!review.value || !editorBridge.applyAiReview?.(review.value, result)) {
    error.value = 'The file or editor changed during review. Open the suggestion again to review the current version.';
    review.value = null; return;
  }
  notice.value = 'Reviewed changes applied. Undo is available; your existing autosave settings apply.';
  review.value = null;
}
function apply(text: string, mode: 'insert' | 'replace') { if (state.openKind !== 'text' || !editorBridge.applyText?.(text, mode)) { error.value = mode === 'replace' ? 'Select text in the editor first.' : 'Open a text file first.'; return; } notice.value = 'Applied to the editor. Undo is available.'; }
onMounted(async () => {
  unlisteners.push(await listen<string>('copilot:chunk', e => { const last = messages.value.at(-1); if (busy.value && last?.role === 'assistant') last.text += e.payload; }));
  unlisteners.push(await listen('copilot:done', () => { busy.value = false; }));
  unlisteners.push(await listen<string>('copilot:error', e => { error.value = e.payload; busy.value = false; }));
  unlisteners.push(await listen('copilot:login-complete', () => { device.value = null; void refresh(); }));
  unlisteners.push(await listen<string>('copilot:login-error', e => { device.value = null; error.value = e.payload; })); await refresh();
});
onBeforeUnmount(() => unlisteners.forEach(f => f()));
watch(() => state.info?.root, (next, prev) => { if (next !== prev) void reset(); });
watch(() => state.ui.aiDiagnostic, (item) => {
  if (!item) return;
  const path = item.file ?? state.mainDocument ?? state.openPath;
  const source = path === state.openPath ? state.content : path ? (state.texts[path] ?? '') : '';
  const line = item.line;
  const surrounding = source && line ? source.split('\n').slice(Math.max(0, line - 6), line + 5).map((text, i) => `${Math.max(1, line - 5) + i}: ${text}`).join('\n') : '';
  const main = state.mainDocument === state.openPath ? state.content : state.mainDocument ? (state.texts[state.mainDocument] ?? '') : '';
  const preambleEnd = main.indexOf('\\begin{document}');
  const preamble = preambleEnd >= 0 ? main.slice(0, preambleEnd).slice(0, 16000) : '';
  const request = `Help fix this LaTeX ${item.kind}. Explain the cause and suggest a minimal fix.\nFile: ${path ?? 'unknown'}${line ? `, line ${line}` : ''}\nDiagnostic: ${item.message}${surrounding ? `\nSurrounding lines:\n\`\`\`latex\n${surrounding}\n\`\`\`` : ''}${preamble ? `\nMain document preamble${preambleEnd > 16000 ? ' (truncated)' : ''}:\n\`\`\`latex\n${preamble}\n\`\`\`` : ''}`;
  state.ui.aiDiagnostic = null;
  if (auth.value?.signedIn && !busy.value) void send(request);
  else { prompt.value = request; notice.value = busy.value ? 'Diagnostic prepared. Send it after the current response finishes.' : 'Diagnostic prepared. Sign in to send it to Copilot.'; }
});

</script>
<template>
  <aside class="copilot" aria-label="GitHub Copilot">
    <header class="top"><div class="brand"><span class="brand-icon"><Icon name="forum" :size="19" /></span><div><small>LOCALLEAF / ASSISTANT</small><h2>GitHub Copilot</h2></div></div><button class="icon-button" aria-label="Close Copilot" @click="$emit('close')"><Icon name="close" :size="19" /></button></header>
    <div class="scroll">
      <div class="account"><div><span class="status-dot" :class="{online:auth?.signedIn}"></span><strong>{{ checking ? 'Checking connection…' : auth?.signedIn ? `Signed in${auth.login ? ` · ${auth.login}` : ''}` : 'Not signed in' }}</strong></div><button v-if="auth?.signedIn" class="link" @click="logout">Sign out</button></div>
      <div v-if="!auth?.signedIn" class="signin"><p>Use your GitHub account and Copilot subscription. Your code is sent only when you choose to share it.</p><label>GitHub OAuth app client ID<input v-model="clientId" placeholder="Client ID (device flow enabled)" autocomplete="off" /></label><button class="primary full" @click="login">Sign in with GitHub</button><div v-if="device" class="device"><strong>Enter code {{ device.userCode }}</strong><p>Then authorize this app on GitHub. Code expires in {{ Math.floor(device.expiresIn / 60) }} minutes.</p><button class="secondary" @click="invoke('open_external', { url: device.verificationUri })">Open GitHub device login ↗</button> <button class="secondary" @click="cancelLogin">Cancel</button></div><div class="cli"><strong>Already signed in to Copilot CLI?</strong><p>In a terminal, run <code>copilot</code> and use <code>/login</code>. Then refresh the connection here.</p><button class="secondary" @click="useCli">Use CLI credentials / Refresh</button></div></div>
      <template v-else><div class="model"><label for="copilot-model">MODEL</label><select id="copilot-model" v-model="model" :disabled="busy" @change="reset"><option value="">Subscription default</option><option v-for="m in models" :key="m.id" :value="m.id">{{ m.name }}</option></select><small>Changing model starts a new conversation.</small></div><div v-if="!messages.length" class="welcome"><div class="welcome-icon"><Icon name="auto_awesome" :size="22" /></div><h3>A collaborator for your LaTeX</h3><p>Ask about an equation, improve an explanation, or investigate a build error.</p></div><div v-else class="messages" aria-live="polite"><article v-for="(message, i) in messages" :key="i" :class="['message', message.role]"><div class="message-label">{{ message.role === 'user' ? 'YOU' : 'COPILOT' }}</div><template v-for="(part, j) in segments(message.text)" :key="j"><p v-if="part.type === 'text'" class="message-text">{{ part.text }}</p><div v-else class="code"><div class="code-heading"><span>{{ part.language || 'code' }}</span><div v-if="['latex','tex'].includes(part.language || '')"><button @click="copy(part.text)">Copy</button><button @click="startReview(part.text)">Review diff</button><button @click="apply(part.text,'insert')">Insert at cursor</button><button @click="apply(part.text,'replace')">Replace selection</button></div><button v-else @click="copy(part.text)">Copy</button></div><pre><code>{{ part.text }}</code></pre></div></template><span v-if="busy && i === messages.length-1 && !message.text" class="typing">Thinking…</span></article></div></template>
      <div v-if="error" class="alert" role="alert">{{ error }}</div><div v-if="notice" class="notice" role="status">{{ notice }} <button @click="notice = ''" aria-label="Dismiss">×</button></div>
    </div>
    <div v-if="auth?.signedIn" class="composer"><button class="quick" :disabled="busy || (!state.result?.errors.length && !state.compileError)" @click="fixError"><Icon name="build" :size="16" /> Fix last compile error</button><textarea v-model="prompt" rows="3" aria-label="Message Copilot" placeholder="Ask about your document…" @keydown.ctrl.enter.prevent="send()" @keydown.meta.enter.prevent="send()"></textarea><div class="context"><label><input v-model="includeFile" type="checkbox" /> Include current file</label><label><input v-model="includeSelection" type="checkbox" /> Include selection</label></div><label class="tool-access"><input v-model="allowReadTools" type="checkbox" :disabled="busy" @change="reset" /> Allow Copilot to read project files through MCP</label><p class="privacy">Copilot receives what you send. MCP writes are disabled. Changing tool access starts a new chat.</p><div class="actions"><button class="secondary" @click="reset">New chat</button><button v-if="busy" class="secondary" @click="stop">Stop</button><button v-else class="primary" :disabled="!canSend" @click="send()">Send ↑</button></div></div>
    <AiDiffReview v-if="review" :snapshot="review" @close="review = null" @apply="applyReview" />
  </aside>
</template>
<style scoped>
.copilot{--line:#d9e5dc;--muted:#5f7569;display:flex;flex:0 0 clamp(320px,29vw,390px);min-width:0;flex-direction:column;background:#f8fbf8;border-left:1px solid #d7e3da;color:#183327;font-family:var(--font-sans)}.top{min-height:68px;padding:12px 16px;display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid var(--line);background:#fff}.brand{display:flex;align-items:center;gap:10px}.brand-icon{display:grid;place-items:center;width:36px;height:36px;border-radius:9px;background:#dcefe3;color:#17663d}.brand small{font-size:10px;letter-spacing:.11em;font-weight:700;color:#688575}.brand h2{margin:2px 0 0;font-size:15px;font-weight:700}.icon-button{width:36px;height:36px;display:grid;place-items:center;border-radius:7px;color:#5d7568;cursor:pointer}.icon-button:hover{background:#e8f1eb}.scroll{overflow-y:auto;min-height:0;flex:1}.account{display:flex;justify-content:space-between;align-items:center;padding:16px;border-bottom:1px solid var(--line);font-size:12px}.status-dot{display:inline-block;width:8px;height:8px;border-radius:50%;background:#b5c3b9;margin-right:7px}.status-dot.online{background:#35a66a}.link{color:#26704a;font-size:12px;text-decoration:underline;cursor:pointer}.signin{padding:16px}.signin p,.cli p,.device p,.welcome p{font-size:12px;line-height:1.55;color:var(--muted);margin:0 0 14px}.signin label,.model label{display:block;font-size:11px;letter-spacing:.04em;font-weight:700;color:#496456}.signin input,.model select,.composer textarea{width:100%;border:1px solid #c8d9cd;background:#fff;border-radius:7px;padding:10px;color:#203a2d;outline:none;margin-top:6px;font-size:12px}.signin input:focus,.model select:focus,.composer textarea:focus{outline:2px solid #79bf93;outline-offset:1px}.primary,.secondary{border-radius:7px;padding:8px 11px;font-size:12px;font-weight:700;cursor:pointer}.primary{background:#12653d;color:#fff}.primary:hover{background:#0d4e30}.primary:disabled,.secondary:disabled,.quick:disabled{opacity:.45;cursor:not-allowed}.secondary{background:#fff;border:1px solid #c8d9cd;color:#315c43}.secondary:hover{background:#eaf4ed}.full{width:100%;margin-top:10px}.device,.cli{border:1px solid var(--line);border-radius:8px;padding:12px;margin-top:16px;font-size:12px}.device{background:#e9f7ec}.device p,.cli p{margin:7px 0 10px}.model{padding:14px 16px;border-bottom:1px solid var(--line)}.model select{margin-top:6px}.model small{display:block;font-size:10px;margin-top:5px;color:var(--muted)}.welcome{text-align:center;padding:42px 22px}.welcome-icon{display:grid;place-items:center;margin:0 auto 13px;width:44px;height:44px;border-radius:12px;background:#e3f2e7;color:#237c4b}.welcome h3{font-size:15px;margin:0 0 8px}.messages{padding:16px;display:flex;flex-direction:column;gap:18px}.message-label{font-size:10px;font-weight:700;letter-spacing:.1em;color:#63806d;margin-bottom:7px}.message-text{font-size:13px;line-height:1.55;white-space:pre-wrap;overflow-wrap:anywhere;margin:0}.message.user{background:#e7f2e9;padding:11px;border-radius:9px}.code{margin:10px 0;border:1px solid #d5e2d9;border-radius:7px;overflow:hidden;background:#fff}.code-heading{display:flex;align-items:center;justify-content:space-between;gap:5px;flex-wrap:wrap;padding:6px 8px;background:#eef5ef;font-size:10px;color:#4f6f5d}.code-heading div{display:flex;gap:8px;flex-wrap:wrap}.code-heading button{font-size:10px;color:#176840;cursor:pointer;text-decoration:underline}.code pre{padding:10px;overflow:auto;white-space:pre;font-size:11px;line-height:1.5}.typing{font-size:12px;color:var(--muted)}.alert,.notice{margin:12px 16px;padding:10px;border-radius:7px;font-size:12px;line-height:1.4;overflow-wrap:anywhere}.alert{background:#fff2e9;color:#8d3c20;border:1px solid #efcdbb}.notice{background:#e7f5ea;color:#205f39}.notice button{float:right}.composer{border-top:1px solid var(--line);padding:12px 16px;background:#fff}.quick{display:flex;gap:5px;align-items:center;color:#246b43;font-size:12px;font-weight:700;cursor:pointer}.composer textarea{resize:vertical;min-height:64px;font-family:inherit}.context{display:flex;gap:8px 14px;flex-wrap:wrap;margin-top:8px;font-size:11px;color:#496456}.context label{display:flex;align-items:center;gap:5px}.context input{accent-color:#167344}.tool-access{display:flex;align-items:center;gap:5px;margin-top:8px;font-size:11px;color:#496456}.tool-access input{accent-color:#167344}.privacy{font-size:10px;line-height:1.4;color:#6b7f72;margin:9px 0}.actions{display:flex;justify-content:space-between;gap:8px}.copilot button:focus-visible{outline:2px solid #4eae77;outline-offset:2px}@media(max-width:1100px){.copilot{position:absolute;z-index:35;right:0;top:0;bottom:0;width:min(390px,100%);box-shadow:-12px 0 32px #0003}}
</style>
