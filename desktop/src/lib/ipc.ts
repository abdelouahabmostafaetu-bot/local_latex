// Typed wrappers around the Tauri commands exposed by src-tauri/src/main.rs.
import { invoke } from "@tauri-apps/api/core";

export type Engine = "tectonic" | "pdflatex" | "xelatex" | "lualatex";

export const ENGINE_LABELS: Record<Engine, string> = {
  pdflatex: "pdfLaTeX",
  xelatex: "XeLaTeX",
  lualatex: "LuaLaTeX",
  tectonic: "Tectonic",
};

export interface FileNode {
  name: string;
  path: string;
  isDir: boolean;
  children?: FileNode[];
}

/** Per-project settings, stored in `.texcraft/settings.json` (shared with the MCP server). */
export interface ProjectSettings {
  displayName?: string | null;
  mainDocument?: string | null;
  engine?: Engine | null;
  autoCompile?: boolean | null;
  draft?: boolean | null;
  stopOnFirstError?: boolean | null;
}

export interface ProjectInfo {
  root: string;
  name: string;
  mainDocument: string | null;
  settings: ProjectSettings | null;
  tree: FileNode[];
  mcpCommand: string[];
}

export interface ToolInfo { name: string; path: string | null }

export interface EngineInfo {
  engine: Engine;
  installed: boolean;
  path: string | null;
}

export type DiagnosticKind = "error" | "warning" | "bad_box";

export interface Diagnostic {
  kind: DiagnosticKind;
  message: string;
  file: string | null;
  line: number | null;
}

export interface CompileResult {
  engine: Engine;
  success: boolean;
  exit_code: number | null;
  pdf_path: string | null;
  log_path: string | null;
  synctex_path: string | null;
  duration_ms: number;
  stdout: string;
  stderr: string;
  errors: Diagnostic[];
  warnings: Diagnostic[];
}

export interface CompileOptions {
  draft: boolean;
  stop_on_first_error: boolean;
}

export interface SearchHit { file: string; line: number; column: number; preview: string }
export interface SearchReport { hits: SearchHit[]; filesSearched: number; skipped: number; truncated: boolean }

export interface PdfLocation {
  page: number;
  h: number;
  v: number;
  width: number;
  height: number;
}

export const ipc = {
  openProject: (path: string) => invoke<ProjectInfo>("open_project", { path }),
  createProject: (parent: string, name: string, templateKind: "blank" | "arabic" = "blank") => invoke<ProjectInfo>("create_project", { parent, name, templateKind }),
  closeProject: () => invoke<void>("close_project"),
  renameOpenProject: (newName: string) => invoke<ProjectInfo>("rename_open_project", { newName }),
  saveProjectSettings: (settings: ProjectSettings) => invoke<void>("save_project_settings", { settings }),
  getTree: () => invoke<FileNode[]>("get_tree"),

  readFile: (path: string) => invoke<string>("read_project_file", { path }),
  readBinary: async (path: string) => new Uint8Array(await invoke<ArrayBuffer>("read_binary_file", { path })),
  writeFile: (path: string, content: string) => invoke<void>("write_project_file", { path, content }),
  createFile: (path: string) => invoke<void>("create_file", { path }),
  createFolder: (path: string) => invoke<void>("create_folder", { path }),
  rename: (from: string, to: string) => invoke<void>("rename_path", { from, to }),
  remove: (path: string) => invoke<void>("delete_path", { path }),
  importFiles: (targetDir: string, paths: string[], overwrite = false) =>
    invoke<string[]>("import_files", { targetDir, sources: paths.map((path) => ({ path })), overwrite }),
  exportFile: (path: string, destination: string) => invoke<void>("export_file", { path, destination }),
  revealInFileManager: (path: string) => invoke<void>("reveal_in_file_manager", { path }),
  renameProjectFolder: (root: string, newName: string) => invoke<string>("rename_project_folder", { root, newName }),
  moveProjectFolder: (root: string, destination: string) => invoke<string>("move_project_folder", { root, destination }),
  trashProjectFolder: (root: string) => invoke<void>("trash_project_folder", { root }),

  searchProject: (query: string, isRegex: boolean, caseSensitive: boolean, overlay: { path: string; content: string } | null) =>
    invoke<SearchReport>("project_search", { query, isRegex, caseSensitive, overlay }),
  cancelProjectSearch: () => invoke<void>("cancel_project_search"),
  listEngines: () => invoke<EngineInfo[]>("list_available_engines"),
  listTools: () => invoke<ToolInfo[]>("list_optional_tools"),
  formatBuffer: (content: string) => invoke<string>("format_tex_buffer", { content }),
  compile: (entryPoint: string, engine: Engine, options: CompileOptions, requestId: string) =>
    invoke<CompileResult>("compile_tex", { entryPoint, engine, options, requestId }),
  cancelCompile: (requestId: string) => invoke<void>("cancel_compile", { requestId }),
  clearBuildDir: () => invoke<void>("clear_build_dir"),

  synctexAvailable: () => invoke<boolean>("synctex_available"),
  synctexForward: (mainDocument: string, file: string, line: number, column: number) =>
    invoke<PdfLocation[]>("synctex_forward", { mainDocument, file, line, column }),
  synctexInverse: (mainDocument: string, page: number, x: number, y: number) =>
    invoke<{ file: string; line: number } | null>("synctex_inverse", { mainDocument, page, x, y }),
};

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
