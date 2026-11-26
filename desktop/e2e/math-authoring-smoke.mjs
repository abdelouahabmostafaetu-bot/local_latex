// Called by ui-smoke.mjs against the mocked local fixture; it never writes real files.
export async function testMathAuthoring(page, menu) {
  const modifier = process.platform === "darwin" ? "Meta" : "Control";
  await menu("app.settings");
  const toggle = page.getByRole("switch", { name: "Quick fractions" });
  await toggle.click();
  if (await toggle.getAttribute("aria-checked") !== "true") throw new Error("Quick fractions did not enable");
  await page.keyboard.press("Escape");
  await page.locator(".cm-content").click();
  await page.keyboard.press(`${modifier}+End`);
  await page.keyboard.press("Enter");
  await page.keyboard.type("$a/");
  const line = page.locator(".cm-activeLine");
  let text = await line.innerText();
  if (text !== "$\\frac{a}{}$") throw new Error(`Fraction expansion failed: ${JSON.stringify(text)}`);
  await page.keyboard.type("b");
  text = await line.innerText();
  if (text !== "$\\frac{a}{b}$") throw new Error(`Denominator cursor failed: ${JSON.stringify(text)}`);
  await page.keyboard.press(`${modifier}+z`);
  await page.keyboard.press(`${modifier}+z`);
  text = await line.innerText();
  if (text !== "$a$") throw new Error(`Undo failed: ${JSON.stringify(text)}`);
  const position = await line.evaluate((element) => {
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    let node;
    while ((node = walker.nextNode())) {
      const offset = node.textContent.indexOf("a");
      if (offset < 0) continue;
      const range = document.createRange();
      range.setStart(node, offset);
      range.setEnd(node, offset + 1);
      const box = range.getBoundingClientRect();
      return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    }
    return null;
  });
  if (!position) throw new Error("No math text to hover");
  await page.mouse.move(position.x, position.y);
  await page.waitForSelector(".cm-math-preview .katex", { timeout: 5000 });
  await page.locator(".cm-content").click();
  await page.keyboard.press(`${modifier}+End`);
  await page.keyboard.press("Home");
  await page.keyboard.press("Shift+End");
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Backspace");
  console.log("ok  math shortcut, cursor, undo, and hover preview");
}
