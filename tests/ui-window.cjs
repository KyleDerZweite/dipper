const assert = require('node:assert/strict');
const { test } = require('node:test');
const vm = require('node:vm');
const fs = require('node:fs');

test('a resize during a pending state query cannot apply stale window chrome', async () => {
  const replies = [];
  const attributes = {};
  const classes = new Set();
  let onResize;
  const button = { setAttribute: (name, value) => { attributes[name] = value; }, querySelector: () => ({ setAttribute() {} }), addEventListener() {} };
  const appWindow = { isMaximized: () => new Promise((resolve) => replies.push(resolve)), onResized: async (listener) => { onResize = listener; } };
  const window = { __TAURI__: { window: { getCurrentWindow: () => appWindow } }, dispatchEvent() { assert.fail('unexpected window error'); } };
  const document = { getElementById: () => button, querySelectorAll: () => [], documentElement: { classList: { toggle: (name, enabled) => enabled ? classes.add(name) : classes.delete(name) } } };
  vm.runInNewContext(fs.readFileSync('ui/window.js', 'utf8'), { window, document });
  assert.equal(replies.length, 1);
  onResize();
  replies.shift()(false);
  await new Promise(setImmediate);
  assert.equal(attributes['aria-label'], undefined, 'stale reply must not change chrome');
  assert.equal(replies.length, 1, 'the newer event requires a trailing host query');
  replies.shift()(true);
  await new Promise(setImmediate);
  assert.equal(attributes['aria-label'], 'Restore window');
  assert.equal(classes.has('window-maximized'), true);
});
