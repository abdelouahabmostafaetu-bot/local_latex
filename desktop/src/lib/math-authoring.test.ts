import { describe, expect, it } from "vitest";
import { fractionExpansion, mathRanges, unsafeLiteralContext } from "./math-authoring";

describe("single-line math detection", () => {
  it("detects inline and display math, including escaped-delimiter forms", () => {
    expect(mathRanges("Text $a+b$ and \\(x\\)")).toEqual([
      { from: 6, to: 9, closed: true, display: false },
      { from: 17, to: 18, closed: true, display: false },
    ]);
    expect(mathRanges("$$a+b$$")).toEqual([{ from: 2, to: 5, closed: true, display: true }]);
    expect(mathRanges("\\[x\\]")).toEqual([{ from: 2, to: 3, closed: true, display: true }]);
  });
  it("ignores comments, verbatim, escaped dollars and oversized lines", () => {
    expect(mathRanges("% $a$")).toEqual([]);
    expect(mathRanges("\\$5 no math")).toEqual([]);
    expect(mathRanges("\\verb|$a$|")).toEqual([]);
    expect(mathRanges("x".repeat(8001))).toEqual([]);
  });
});

describe("fraction shortcut", () => {
  it("expands simple numerators inside open math only", () => {
    expect(fractionExpansion("$a")).toEqual({ from: 1, insert: "\\frac{a}{}", cursor: 10 });
    expect(fractionExpansion("let $\\alpha")?.insert).toBe("\\frac{\\alpha}{}");
    expect(fractionExpansion("\\(x_1")?.insert).toBe("\\frac{x_1}{}");
    expect(fractionExpansion("$ (x+y)")?.insert).toBe("\\frac{(x+y)}{}");
  });
  it("leaves text, comments, closed math and mixed-script words unchanged", () => {
    for (const s of ["plain a", "$a$ b", "% $a", "$x % a", "$\\text{hello", "$αx", "$a+"]) {
      expect(fractionExpansion(s)).toBeNull();
    }
  });
});


describe("literal environment protection", () => {
  it("fails closed in common literal blocks and on very large documents", () => {
    expect(unsafeLiteralContext("\\begin{verbatim}\n$a")).toBe(true);
    expect(unsafeLiteralContext("\\begin{minted}{text}\n$a")).toBe(true);
    expect(unsafeLiteralContext("\\begin{verbatim}\n% \\end{verbatim}\n$a")).toBe(true);
    expect(unsafeLiteralContext("\\begin{verbatim}\nplain\n\\end{verbatim}\n$a")).toBe(false);
    expect(unsafeLiteralContext("x".repeat(250001))).toBe(true);
  });
});
