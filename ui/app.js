const $ = (id) => document.getElementById(id);
const invoke = window.__TAURI__?.core.invoke;
const titles = { dictation: "Dictation", journal: "Journal", settings: "Settings" };
const configurationActions = ["save", "configure-hold", "kde-shortcuts", "refresh"];
const pending = new Set();
let busy = false;
let initialized = false;
let binding = null;
let savedSettings = null;
let inputCatalog = null;
let journalRequest = null;
const lastTranscript = window.DipperTranscript.mount($("last-transcript"), call, feedback);
let latestSnapshot = { recording: false, busy: false, error: false };

function navigate() {
  let view = location.hash.slice(1);
  if (!Object.hasOwn(titles, view)) {
    view = "dictation";
    history.replaceState(null, "", "#dictation");
  }
  for (const name of Object.keys(titles)) $("view-" + name).hidden = name !== view;
  for (const link of document.querySelectorAll(".nav-link")) {
    if (link.hash === "#" + view) link.setAttribute("aria-current", "page");
    else link.removeAttribute("aria-current");
  }
  $("view-title").textContent = titles[view];
  document.title = `${titles[view]} · Dipper`;
  document.querySelector(".view-scroll").scrollTop = 0;
}
window.addEventListener("hashchange", navigate);
navigate();
document.querySelector(".skip-link").addEventListener("click", (event) => {
  event.preventDefault();
  $("main-content").focus();
});
function feedback(message, error = false) {
  $("feedback").hidden = !message;
  $("feedback").textContent = message;
  $("feedback").classList.toggle("error-detail", error);
  $("feedback").setAttribute("role", error ? "alert" : "status");
}
function synchronizeControls() {
  const configurationPending = configurationActions.some((id) => pending.has(id));
  $("settings-fields").disabled = busy || configurationPending;
  for (const id of configurationActions) $(id).disabled = busy || configurationPending;
  $("retry-recovery").disabled = busy || pending.has("retry-recovery");
  $("cancel").disabled = !busy || pending.has("cancel");
}
async function call(name, args = {}) {
  if (!invoke) throw new Error("Open Dipper in the desktop app to use native controls.");
  return invoke(name, args);
}
function focusSettingsError(error) {
  const message = String(error).toLowerCase();
  const field = message.includes("model") ? $("model") : message.includes("runtime") || message.includes("librar") ? $("runtime") : message.includes("shortcut") || message.includes("hold") ? $("hold") : null;
  if (!field) return;
  if (field.closest("details")) field.closest("details").open = true;
  field.setAttribute("aria-invalid", "true");
  field.setAttribute("aria-errormessage", "feedback");
  field.focus();
}
async function action(id, task, success, settingsAction = false) {
  if (pending.has(id)) return;
  pending.add(id);
  synchronizeControls();
  const button = $(id);
  const originalLabel = button.textContent;
  if (id === "save") button.textContent = "Saving…";
  if (id === "configure-hold") button.textContent = "Waiting for KDE…";
  let failure = null;
  try {
    await task();
    if (success) feedback(success);
  } catch (error) {
    failure = error;
    feedback(String(error), true);
  } finally {
    pending.delete(id);
    button.textContent = originalLabel;
    synchronizeControls();
    if (failure && settingsAction) focusSettingsError(failure);
  }
}
function updatePrompt() {
  $("binding").textContent = binding || "Unassigned";
  $("hold-instruction").textContent = latestSnapshot.recording ? "Release to transcribe" : latestSnapshot.busy ? "Processing dictation" : binding ? "Hold to dictate" : "Configure your Hold key";
}
function applySnapshot(snapshot) {
  const wasBusy = busy;
  latestSnapshot = snapshot;
  busy = snapshot.busy;
  $("phase").textContent = snapshot.phase;
  $("detail").textContent = snapshot.detail;
  $("detail").classList.toggle("error", snapshot.error);
  $("status-light").className = "status-light " + (snapshot.recording ? "recording" : busy ? "processing" : snapshot.error ? "error" : "");
  synchronizeControls();
  updatePrompt();
  if (wasBusy && !busy && initialized) refreshJournal().catch((error) => feedback(String(error), true));
}
function updateInputSummary() {
  if (!savedSettings || !inputCatalog) return;
  const name = savedSettings.input_name || inputCatalog.default_name;
  const device = inputCatalog.devices.find((input) => input.node_name === name);
  $("input-summary").textContent = savedSettings.input_name ? (device?.description || `${name} (unavailable)`) : `System default${device ? " · " + device.description : " (unavailable)"}`;
  $("input-summary").title = $("input-summary").textContent;
}
async function refreshInputs(selected) {
  const catalog = await call("list_inputs");
  inputCatalog = catalog;
  const previous = selected ?? $("input").value;
  $("input").replaceChildren(new Option("System default", ""));
  for (const input of catalog.devices) {
    const suffix = input.node_name === catalog.default_name ? " (current default)" : "";
    $("input").append(new Option(input.description + suffix, input.node_name));
  }
  if (previous && !catalog.devices.some((input) => input.node_name === previous)) $("input").append(new Option(previous + " (unavailable)", previous));
  $("input").value = previous;
  updateInputSummary();
}
function renderJournal(recordings) {
  lastTranscript.update(recordings);
  const root = $("journal");
  root.replaceChildren();
  $("journal-count").textContent = `${recordings.length} recent recording${recordings.length === 1 ? "" : "s"}`;
  if (!recordings.length) {
    const empty = document.createElement("div"); empty.className = "empty";
    const message = document.createElement("p"); message.textContent = "No recordings yet.";
    const link = document.createElement("a"); link.href = "#dictation"; link.className = "text-link empty-action"; link.textContent = "Go to Dictation";
    empty.append(message, link); root.append(empty); return;
  }
  for (const recording of recordings) {
    const entry = document.createElement("article"); entry.className = "entry";
    const top = document.createElement("div"); top.className = "entry-top";
    const date = new Date(Number(recording.started_at));
    const time = document.createElement("time");
    if (!Number.isNaN(date.getTime())) { time.dateTime = date.toISOString(); time.textContent = new Intl.DateTimeFormat("en-GB", { dateStyle: "medium", timeStyle: "short" }).format(date); }
    else time.textContent = "Recording time unavailable";
    top.append(time);
    if (recording.raw_text) {
      const copy = document.createElement("button"); copy.className = "secondary-button"; copy.textContent = "Copy"; copy.type = "button";
      copy.addEventListener("click", async () => {
        if (copy.disabled) return;
        copy.disabled = true;
        try { await call("copy_transcript", { recordingId: recording.id }); feedback("Transcript copied."); }
        catch (error) { feedback(String(error), true); }
        finally { copy.disabled = false; }
      });
      top.append(copy);
    }
    entry.append(top);
    const outcomes = document.createElement("div"); outcomes.className = "outcomes";
    for (const outcome of recording.state.split(" · ")) { const text = document.createElement("span"); text.textContent = outcome; outcomes.append(text); }
    entry.append(outcomes);
    if (recording.raw_text) { const p = document.createElement("p"); p.className = "transcript"; p.textContent = recording.raw_text; entry.append(p); }
    if (recording.detail) { const p = document.createElement("p"); p.className = "entry-detail"; p.textContent = recording.detail; entry.append(p); }
    root.append(entry);
  }
}
async function refreshJournal() {
  if (journalRequest) return journalRequest;
  $("journal").setAttribute("aria-busy", "true");
  journalRequest = (async () => {
    try { renderJournal(await call("recent_recordings")); }
    catch (error) {
      lastTranscript.unavailable("Open Journal and retry loading.");
      $("journal-count").textContent = "Recordings unavailable";
      $("journal").replaceChildren();
      const message = document.createElement("p"); message.className = "empty"; message.textContent = `Could not load recordings: ${String(error)}`;
      const retry = document.createElement("button"); retry.className = "secondary-button"; retry.textContent = "Retry loading"; retry.type = "button";
      retry.addEventListener("click", () => refreshJournal().then(() => feedback("Recordings loaded.")).catch((failure) => feedback(String(failure), true)));
      $("journal").append(message, retry);
      throw error;
    } finally { $("journal").setAttribute("aria-busy", "false"); journalRequest = null; }
  })();
  return journalRequest;
}
$("retry-recovery").addEventListener("click", () => action("retry-recovery", async () => { await call("retry_recovery"); await refreshJournal(); }, "Recovery checked. No text was inserted."));
$("cancel").addEventListener("click", () => action("cancel", () => call("cancel_dictation")));
$("refresh").addEventListener("click", () => action("refresh", () => refreshInputs(), "Input devices refreshed."));
$("settings").addEventListener("submit", (event) => {
  event.preventDefault();
  if (busy || configurationActions.some((id) => pending.has(id))) return;
  for (const input of $("settings").querySelectorAll("input")) { input.removeAttribute("aria-invalid"); input.removeAttribute("aria-errormessage"); }
  const invalid = [...$("settings").querySelectorAll("input")].find((input) => !input.checkValidity());
  if (invalid) {
    if (invalid.closest("details")) invalid.closest("details").open = true;
    invalid.setAttribute("aria-invalid", "true"); invalid.setAttribute("aria-errormessage", "feedback");
    feedback(`Enter ${invalid.labels[0].textContent.toLowerCase()}${invalid.validity.patternMismatch ? " as an absolute path" : ""}.`, true);
    invalid.reportValidity(); invalid.focus(); return;
  }
  const settings = {
    input_name: $("input").value || null,
    model_path: $("model").value.trim(),
    native_lib_dir: $("runtime").value.trim(),
    preferred_hold: $("hold").value.trim(),
  };
  action("save", async () => { await call("save_settings", { settings }); savedSettings = settings; updateInputSummary(); }, "Settings saved. Changes apply to the next dictation.", true);
});
$("configure-hold").addEventListener("click", () => action("configure-hold", async () => {
  binding = await call("configure_hold", { preferred: $("hold").value.trim() });
  updatePrompt();
}, "Hold configured.", true));
$("kde-shortcuts").addEventListener("click", () => action("kde-shortcuts", () => call("open_shortcut_settings")));
async function initialize() {
  await window.__TAURI__?.event.listen("navigate-view", (event) => { if (Object.hasOwn(titles, event.payload)) location.hash = event.payload; });
  await window.__TAURI__?.event.listen("dictation-state", (event) => applySnapshot(event.payload));
  savedSettings = await call("get_settings");
  $("model").value = savedSettings.model_path;
  $("runtime").value = savedSettings.native_lib_dir;
  $("hold").value = savedSettings.preferred_hold;
  await refreshInputs(savedSettings.input_name ?? "").catch((error) => {
    $("input-summary").textContent = "Input devices unavailable. Refresh in Settings.";
    if (savedSettings.input_name) {
      $("input").append(new Option(savedSettings.input_name + " (availability unknown)", savedSettings.input_name));
      $("input").value = savedSettings.input_name;
    }
    feedback(`Could not list input devices: ${String(error)}. Use Refresh devices to retry.`, true);
  });
  binding = await call("effective_binding");
  applySnapshot(await call("get_snapshot"));
  initialized = true;
  await refreshJournal().catch((error) => feedback(String(error), true));
}
initialize().catch((error) => {
  $("phase").textContent = "Setup needed";
  $("detail").textContent = String(error);
  feedback(String(error), true);
});

window.addEventListener("dipper-window-error", (event) => feedback(event.detail, true));
