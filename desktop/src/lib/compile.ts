// Compilation: running the TeX engine, loading the PDF, diagnostics and auto-compile.
import { ipc, errorMessage, type Diagnostic } from "./ipc";
import { basename } from "./latex";
import { guarded, notify, recentlyRan, session, state } from "./state";
import { flushSave } from "./files";

// ---------------------------------------------------------------------------
// Compilation
// ---------------------------------------------------------------------------

/** `queued` marks the internal re-run of a request that arrived mid-compile (never deduplicated). */
let autoCompileTimer: ReturnType<typeof setTimeout> | undefined;
const AUTO_COMPILE_DELAY = 700;
let revision = 0;
let sequence = 0;
let activeRequest: string | null = null;

/** A new edit invalidates the previous result immediately, not only after the debounce. */
export function scheduleAutoCompile() {
  revision++;
  state.compileQueued = false;
  if (activeRequest) void ipc.cancelCompile(activeRequest).catch(() => {});
  clearTimeout(autoCompileTimer);
  autoCompileTimer = setTimeout(() => void compile(true), AUTO_COMPILE_DELAY);
}

export function cancelAutoCompile() {
  clearTimeout(autoCompileTimer);
  revision++;
  state.compileQueued = false;
  if (activeRequest) void ipc.cancelCompile(activeRequest).catch(() => {});
}

export async function compile(queued = false) {
  clearTimeout(autoCompileTimer);
  if (!state.info || (!queued && recentlyRan("compile"))) return;
  if (!state.mainDocument) {
    notify("No main document: create a .tex file or choose one in Settings → Compiler", "error");
    return;
  }
  if (state.compiling) {
    state.compileQueued = true;
    return;
  }
  const gen = session.generation;
  const requestRevision = revision;
  state.compiling = true;
  state.compileError = null;
  try {
    await flushSave();
    if (gen === session.generation && requestRevision === revision && state.mainDocument) {
      const requestId = `${Date.now()}-${++sequence}`;
      activeRequest = requestId;
      const result = await ipc.compile(state.mainDocument, state.engine, {
        draft: state.draft,
        stop_on_first_error: state.stopOnFirstError,
      }, requestId);
      if (gen === session.generation && requestRevision === revision) {
        state.result = result;
        state.engine = result.engine;
        if (result.success && result.pdf_path) {
          state.pdfPath = result.pdf_path;
          await reloadPdf(requestRevision);
        }
        if (!result.pdf_path) state.ui.showLogs = true;
      }
    }
  } catch (e) {
    if (gen === session.generation && requestRevision === revision) {
      state.result = null;
      state.compileError = errorMessage(e);
      state.ui.showLogs = true;
    }
  } finally {
    activeRequest = null;
    state.compiling = false;
    state.lastCompileAt = Date.now();
  }
  if (state.compileQueued) {
    state.compileQueued = false;
    void compile(true);
  }
}

export async function recompileFromScratch() {
  await guarded(() => ipc.clearBuildDir());
  await compile(true);
}

export async function reloadPdf(expectedRevision?: number) {
  const path = state.pdfPath ?? (state.mainDocument ? `.texcraft/build/${basename(state.mainDocument).replace(/\.tex$/, "")}.pdf` : null);
  if (!path) return;
  const gen = session.generation;
  const data = await ipc.readBinary(path).catch(() => null);
  if (data && gen === session.generation && (expectedRevision === undefined || expectedRevision === revision)) {
    state.pdfPath = path;
    state.pdfData = data;
    state.pdfVersion++;
  }
}

export function diagnostics(): { errors: Diagnostic[]; warnings: Diagnostic[]; badBoxes: Diagnostic[] } {
  const r = state.result;
  return {
    errors: r?.errors ?? [],
    warnings: r?.warnings.filter((w) => w.kind === "warning") ?? [],
    badBoxes: r?.warnings.filter((w) => w.kind === "bad_box") ?? [],
  };
}

