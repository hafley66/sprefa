const { spawn, execSync } = require("child_process");
const { chromium } = require("playwright");
const path = require("path");

const VSCODE = "/Applications/Visual Studio Code.app/Contents/MacOS/Electron";
const PORT = 9334;
const PROOF = path.join(__dirname, ".");
const WS = "/tmp/dl6ws";
const UD = "/tmp/dl6vd2";

function sleep(ms) { return new Promise(r => setTimeout(r, ms)); }
async function waitFor(cond, ms, every = 250) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (await cond()) return true;
    await sleep(every);
  }
  return false;
}

const child = spawn(VSCODE, [
  `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${UD}`,
  "--extensions-dir=/tmp/emptydir",
  "--disable-workspace-trust",
  "--disable-updates",
  "--no-sandbox",
  `--extensionDevelopmentPath=${PROOF}`,
  WS,
], { stdio: ["ignore", "pipe", "pipe"] });
child.stderr.on("data", d => process.stderr.write("VSERR:" + d.toString().slice(0, 80) + "\n"));

(async () => {
  // wait for CDP
  const up = await waitFor(async () => {
    try { const r = await fetch(`http://127.0.0.1:${PORT}/json/list`); return r.ok; } catch { return false; }
  }, 40000);
  if (!up) { console.error("VS Code CDP never came up"); child.kill("SIGKILL"); process.exit(1); }
  console.log("CDP up");
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
  let page = null;
  const deadline = Date.now() + 20000;
  while (!page && Date.now() < deadline) {
    for (const c of browser.contexts()) for (const p of c.pages()) if (p.url().includes("vscode-app")) page = p;
    if (!page) await sleep(300);
  }
  if (!page) { console.error("no editor page"); child.kill("SIGKILL"); process.exit(1); }
  await page.bringToFront();
  page.on("console", m => { if (m.type() === "error") console.log("PAGEERR:", m.text().slice(0, 120)); });
  await sleep(3000);

  async function pal(cmd) {
    await page.keyboard.press("Meta+Shift+P");
    await sleep(700);
    await page.keyboard.type(cmd, { delay: 10 });
    await sleep(700);
    await page.keyboard.press("Enter");
    await sleep(1000);
  }

  console.log("TITLE:", await page.title());
  console.log("TABS:", await page.evaluate(() => [...document.querySelectorAll('.monaco-icon-label .label-name')].map(e=>e.textContent.trim()).slice(0,10)));

  const before = await page.evaluate(() => document.body.innerText.slice(0, 400));
  console.log("BODY_BEFORE:", JSON.stringify(before.replace(/\n+/g," | ").slice(0,400)));

  // After extension activate, editor should show scratch.dl6 with a squiggle.
  // Open the Problems panel and dump its rendered text.
  await pal("workbench.view.problems");
  await sleep(1200);
  const problems = await page.evaluate(() => {
    const area = document.querySelector('.markers-panel') || document.body;
    return area.innerText.slice(0, 1500);
  });
  console.log("=== PROBLEMS PANEL TEXT ===");
  console.log(problems);

  // Screenshot the problems panel (shows the message render).
  try { await page.screenshot({ path: path.join(PROOF, "shot-1-problems.png") }); } catch (e) { console.log("shot1 fail", e.message); }

  // Close problems, focus editor, and show hover on the bad line programmatically.
  await pal("workbench.action.closePanel");
  await sleep(600);
  // Click into the editor to focus it, then close any palette.
  await page.mouse.click(400, 120);
  await sleep(500);
  await page.keyboard.press("Escape");
  await sleep(300);

  // Hover the squiggle directly with the mouse (most reliable render trigger).
  const squiggle = await page.locator('.squiggly-error, .squiggly-warning').first().boundingBox();
  if (squiggle) {
    await page.mouse.move(squiggle.x + squiggle.width / 2, squiggle.y + 4);
    await sleep(1600);
  } else {
    // fall back: caret to line 3 then showHover command
    await page.keyboard.press("Meta+Shift+P");
    await sleep(600);
    await page.keyboard.type("workbench.action.gotoLine", { delay: 10 });
    await sleep(600);
    await page.keyboard.press("Enter");
    await sleep(500);
    await page.keyboard.type("3", { delay: 20 });
    await page.keyboard.press("Enter");
    await sleep(500);
    await page.keyboard.press("Escape");
    await pal("editor.action.showHover");
  }
  await sleep(400);
  const hoverText = await page.evaluate(() => {
    const h = document.querySelector('.monaco-hover-content');
    return h ? h.innerText.slice(0, 1800) : "(no hover content)";
  });
  console.log("=== HOVER TEXT ===");
  console.log(hoverText);
  const hoverHtml = await page.evaluate(() => {
    const h = document.querySelector('.monaco-hover-content');
    return h ? h.innerHTML.slice(0, 3000) : "(no hover content)";
  });
  console.log("=== HOVER HTML ===");
  console.log(hoverHtml);
  try { await page.screenshot({ path: path.join(PROOF, "shot-2-hover.png") }); } catch (e) { console.log("shot2 fail", e.message); }

  // Open the webview and capture.
  await pal("dl6RichDiag.openWebview");
  await sleep(1500);
  const wvText = await page.evaluate(() => {
    const frames = [...document.querySelectorAll('iframe.webview')];
    const bodies = [...document.querySelectorAll('iframe.webview')].map(f => {
      try { return f.contentDocument ? f.contentDocument.body.innerText : "(no doc)"; } catch { return "(blocked)"; }
    });
    return { count: frames.length, bodies };
  });
  console.log("=== WEBVIEW ===", JSON.stringify(wvText));
  try { await page.screenshot({ path: path.join(PROOF, "shot-3-webview.png") }); } catch (e) { console.log("shot3 fail", e.message); }

  // reportState writes proof-state.json for non-render confirmation
  await pal("dl6RichDiag.reportState");

  await browser.close();
  child.kill("SIGKILL");
  console.log("DONE");
})();
