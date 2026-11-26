/** Conservative single-line math detection for local previews and opt-in input rules.
 * Unsupported/multiline TeX is left untouched rather than guessed. */
import { EditorSelection, type Extension } from "@codemirror/state";
import { isolateHistory } from "@codemirror/commands";
import { EditorView, hoverTooltip } from "@codemirror/view";
import katex from "katex";
import "katex/dist/katex.min.css";

export interface MathRange { from: number; to: number; closed: boolean; display: boolean }

function escaped(text: string, pos: number): boolean {
  let slash = 0;
  while (pos > slash && text[pos - slash - 1] === "\\") slash++;
  return slash % 2 === 1;
}

/** Ranges exclude delimiters. Never treats comments, escaped dollars, or verbatim as math. */
export function mathRanges(line: string): MathRange[] {
  if (line.length > 8000 || /\\verb\*?/.test(line)) return [];
  const ranges: MathRange[] = [];
  let current: { from: number; close: string; display: boolean } | null = null;
  let limit = line.length;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === "%" && !escaped(line, i)) { limit = i; break; }
    if (c === "$" && !escaped(line, i)) {
      const token = line[i + 1] === "$" ? "$$" : "$";
      if (!current) current = { from: i + token.length, close: token, display: token === "$$" };
      else if (current.close === token) {
        ranges.push({ from: current.from, to: i, closed: true, display: current.display });
        current = null;
      }
      i += token.length - 1;
      continue;
    }
    if (c === "\\" && !escaped(line, i) && i + 1 < line.length) {
      const token = line.slice(i, i + 2);
      if (!current && (token === "\\(" || token === "\\[")) {
        current = { from: i + 2, close: token === "\\(" ? "\\)" : "\\]", display: token === "\\[" };
        i++;
      } else if (current?.close === token) {
        ranges.push({ from: current.from, to: i, closed: true, display: current.display });
        current = null;
        i++;
      }
    }
  }
  if (current) ranges.push({ from: current.from, to: limit, closed: false, display: current.display });
  return ranges;
}

export interface FractionExpansion { from: number; insert: string; cursor: number }
/** The rule is fixed and declarative: no eval, dynamic code, or user-supplied regex. */
export function fractionExpansion(linePrefix: string): FractionExpansion | null {
  const range = mathRanges(linePrefix).find(r => !r.closed && r.to === linePrefix.length);
  if (!range) return null;
  const math = linePrefix.slice(range.from);
  // Avoid changing prose embedded in math commands; unsupported nesting fails closed.
  if (/\\(?:text|texttt|textrm|textsf|mbox|operatorname)\s*\{/.test(math)) return null;
  const match = /(?:\\[A-Za-z]+|[A-Za-z0-9]+(?:_[A-Za-z0-9]+)?|\([^()\n]+\))$/.exec(math);
  if (!match) return null;
  const from = range.from + match.index;
  if (from > range.from && /[\p{L}\p{N}_\\]/u.test(linePrefix[from - 1])) return null;
  const insert = `\\frac{${match[0]}}{}`;
  return { from, insert, cursor: from + insert.length - 1 };
}

/** Fail closed inside common literal environments or exceptionally large documents. */
export function unsafeLiteralContext(prefix: string): boolean {
  if (prefix.length > 250_000) return true;
  let literal: string | null = null;
  for (const line of prefix.split("\n")) {
    if (literal) {
      const close = /^\s*\\end\{(verbatim\*?|lstlisting|minted|comment)\}\s*(?:%.*)?$/.exec(line);
      if (close?.[1] === literal) literal = null;
      continue;
    }
    let end = line.length;
    for (let i = 0; i < line.length; i++) if (line[i] === "%" && !escaped(line, i)) { end = i; break; }
    const open = /\\begin\{(verbatim\*?|lstlisting|minted|comment)\}/.exec(line.slice(0, end));
    if (open && !escaped(line, open.index)) literal = open[1];
  }
  return literal !== null;
}

export function quickFractionInput(isTex: () => boolean): Extension {
  return EditorView.inputHandler.of((view, from, to, text) => {
    if (!isTex() || text !== "/" || from !== to || view.composing || view.state.readOnly || view.state.selection.ranges.length !== 1 || view.state.selection.main.head !== from) return false;
    const line = view.state.doc.lineAt(from);
    const offset = from - line.from;
    // Only act when both delimiters are already present on this line.
    if (!mathRanges(line.text).some(r => r.closed && r.from <= offset && offset <= r.to)) return false;
    if (unsafeLiteralContext(view.state.sliceDoc(0, from))) return false;
    const expansion = fractionExpansion(line.text.slice(0, offset));
    if (!expansion) return false;
    view.dispatch({
      changes: { from: line.from + expansion.from, to, insert: expansion.insert },
      selection: EditorSelection.cursor(line.from + expansion.cursor),
      annotations: isolateHistory.of("full"),
      userEvent: "input.type",
      scrollIntoView: true,
    });
    return true;
  });
}

/** KaTeX renders locally with untrusted commands disabled and resource limits. */
export function localMathHover(isTex: () => boolean): Extension {
  return hoverTooltip((view, pos) => {
    if (!isTex()) return null;
    const line = view.state.doc.lineAt(pos);
    const local = pos - line.from;
    const range = mathRanges(line.text).find(r => r.closed && local >= r.from && local <= r.to);
    if (!range || range.to - range.from > 1000 || range.to === range.from) return null;
    const source = line.text.slice(range.from, range.to);
    return {
      pos: line.from + range.from,
      end: line.from + range.to,
      above: true,
      create() {
        const dom = document.createElement("div");
        dom.className = "cm-math-preview";
        dom.setAttribute("role", "status");
        try {
          katex.render(source, dom, { displayMode: range.display, throwOnError: true, trust: false, strict: "ignore", maxExpand: 1000, maxSize: 20 });
        } catch {
          dom.textContent = "Preview unavailable for this expression; LaTeX source is unchanged.";
        }
        return { dom };
      },
    };
  }, { hoverTime: 350, hideOnChange: true });
}
