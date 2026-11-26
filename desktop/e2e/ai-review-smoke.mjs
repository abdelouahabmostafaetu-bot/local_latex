// Exercises review decisions against a mocked Copilot response; no real project is changed.
export async function testAiReview(page, original) {
const editor = page.locator('.cm-content').first();

const proposed = original.replace('LocalLeaf Test Document','A New Title').replace('A leaf.','A new caption.');
await editor.click();
await page.keyboard.press('Control+Home'); await page.keyboard.press('Control+a');
await page.getByRole('button',{name:'Copilot',exact:true}).click();
await page.getByLabel('Message Copilot').fill('Suggest changes');
await page.getByRole('button',{name:'Send ↑'}).click();
await page.evaluate((text) => {window.__emit('copilot:chunk','```latex\n'+text+'```');window.__emit('copilot:done',null)},proposed);
await page.getByRole('button',{name:'Review diff'}).click();
const dialog = page.getByRole('dialog',{name:'Review Copilot changes'});
await dialog.waitFor();
let accept = dialog.getByRole('button',{name:'Accept this change'});
if (await accept.count() !== 2) throw new Error(`Expected two hunks, got ${await accept.count()}`);
await accept.first().click();
await dialog.getByRole('button',{name:'Reject this change'}).first().click();
await dialog.locator('.cm-content').focus();
await page.keyboard.press('Control+z');
if (await dialog.getByRole('button',{name:'Reject this change'}).count() !== 1) throw new Error('Undo decision failed');
await dialog.getByRole('button',{name:'Reject this change'}).click();
await dialog.getByRole('button',{name:'Apply reviewed changes'}).click();
let content = await editor.innerText();
if (!content.includes('A New Title') || content.includes('A new caption.')) throw new Error('Partial review applied incorrectly');
await editor.click(); await page.keyboard.press('Control+z');
content = await editor.innerText();
if (!content.includes('LocalLeaf Test Document') || content.includes('A New Title')) throw new Error('Editor undo did not restore original');
console.log('PASS: two hunks, accept, reject, undo decision, apply, editor undo');
// Simulate an external file switch while the modal is open: stale suggestions must not apply.
await editor.click(); await page.keyboard.press('Control+a');
await page.getByRole('button',{name:'Review diff'}).click();
await dialog.waitFor();
await page.getByText('refs.bib', {exact:true}).first().evaluate(el => el.click());
await page.waitForFunction(() => document.querySelector('.cm-content')?.textContent?.includes('@book'));
while (await dialog.getByRole('button',{name:'Accept this change'}).count()) await dialog.getByRole('button',{name:'Accept this change'}).first().click();
await dialog.getByRole('button',{name:'Apply reviewed changes'}).click();
await page.getByText('The file or editor changed during review.', {exact:false}).waitFor();
if (!(await editor.innerText()).includes('@book')) throw new Error('Stale review overwrote another file');
console.log('PASS: stale review cannot overwrite another file');
// Return to main for the diagnostic workflow.
await page.getByText('main.tex', {exact:true}).first().click();

}
