const $ = (id) => document.getElementById(id);
const invoke = window.__TAURI__?.core.invoke;
let refreshing = false;
let dirty = false;
let stateVersion = 0;
let latestSnapshot = { busy: false };
function feedback(message, error = false) {
  $('feedback').hidden = !message;
  $('feedback').textContent = message;
  $('feedback').setAttribute('role', error ? 'alert' : 'status');
  $('feedback').classList.toggle('error-detail', error);
}
async function call(name, args = {}) {
  if (!invoke) throw new Error('Open the tray panel in Dipper to use native controls.');
  return invoke(name, args);
}
const lastTranscript = window.DipperTranscript.mount($('last-transcript'), call, feedback, 'quick');
function state(snapshot) {
  latestSnapshot = snapshot;
  $('phase').textContent = snapshot.phase;
  $('status-light').className = 'status-light ' + (snapshot.recording ? 'recording' : snapshot.busy ? 'processing' : snapshot.error ? 'error' : '');
  $('open-app').disabled = snapshot.busy;
  $('microphone').disabled = snapshot.busy;
  $('cancel-quick').hidden = !snapshot.busy;
}
async function refresh() {
  dirty = true;
  if (refreshing) return;
  refreshing = true;
  try {
    do {
      dirty = false;
      const version = stateVersion;
      const snapshot = await call('get_snapshot'); if (version === stateVersion) state(snapshot);
      const settings = await call('get_settings');
      const binding = await call('effective_binding'); $('binding').textContent = binding || 'Unassigned';
      try {
        const catalog = await call('list_inputs');
        const name = settings.input_name || catalog.default_name;
        const device = catalog.devices.find((input) => input.node_name === name);
        $('input-summary').textContent = device?.description || (settings.input_name ? name + ' (unavailable)' : 'System default (unavailable)');
      } catch { $('input-summary').textContent = 'Input devices unavailable'; }
      $('input-summary').title = $('input-summary').textContent;
      try { lastTranscript.update(await call('recent_recordings')); }
      catch (error) { lastTranscript.unavailable('Open Journal to retry.'); feedback(String(error), true); }
    } while (dirty);
  } catch (error) { feedback(String(error), true); }
  finally { refreshing = false; }
}
async function action(button, name, args = {}) {
  if (button.disabled) return;
  button.disabled = true;
  try { await call(name, args); } catch (error) { feedback(String(error), true); }
  finally { button.disabled = false; state(latestSnapshot); }
}
$('quick-close').addEventListener('click', () => action($('quick-close'), 'hide_quick'));
$('microphone').addEventListener('click', () => action($('microphone'), 'open_main', { view: 'settings' }));
$('open-app').addEventListener('click', () => action($('open-app'), 'open_main', { view: null }));
$('quit-app').addEventListener('click', () => action($('quit-app'), 'quit_app'));
$('cancel-quick').addEventListener('click', () => action($('cancel-quick'), 'cancel_dictation'));
window.addEventListener('keydown', (event) => { if (event.key === 'Escape') { event.preventDefault(); call('hide_quick').catch((error) => feedback(String(error), true)); } });
(async () => {
  await window.__TAURI__?.event.listen('dictation-state', (event) => { stateVersion++; state(event.payload); });
  await window.__TAURI__?.event.listen('quick-opened', () => { feedback(''); refresh(); $('quick-close').focus(); });
  await refresh();
  $('quick-close').focus();
})().catch((error) => feedback(String(error), true));
