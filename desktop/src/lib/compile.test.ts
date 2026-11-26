import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  compile: vi.fn(), cancelCompile: vi.fn(), readBinary: vi.fn(), flushSave: vi.fn(),
  state: {
    info: {}, mainDocument: "main.tex", compiling: false, compileQueued: false,
    compileError: null, result: null, pdfPath: null, pdfData: null, pdfVersion: 0,
    texts: { "main.tex": "" }, engines: [], engine: "pdflatex", draft: false,
    stopOnFirstError: false, lastCompileAt: 0, ui: { showLogs: false },
  },
  session: { generation: 1 },
}));
vi.mock("./ipc", () => ({ ipc: { compile: mocks.compile, cancelCompile: mocks.cancelCompile, readBinary: mocks.readBinary }, errorMessage: String }));
vi.mock("./state", () => ({ state: mocks.state, session: mocks.session, notify: vi.fn(), guarded: vi.fn(), recentlyRan: () => false }));
vi.mock("./files", () => ({ flushSave: mocks.flushSave }));
import { cancelAutoCompile, compile, scheduleAutoCompile } from "./compile";

describe("auto compile cancellation", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    Object.assign(mocks.state, { compiling: false, compileQueued: false, result: null, compileError: null, pdfPath: null, pdfData: null });
    mocks.compile.mockReset(); mocks.cancelCompile.mockReset().mockResolvedValue(undefined);
    mocks.flushSave.mockReset().mockResolvedValue(undefined);
  });
  afterEach(() => { cancelAutoCompile(); vi.useRealTimers(); });
  it("cancels an active build and discards its stale result", async () => {
    let resolve!: (value: unknown) => void;
    mocks.compile.mockReturnValue(new Promise(r => { resolve = r; }));
    const running = compile();
    await vi.waitFor(() => expect(mocks.compile).toHaveBeenCalledOnce());
    const requestId = mocks.compile.mock.calls[0][3];
    scheduleAutoCompile();
    expect(mocks.cancelCompile).toHaveBeenCalledWith(requestId);
    resolve({ success: true, pdf_path: "old.pdf" });
    await running;
    expect(mocks.state.result).toBeNull();
    expect(mocks.state.pdfPath).toBeNull();
    expect(mocks.state.compileError).toBeNull();
  });
  it("does not start a build if an edit lands while saving", async () => {
    let saved!: () => void;
    mocks.flushSave.mockReturnValue(new Promise<void>(r => { saved = r; }));
    const running = compile();
    scheduleAutoCompile();
    saved();
    await running;
    expect(mocks.compile).not.toHaveBeenCalled();
  });
});
