/** A local, deliberately conservative lexical index. Not a substitute for a TeX compilation. */
export type SymbolKind = 'label' | 'reference' | 'citation' | 'bibentry';
export interface SymbolOccurrence {
  kind: SymbolKind;
  key: string;
  file: string;
  line: number;
  column: number;
  from: number;
  to: number;
}
export interface ProjectIndex {
  occurrences: SymbolOccurrence[];
  definitions: Map<string, SymbolOccurrence[]>;
  citations: Map<string, SymbolOccurrence[]>;
  missing: SymbolOccurrence[];
  duplicates: SymbolOccurrence[][];
  fileCount: number;
}

/** Preserve offsets and newlines while hiding comments, inline verbatim and verbatim environments. */
function maskTex(source: string): string {
  const chars = source.split('');
  const hide = (start: number, end: number) => {
    for (let i = start; i < end; i++) if (chars[i] !== '\n' && chars[i] !== '\r') chars[i] = ' ';
  };
  let i = 0;
  while (i < source.length) {
    if (source[i] === '%') {
      let slashes = 0;
      for (let j = i - 1; j >= 0 && source[j] === '\\'; j--) slashes++;
      if (slashes % 2 === 0) {
        let end = source.indexOf('\n', i); if (end < 0) end = source.length;
        hide(i, end); i = end; continue;
      }
    }
    if (source[i] === '\\') {
      // TeX's control symbol \\ is not the beginning of another command.
      if (source[i + 1] === '\\') { hide(i, i + 2); i += 2; continue; }
      const verb = /^\\verb\*?([^a-zA-Z\s])/.exec(source.slice(i));
      if (verb) {
        const start = i; const delimiter = verb[1]; const end = source.indexOf(delimiter, i + verb[0].length);
        const newline = source.indexOf('\n', i); const stop = end < 0 || (newline >= 0 && newline < end) ? (newline < 0 ? source.length : newline) : end + 1;
        hide(start, stop); i = stop; continue;
      }
      const begin = /^\\begin\s*\{(verbatim\*?|Verbatim\*?|lstlisting\*?|minted\*?)\}/.exec(source.slice(i));
      if (begin) {
        const start = i; const close = new RegExp('\\\\end\\s*\\{' + begin[1].replace('*', '\\*') + '\\}', 'g');
        close.lastIndex = i + begin[0].length; const match = close.exec(source); const stop = match ? match.index + match[0].length : source.length;
        hide(start, stop); i = stop; continue;
      }
    }
    i++;
  }
  return chars.join('');
}

function lineLocator(source: string): (offset: number) => { line: number; column: number } {
  const starts = [0];
  for (let i = 0; i < source.length; i++) if (source[i] === "\n") starts.push(i + 1);
  return (offset) => {
    let lo = 0, hi = starts.length;
    while (lo + 1 < hi) { const mid = (lo + hi) >>> 1; if (starts[mid] <= offset) lo = mid; else hi = mid; }
    return { line: lo + 1, column: offset - starts[lo] + 1 };
  };
}

function add(map: Map<string, SymbolOccurrence[]>, item: SymbolOccurrence) {
  const list = map.get(item.key) ?? []; list.push(item); map.set(item.key, list);
}

/** Indexes only indexed .tex and .bib files; dynamically generated keys are deliberately skipped. */
export function indexProjectSymbols(texts: Record<string, string>): ProjectIndex {
  const occurrences: SymbolOccurrence[] = [];
  const definitions = new Map<string, SymbolOccurrence[]>();
  const citations = new Map<string, SymbolOccurrence[]>();
  const entries = Object.entries(texts).filter(([path]) => /\.(?:tex|bib)$/i.test(path)).sort(([a], [b]) => a.localeCompare(b));
  for (const [file, source] of entries) {
    const position = lineLocator(source);
    if (/\.bib$/i.test(file)) {
      const re = /@([a-zA-Z]+)\s*[({]\s*([^\s,{}()]+)\s*,/g;
      for (const m of source.matchAll(re)) {
        if (/^(?:comment|string|preamble)$/i.test(m[1])) continue;
        const from = m.index! + m[0].lastIndexOf(m[2]);
        const item: SymbolOccurrence = { kind: 'bibentry', key: m[2], file, ...position(from), from, to: from + m[2].length };
        occurrences.push(item); add(citations, item);
      }
      continue;
    }
    const masked = maskTex(source);
    const re = /\\(label|(?:eq|auto|page|name|v|c|C)?ref|cite[a-zA-Z]*)(\*)?\s*(?:\[[^\]\n]*\]\s*)*\{([^{}]*)\}/g;
    for (const m of masked.matchAll(re)) {
      const command = m[1]; const raw = m[3];
      const kind: SymbolKind = command === 'label' ? 'label' : command.startsWith('cite') ? 'citation' : 'reference';
      const groupStart = m.index! + m[0].lastIndexOf('{') + 1;
      const keys = kind === 'label' ? [raw] : raw.split(',');
      let offset = 0;
      for (const part of keys) {
        const trimmed = part.trim(); const leading = part.length - part.trimStart().length;
        const from = groupStart + offset + leading; offset += part.length + 1;
        // Macros and unresolved expansions cannot be resolved with a lexical index.
        if (!trimmed || /[\\{}#\n\r]/.test(trimmed)) continue;
        const item: SymbolOccurrence = { kind, key: trimmed, file, ...position(from), from, to: from + trimmed.length };
        occurrences.push(item); if (kind === 'label') add(definitions, item);
      }
    }
  }
  const missing = occurrences.filter((o) => (o.kind === 'reference' && !definitions.has(o.key)) || (o.kind === 'citation' && !citations.has(o.key)));
  const duplicates = [...definitions.values()].filter((group) => group.length > 1);
  return { occurrences, definitions, citations, missing, duplicates, fileCount: entries.length };
}

export function relatedOccurrences(index: ProjectIndex, selected: SymbolOccurrence): SymbolOccurrence[] {
  const family = selected.kind === 'label' || selected.kind === 'reference' ? new Set(['label', 'reference']) : new Set(['bibentry', 'citation']);
  return index.occurrences.filter((o) => o.key === selected.key && family.has(o.kind));
}
