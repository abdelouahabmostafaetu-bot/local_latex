import { describe, expect, it } from 'vitest';
import { indexProjectSymbols, relatedOccurrences } from './observatory';

describe('project symbol observatory', () => {
  it('links labels and citations across files, including multi-key commands', () => {
    const index = indexProjectSymbols({
      'main.tex': String.raw`\section{Test}\label{sec:intro}
See \eqref{eq:one} and \cref{sec:intro,eq:missing}.
\cite[see][p. 3]{smith,missing}`,
      'chapters/a.tex': String.raw`\begin{equation}\label{eq:one}x=1\end{equation}`,
      'refs.bib': '@article{smith,\n title={A paper}\n}',
    });
    expect(index.fileCount).toBe(3);
    expect(index.missing.map((x) => x.key)).toEqual(['eq:missing', 'missing']);
    const label = index.occurrences.find((x) => x.kind === 'label' && x.key === 'eq:one')!;
    expect(relatedOccurrences(index, label).map((x) => x.kind).sort()).toEqual(['label', 'reference']);
    expect(index.occurrences.find((x) => x.key === 'eq:missing')?.line).toBe(2);
  });
  it('ignores comments, verbatim, escaped commands and dynamic keys', () => {
    const source = String.raw`% \ref{comment}
\verb|\ref{inline}| \ref{real} % \cite{commented}
\begin{verbatim}
\ref{fake}
\end{verbatim}
\\ref{not-command}
\ref{\dynamic}`;
    const index = indexProjectSymbols({ 'main.tex': source });
    expect(index.missing.map((x) => x.key)).toEqual(['real']);
    const hit = index.missing[0];
    expect(source.slice(hit.from, hit.to)).toBe('real');
  });
  it('counts duplicate labels and ignores bib directives', () => {
    const index = indexProjectSymbols({ 'a.tex': '\\label{x}\\label{x}', 'a.bib': '@string{short = "x"}\n@comment{no,}\n@book{book, title={x}}' });
    expect(index.duplicates).toHaveLength(1);
    expect(index.citations.has('book')).toBe(true);
    expect(index.citations.has('no')).toBe(false);
  });
  it('preserves offsets and columns for multiple keys', () => {
    const text = 'α\n\\cref{ first, second }';
    const result = indexProjectSymbols({ 'a.tex': text });
    expect(result.missing.map(x => text.slice(x.from, x.to))).toEqual(['first', 'second']);
    expect(result.missing[0].column).toBe(8);
  });
});
