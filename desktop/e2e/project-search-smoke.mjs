// UI exercise uses the Tauri fixture's search mock; backend behavior has Rust unit tests.
export async function testProjectSearch(page) {
  const modifier = process.platform === "darwin" ? "Meta" : "Control";
  await page.locator(".cm-content").click();
  await page.keyboard.press(`${modifier}+End`);
  await page.keyboard.press("Enter");
  await page.keyboard.type("UNSAVED_SEARCH_NEEDLE");
  await page.keyboard.press(`${modifier}+Shift+f`);
  const dialog = page.getByRole("dialog", { name: "Search project" });
  const query = dialog.getByLabel("Find in project");
  await query.fill("UNSAVED_SEARCH_NEEDLE");
  const match = dialog.getByRole("button", { name: /main\.tex.*UNSAVED_SEARCH_NEEDLE/s });
  await match.waitFor({ timeout: 5000 });
  await match.click();
  if (!(await page.locator(".cm-activeLine").innerText()).includes("UNSAVED_SEARCH_NEEDLE")) throw new Error("Search result did not navigate to the live buffer");
  await page.keyboard.press(`${modifier}+Shift+f`);
  await dialog.getByRole("checkbox", { name: "Regular expression" }).check();
  await dialog.getByLabel("Find in project").fill("[");
  await dialog.getByRole("alert").waitFor({ timeout: 5000 });
  await page.keyboard.press("Escape");
  await page.locator(".cm-content").click();
  await page.keyboard.press(`${modifier}+End`);
  await page.keyboard.press("Home");
  await page.keyboard.press("Shift+End");
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Backspace");
  console.log("ok  project search, live buffer, navigation, and invalid regex");
}
