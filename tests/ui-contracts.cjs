const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const UI_DIR = path.join(__dirname, "..", "ui");

test("HTML views comply with strict Tauri CSP (no inline scripts, styles, handlers, or external URLs)", () => {
  for (const file of ["index.html", "quick.html"]) {
    const html = fs.readFileSync(path.join(UI_DIR, file), "utf8");
    assert.equal(/\son[a-z]+\s*=/i.test(html), false, `${file} must not contain inline event handlers`);
    assert.equal(/\sstyle\s*=/i.test(html), false, `${file} must not contain inline style attributes`);
    assert.equal(/<style[\s>]/i.test(html), false, `${file} must not contain inline <style> tags`);
    assert.equal(/https?:\/\//i.test(html), false, `${file} must not load external HTTP/CDN assets`);
    const scriptTags = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi)];
    assert.ok(scriptTags.length > 0, `${file} should load local scripts`);
    for (const [, attrs, body] of scriptTags) {
      assert.match(attrs, /\bsrc=["'][^"']+["']/, `${file} <script> must reference local src`);
      assert.equal(body.trim(), "", `${file} <script> tags must not have inline bodies`);
    }
  }
});

test("JS files never use innerHTML, insertAdjacentHTML, or outerHTML", () => {
  const jsFiles = fs.readdirSync(UI_DIR).filter((name) => name.endsWith(".js"));
  for (const file of jsFiles) {
    const content = fs.readFileSync(path.join(UI_DIR, file), "utf8");
    assert.equal(/\.innerHTML\b/.test(content), false, `${file} must not use innerHTML`);
    assert.equal(/\.outerHTML\b/.test(content), false, `${file} must not use outerHTML`);
    assert.equal(/insertAdjacentHTML\b/.test(content), false, `${file} must not use insertAdjacentHTML`);
    assert.equal(/eval\s*\(/.test(content), false, `${file} must not use eval`);
  }
});

test("window.js, icons.svg, logo.png, and quick.html host contracts remain intact", () => {
  const indexHtml = fs.readFileSync(path.join(UI_DIR, "index.html"), "utf8");
  for (const id of ["window-minimize", "window-maximize", "window-close", "app-version", "main-content"]) {
    assert.ok(indexHtml.includes(`id="${id}"`), `index.html missing #${id}`);
  }
  assert.ok(
    indexHtml.includes('href="icons.svg#icon-maximize"'),
    "index.html maximize button must preserve <use href='icons.svg#icon-maximize'>"
  );
  for (const edge of ["North", "East", "South", "West", "NorthWest", "NorthEast", "SouthWest", "SouthEast"]) {
    assert.ok(indexHtml.includes(`data-resize="${edge}"`), `index.html missing resize handle ${edge}`);
  }

  const quickHtml = fs.readFileSync(path.join(UI_DIR, "quick.html"), "utf8");
  assert.ok(
    quickHtml.includes("<title>Dipper Quick Panel</title>"),
    "quick.html must preserve Dipper Quick Panel title for KWin placement"
  );

  const iconsSvg = fs.readFileSync(path.join(UI_DIR, "icons.svg"), "utf8");
  for (const symbol of [
    "icon-minimize",
    "icon-maximize",
    "icon-restore",
    "icon-close",
    "icon-mic",
    "icon-journal",
    "icon-settings",
    "icon-play",
    "icon-refresh",
    "icon-copy",
    "icon-check",
    "icon-alert",
    "icon-chevron-down",
    "icon-search",
    "icon-stop",
    "icon-arrow",
  ]) {
    assert.ok(iconsSvg.includes(`id="${symbol}"`), `icons.svg missing #${symbol}`);
  }

  assert.equal(/https?:\/\//i.test(fs.readFileSync(path.join(UI_DIR, "styles.css"), "utf8")), false, "styles.css must not load external fonts or URLs");

  const appJs = fs.readFileSync(path.join(UI_DIR, "app.js"), "utf8");
  for (const symbol of [
    "dictation-state",
    "dictation-level",
    "navigate-view",
    "check_setup_paths",
    "get_recording_audio",
    "retry_recognition",
    "retry_recovery",
    "audioRegistry.revoke",
    "audioRegistry.revokeAll",
  ]) {
    assert.ok(appJs.includes(symbol), `app.js must wire ${symbol}`);
  }

  const formatJs = fs.readFileSync(path.join(UI_DIR, "journal_format.js"), "utf8");
  assert.ok(formatJs.includes("URL.revokeObjectURL"), "journal_format.js must call URL.revokeObjectURL");

  const quickJs = fs.readFileSync(path.join(UI_DIR, "quick.js"), "utf8");
  for (const symbol of ["dictation-state", "dictation-level", "quick-opened", "open_main"]) {
    assert.ok(quickJs.includes(symbol), `quick.js must wire ${symbol}`);
  }

  assert.equal(fs.existsSync(path.join(UI_DIR, "logo.png")), true, "ui/logo.png must exist");
});

test("DipperTranscript toggles Recovered badge for non-complete audio with transcript", () => {
  const elements = [];
  const document = {
    createElement: () => {
      const el = {
        children: [],
        hidden: false,
        append(...items) {
          this.children.push(...items);
        },
        addEventListener(_, cb) {
          this.click = cb;
        },
      };
      elements.push(el);
      return el;
    },
  };
  const window = {};
  vm.runInNewContext(fs.readFileSync(path.join(UI_DIR, "transcript.js"), "utf8"), { window, document });
  const view = window.DipperTranscript.mount({ append() {} }, async () => {}, () => {});
  const badge = elements.find((el) => el.textContent === "Recovered");
  assert.ok(badge, "Recovered pill element should exist");
  assert.equal(badge.hidden, true);

  view.update([
    { id: "rec-1", audio_state: "failed", raw_text: "Recovered phrase", state: "Audio failed" },
  ]);
  assert.equal(badge.hidden, false);

  view.update([
    { id: "rec-2", audio_state: "complete", raw_text: "Clean phrase", state: "Audio complete" },
  ]);
  assert.equal(badge.hidden, true);
});
