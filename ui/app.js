const $ = (id) => document.getElementById(id);
const invoke = window.__TAURI__?.core.invoke;
let busy = false;
let initialized = false;
function feedback(message, error = false) {
  $("feedback").hidden = !message;
  $("feedback").textContent = message;
  $("feedback").classList.toggle("error-detail", error);
}
async function call(name, args = {}) {
  if (!invoke) throw new Error("Open this page in the Dipper desktop app.");
  return invoke(name, args);
}
async function action(task, success) {
  try { await task(); if (success) feedback(success); }
  catch (error) { feedback(String(error), true); }
}
function applySnapshot(snapshot) {
  busy = snapshot.busy;
  $("phase").textContent = snapshot.phase;
  $("detail").textContent = snapshot.detail;
  $("cancel").disabled = !busy;
  for (const id of ["save", "configure-hold", "kde-shortcuts", "refresh", "retry-recovery"]) $(id).disabled = busy;
  $("status-light").className = "light " + (snapshot.recording ? "recording" : busy ? "processing" : snapshot.error ? "error" : "");
  if (!busy && initialized) refreshJournal().catch((error) => feedback(String(error), true));
}
async function refreshInputs(selected) {
  const catalog = await call("list_inputs");
  const previous = selected ?? $("input").value;
  $("input").replaceChildren(new Option("System default at each recording", ""));
  for (const input of catalog.devices) {
    const suffix = input.node_name === catalog.default_name ? " (current default)" : "";
    $("input").append(new Option(input.description + suffix, input.node_name));
  }
  if (previous && !catalog.devices.some((input) => input.node_name === previous)) {
    $("input").append(new Option(previous + " (unavailable)", previous));
  }
  $("input").value = previous;
}
async function refreshJournal() {
  const recordings = await call("recent_recordings");
  const root = $("journal");
  root.replaceChildren();
  if (!recordings.length) {
    const p = document.createElement("p"); p.className = "empty";
    p.textContent = "No recordings yet. Focus the T3 composer, then press and hold your configured key.";
    root.append(p); return;
  }
  for (const recording of recordings) {
    const entry = document.createElement("article"); entry.className = "entry";
    const top = document.createElement("div"); top.className = "entry-top";
    const label = document.createElement("span");
    label.textContent = new Date(Number(recording.started_at)).toLocaleString() + " · " + recording.state;
    top.append(label);
    if (recording.raw_text) {
      const copy = document.createElement("button"); copy.className = "secondary"; copy.textContent = "Copy";
      copy.addEventListener("click", () => action(() => call("copy_transcript", { recordingId: recording.id }), "Transcript copied."));
      top.append(copy);
    }
    entry.append(top);
    if (recording.raw_text) { const p = document.createElement("p"); p.className = "transcript"; p.textContent = recording.raw_text; entry.append(p); }
    if (recording.detail) { const p = document.createElement("p"); p.className = "hint"; p.textContent = recording.detail; entry.append(p); }
    root.append(entry);
  }
}
$("retry-recovery").addEventListener("click", () => action(async () => { await call("retry_recovery"); await refreshJournal(); }, "Journal recovery reconciled. No text was inserted."));
$("cancel").addEventListener("click", () => action(() => call("cancel_dictation")));
$("refresh").addEventListener("click", () => action(() => refreshInputs()));
$("settings").addEventListener("submit", (event) => {
  event.preventDefault();
  action(() => call("save_settings", { settings: {
    input_name: $("input").value || null,
    model_path: $("model").value.trim(),
    native_lib_dir: $("runtime").value.trim(),
    preferred_hold: $("hold").value.trim(),
  }}), "Settings saved. Changes apply to the next dictation.");
});
$("configure-hold").addEventListener("click", () => action(async () => {
  const binding = await call("configure_hold", { preferred: $("hold").value.trim() });
  $("binding").textContent = "Hold " + binding;
}, "Hold configured. Use only this key during dictation."));
$("kde-shortcuts").addEventListener("click", () => action(() => call("open_shortcut_settings")));
async function initialize() {
  await window.__TAURI__?.event.listen("dictation-state", (event) => applySnapshot(event.payload));
  const settings = await call("get_settings");
  $("model").value = settings.model_path;
  $("runtime").value = settings.native_lib_dir;
  $("hold").value = settings.preferred_hold;
  await refreshInputs(settings.input_name ?? "");
  const binding = await call("effective_binding");
  if (binding) $("binding").textContent = "Hold " + binding;
  applySnapshot(await call("get_snapshot"));
  initialized = true;
  await refreshJournal();
}
initialize().catch((error) => { $("phase").textContent = "Setup needed"; $("detail").textContent = String(error); feedback(String(error), true); });
