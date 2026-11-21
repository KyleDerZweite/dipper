(() => {
  "use strict";

  const F = window.DipperFormat;
  const $ = (id) => document.getElementById(id);
  const invoke = window.__TAURI__?.core.invoke;
  const titles = { dictation: "Dictation", journal: "Journal", settings: "Settings" };
  const configurationActions = [
    "save",
    "configure-hold",
    "kde-shortcuts",
    "refresh",
    "recheck-setup",
  ];
  const WAVE_BAR_COUNT = 24;

  const pending = new Set();
  const audioRegistry = F.createAudioUrlRegistry();
  const waveBars = [];

  let busy = false;
  let initialized = false;
  let binding = null;
  let savedSettings = null;
  let selectedModelProfile = "large_v3_turbo_q8_0";
  let inputCatalog = null;
  let journalRefresh = null;
  let journalDirty = false;
  let snapshotSeq = 0;
  let allRecordings = [];
  let journalFilter = "all";
  let journalSearch = "";
  let pendingRecoveryId = null;
  let latestSetupStatus = null;
  let setupCheckSeq = 0;
  let setupCheckTimer = null;

  let latestSnapshot = {
    phase: "Starting",
    detail: "Connecting to the desktop.",
    recording: false,
    busy: false,
    error: false,
    recording_started_at_ms: null,
  };

  let targetAudioLevel = 0;
  let smoothedAudioLevel = 0;
  let waveRafId = null;
  let timerIntervalId = null;
  let recordingFallbackStartMs = null;

  const lastTranscript = window.DipperTranscript.mount(
    $("last-transcript"),
    call,
    feedback,
    "full"
  );

  function createIcon(name, className = "icon icon-sm") {
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("class", className);
    svg.setAttribute("aria-hidden", "true");
    const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
    use.setAttribute("href", `icons.svg#${name}`);
    svg.append(use);
    return svg;
  }

  function decodeBase64ToBytes(base64) {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }

  function buildWaveformBars() {
    const root = $("dictation-waveform");
    if (!root) return;
    const nodes = [];
    waveBars.length = 0;
    for (let i = 0; i < WAVE_BAR_COUNT; i += 1) {
      const bar = document.createElement("span");
      bar.className = "wave-bar";
      bar.style.transform = "scaleY(0.140)";
      waveBars.push(bar);
      nodes.push(bar);
    }
    root.replaceChildren(...nodes);
  }

  function updateElapsedDisplay() {
    const timer = $("elapsed-timer");
    if (!timer) return;
    if (!latestSnapshot.recording) {
      timer.textContent = "00:00";
      timer.classList.remove("is-live");
      return;
    }
    const startMs = latestSnapshot.recording_started_at_ms ?? recordingFallbackStartMs;
    const elapsedMs = F.computeElapsedMs(startMs, Date.now());
    timer.textContent = F.formatElapsedTimer(elapsedMs);
    timer.classList.add("is-live");
  }

  function stepWaveform(now) {
    waveRafId = null;
    const isRecording = Boolean(latestSnapshot.recording);
    if (isRecording) {
      const rate = targetAudioLevel > smoothedAudioLevel ? 0.42 : 0.16;
      smoothedAudioLevel += (targetAudioLevel - smoothedAudioLevel) * rate;
      updateElapsedDisplay();
    } else {
      targetAudioLevel = 0;
      smoothedAudioLevel *= 0.78;
    }

    const active = isRecording || smoothedAudioLevel >= 0.008;
    const half = (WAVE_BAR_COUNT - 1) / 2;
    for (let i = 0; i < waveBars.length; i += 1) {
      if (!active) {
        waveBars[i].style.transform = "scaleY(0.140)";
        continue;
      }
      const centerDist = Math.abs(i - half) / half;
      const envelope = 1 - centerDist * centerDist * 0.56;
      const ripple = 0.72 + 0.28 * Math.sin(now * 0.012 + i * 0.72);
      const resting = isRecording ? 0.16 : 0.14;
      const scale = Math.min(1, resting + smoothedAudioLevel * envelope * ripple * 0.9);
      waveBars[i].style.transform = `scaleY(${scale.toFixed(3)})`;
    }

    if (active) {
      waveRafId = requestAnimationFrame(stepWaveform);
    } else {
      smoothedAudioLevel = 0;
    }
  }

  function ensureWaveformRunning() {
    if (waveRafId === null) {
      waveRafId = requestAnimationFrame(stepWaveform);
    }
  }

  function canAnimateLiveVisuals() {
    const dictationView = $("view-dictation");
    return !document.hidden && Boolean(dictationView && !dictationView.hidden);
  }

  function syncLiveVisuals() {
    if (latestSnapshot.recording && canAnimateLiveVisuals()) {
      if (timerIntervalId === null) {
        timerIntervalId = setInterval(updateElapsedDisplay, 250);
      }
      updateElapsedDisplay();
      ensureWaveformRunning();
    } else {
      if (timerIntervalId !== null) {
        clearInterval(timerIntervalId);
        timerIntervalId = null;
      }
      if (!canAnimateLiveVisuals()) {
        if (waveRafId !== null) {
          cancelAnimationFrame(waveRafId);
          waveRafId = null;
        }
        smoothedAudioLevel = 0;
        for (const bar of waveBars) {
          bar.style.transform = "scaleY(0.140)";
        }
      } else {
        updateElapsedDisplay();
        if (smoothedAudioLevel > 0) {
          ensureWaveformRunning();
        }
      }
    }
  }

  function navigate() {
    let view = location.hash.slice(1);
    if (!Object.hasOwn(titles, view)) {
      view = "dictation";
      history.replaceState(null, "", "#dictation");
    }
    for (const name of Object.keys(titles)) {
      $("view-" + name).hidden = name !== view;
    }
    for (const link of document.querySelectorAll(".nav-link")) {
      if (link.hash === "#" + view) link.setAttribute("aria-current", "page");
      else link.removeAttribute("aria-current");
    }
    $("view-title").textContent = titles[view];
    document.title = `${titles[view]} · Dipper`;
    document.querySelector(".view-scroll").scrollTop = 0;
    syncLiveVisuals();
  }

  function feedback(message, error = false) {
    const node = $("feedback");
    node.hidden = !message;
    node.textContent = message;
    node.classList.toggle("error-detail", error);
    node.setAttribute("role", error ? "alert" : "status");
  }

  async function call(name, args = {}) {
    if (!invoke) {
      throw new Error("Open Dipper in the desktop app to use native controls.");
    }
    return invoke(name, args);
  }

  function synchronizeControls() {
    const configurationPending = configurationActions.some((id) => pending.has(id));
    const fieldsDisabled = busy || configurationPending;
    $("settings-fields").disabled = fieldsDisabled;
    for (const card of document.querySelectorAll("[data-model-card]")) {
      card.disabled = fieldsDisabled;
    }
    for (const id of configurationActions) {
      if ($(id)) $(id).disabled = fieldsDisabled;
    }
    $("retry-recovery").disabled = busy || pending.has("retry-recovery");
    $("cancel").disabled = !busy || pending.has("cancel");
    $("cancel").hidden = !busy;
    $("hero-cancel").disabled = !busy || pending.has("cancel");
    $("hero-cancel").hidden = !busy;

    const rowsById = new Map(allRecordings.map((row) => [row.id, row]));
    for (const button of document.querySelectorAll("[data-recover-btn]")) {
      const row = rowsById.get(button.dataset.recordingId);
      const state = F.getRecoverActionState(row, {
        busy: busy || pending.has("retry-recovery"),
        pendingRecordingId: pendingRecoveryId,
      });
      button.disabled = state.disabled;
      button.textContent = state.label;
      button.title = state.title;
    }
  }

  function focusSettingsError(error) {
    const message = String(error).toLowerCase();
    const field = message.includes("model")
      ? $("model")
      : message.includes("runtime") || message.includes("librar")
        ? $("runtime")
        : message.includes("shortcut") || message.includes("hold")
          ? $("hold")
          : null;
    if (!field) return;
    if (location.hash !== "#settings") {
      location.hash = "#settings";
      navigate();
    }
    field.setAttribute("aria-invalid", "true");
    field.setAttribute("aria-errormessage", "feedback");
    field.focus();
  }

  async function action(id, task, success, settingsAction = false) {
    if (pending.has(id)) return;
    pending.add(id);
    synchronizeControls();
    const button = $(id);
    const originalLabel = button ? button.textContent : "";
    if (button && id === "save") button.textContent = "Saving…";
    if (button && id === "configure-hold") button.textContent = "Waiting for KDE…";
    let failure = null;
    try {
      await task();
      if (success) feedback(success);
    } catch (error) {
      failure = error;
      feedback(String(error), true);
    } finally {
      pending.delete(id);
      if (button && (id === "save" || id === "configure-hold")) {
        button.textContent = originalLabel;
      }
      synchronizeControls();
      if (failure && settingsAction) focusSettingsError(failure);
    }
  }

  function updatePrompt() {
    $("binding").textContent = binding || "Unassigned";
    $("hold-instruction").textContent = latestSnapshot.recording
      ? "Release to transcribe"
      : latestSnapshot.busy
        ? "Processing dictation"
        : binding
          ? "Hold to dictate"
          : "Configure your Hold key";
  }

  function applySnapshot(snapshot) {
    const wasBusy = busy;
    const wasRecording = Boolean(latestSnapshot.recording);
    latestSnapshot = snapshot;
    busy = Boolean(snapshot.busy);

    if (snapshot.recording && !wasRecording) {
      recordingFallbackStartMs = snapshot.recording_started_at_ms || Date.now();
    } else if (!snapshot.recording) {
      recordingFallbackStartMs = null;
      targetAudioLevel = 0;
    }

    $("phase").textContent = snapshot.phase;
    $("detail").textContent = snapshot.detail;
    $("detail").classList.toggle("error", Boolean(snapshot.error));

    const toneClass = snapshot.recording
      ? "recording"
      : busy
        ? "processing"
        : snapshot.error
          ? "error"
          : "ready";
    $("status-light").className = `status-light ${toneClass}`;

    const hero = $("dictation-hero");
    hero.classList.toggle("is-recording", Boolean(snapshot.recording));
    hero.classList.toggle("is-processing", Boolean(busy && !snapshot.recording));
    hero.classList.toggle("is-error", Boolean(snapshot.error && !busy));

    const heroPill = $("hero-state-pill");
    heroPill.className = `state-badge ${
      snapshot.recording
        ? "tone-recording"
        : busy
          ? "tone-warning"
          : snapshot.error
            ? "tone-danger"
            : "tone-success"
    }`;
    $("hero-phase-text").textContent = snapshot.phase;

    syncLiveVisuals();

    synchronizeControls();
    updatePrompt();
    updateLatestRecoveryCallout();

    if (wasBusy && !busy && initialized) {
      refreshJournal().catch((error) => feedback(String(error), true));
      scheduleSetupCheck(0);
    }
  }

  function updateRecognitionSummary() {
    const profileId = savedSettings?.model_profile || selectedModelProfile;
    const meta = F.getModelProfileMeta(profileId);
    $("recognition-summary").textContent = `${meta.shortLabel} · Auto language`;
    $("recognition-summary").title = meta.label;
  }

  function updateInputSummary() {
    if (!savedSettings || !inputCatalog) return;
    const name = savedSettings.input_name || inputCatalog.default_name;
    const device = inputCatalog.devices.find((input) => input.node_name === name);
    const label = savedSettings.input_name
      ? device?.description || `${name} (unavailable)`
      : `System default${device ? " · " + device.description : " (unavailable)"}`;
    $("input-summary").textContent = label;
    $("input-summary").title = label;
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
    if (previous && !catalog.devices.some((input) => input.node_name === previous)) {
      $("input").append(new Option(previous + " (unavailable)", previous));
    }
    $("input").value = previous;
    updateInputSummary();
  }

  function syncModelCardSelection() {
    for (const card of document.querySelectorAll("[data-model-card]")) {
      const selected = card.dataset.modelCard === selectedModelProfile;
      card.classList.toggle("is-selected", selected);
      card.setAttribute("aria-checked", String(selected));
    }
  }

  function selectModelProfile(profileId) {
    const modelPath = F.modelPathForProfile($("model").value, profileId);
    if (selectedModelProfile === profileId && modelPath === $("model").value) return;
    selectedModelProfile = profileId;
    const profile = F.getModelProfileMeta(profileId);
    $("model").placeholder = `/path/to/${profile.defaultFilename}`;
    $("model").value = modelPath;
    syncModelCardSelection();
    synchronizeControls();
    scheduleSetupCheck(0);
  }

  function renderModelCards() {
    const container = $("model-cards");
    const cards = [];
    for (const profile of F.MODEL_PROFILES) {
      const selected = profile.id === selectedModelProfile;
      const button = document.createElement("button");
      button.type = "button";
      button.className = `model-card${selected ? " is-selected" : ""}`;
      button.setAttribute("role", "radio");
      button.setAttribute("aria-checked", String(selected));
      button.dataset.modelCard = profile.id;

      const top = document.createElement("div");
      top.className = "model-card-top";
      const title = document.createElement("span");
      title.className = "model-card-title";
      title.textContent = profile.shortLabel;
      const badge = document.createElement("span");
      badge.className = `status-pill ${profile.recommended ? "tone-success" : "tone-muted"}`;
      badge.textContent = profile.badge;
      top.append(title, badge);

      const metrics = document.createElement("p");
      metrics.className = "model-card-metrics";
      metrics.textContent = profile.metrics;

      const desc = document.createElement("p");
      desc.className = "model-card-desc";
      desc.textContent = profile.description;

      button.append(top, metrics, desc);
      button.addEventListener("click", () => {
        if (button.disabled) return;
        selectModelProfile(profile.id);
      });
      button.addEventListener("keydown", (event) => {
        if (button.disabled) return;
        const idx = F.MODEL_PROFILES.findIndex((item) => item.id === profile.id);
        let nextIdx = -1;
        if (event.key === "ArrowRight" || event.key === "ArrowDown") {
          nextIdx = (idx + 1) % F.MODEL_PROFILES.length;
        } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
          nextIdx = (idx - 1 + F.MODEL_PROFILES.length) % F.MODEL_PROFILES.length;
        }
        if (nextIdx >= 0) {
          event.preventDefault();
          const nextProfile = F.MODEL_PROFILES[nextIdx];
          selectModelProfile(nextProfile.id);
          const nextCard = container.querySelector(`[data-model-card="${nextProfile.id}"]`);
          if (nextCard) nextCard.focus();
        }
      });
      cards.push(button);
    }
    container.replaceChildren(...cards);
  }

  function renderSetupStatus(setupStatus) {
    latestSetupStatus = setupStatus;
    const summary = F.summarizeSetupStatus(
      setupStatus,
      { model_profile: selectedModelProfile },
      binding
    );

    const stripNodes = summary.items.map((item) => {
      const link = document.createElement("a");
      link.href = "#settings";
      link.className = `setup-strip-item tone-${item.status}`;
      link.title = item.detail;

      const top = document.createElement("span");
      top.className = "setup-strip-head";
      const dot = document.createElement("span");
      dot.className = `setup-dot tone-${item.status}`;
      dot.setAttribute("aria-hidden", "true");
      const label = document.createElement("span");
      label.className = "setup-strip-label";
      label.textContent = item.label;
      top.append(dot, label);

      const value = document.createElement("span");
      value.className = "setup-strip-value";
      value.textContent = item.summary;

      link.append(top, value);
      return link;
    });
    $("setup-strip-items").replaceChildren(...stripNodes);

    const overallBadge = $("setup-overall-badge");
    if (setupStatus) {
      overallBadge.textContent = summary.ready ? "Ready" : "Attention needed";
      overallBadge.className = `status-pill ${summary.ready ? "tone-success" : "tone-warning"}`;
    } else {
      overallBadge.textContent = "Checking";
      overallBadge.className = "status-pill tone-muted";
    }

    const checkRows = [...summary.items, summary.recovery].map((item) => {
      const row = document.createElement("div");
      row.className = `setup-check-row tone-${item.status}`;

      const left = document.createElement("div");
      left.className = "setup-check-left";
      const dot = document.createElement("span");
      dot.className = `setup-dot tone-${item.status}`;
      dot.setAttribute("aria-hidden", "true");
      const textWrap = document.createElement("div");
      textWrap.className = "setup-check-text";
      const label = document.createElement("span");
      label.className = "setup-check-label";
      label.textContent = item.label;
      const detail = document.createElement("span");
      detail.className = "setup-check-detail";
      detail.textContent = item.detail;
      textWrap.append(label, detail);
      left.append(dot, textWrap);

      const badge = document.createElement("span");
      badge.className = `status-pill ${
        item.status === "ok"
          ? "tone-success"
          : item.status === "error"
            ? "tone-danger"
            : item.status === "warn"
              ? "tone-warning"
              : "tone-muted"
      }`;
      badge.textContent = item.summary;

      row.append(left, badge);
      return row;
    });
    $("setup-check-rows").replaceChildren(...checkRows);

    $("retry-recovery").classList.toggle(
      "is-attention",
      Boolean(setupStatus?.recovery_unpersisted)
    );
  }

  async function runSetupCheck() {
    if (!invoke) return;
    const seq = ++setupCheckSeq;
    try {
      const modelInput = $("model");
      const runtimeInput = $("runtime");
      const modelPath = modelInput ? modelInput.value.trim() : null;
      const nativeLibDir = runtimeInput ? runtimeInput.value.trim() : null;
      const status = await call("check_setup_paths", {
        modelProfile: selectedModelProfile,
        modelPath,
        nativeLibDir,
      });
      if (seq !== setupCheckSeq) return;
      binding = status.effective_binding || null;
      updatePrompt();
      renderSetupStatus(status);
    } catch (error) {
      if (seq !== setupCheckSeq) return;
      const fallback = {
        model_exists: false,
        model_size_bytes: null,
        runtime_exists: false,
        desktop_ready: false,
        desktop_error: String(error),
        effective_binding: binding,
        recovery_unpersisted: false,
        ready: false,
        issue: String(error),
      };
      renderSetupStatus(fallback);
    }
  }

  function scheduleSetupCheck(delayMs = 250) {
    if (setupCheckTimer !== null) {
      clearTimeout(setupCheckTimer);
      setupCheckTimer = null;
    }
    if (delayMs <= 0) {
      runSetupCheck();
      return;
    }
    setupCheckTimer = setTimeout(() => {
      setupCheckTimer = null;
      runSetupCheck();
    }, delayMs);
  }

  function updateLatestRecoveryCallout() {
    const prompt = F.getLatestRecoveryPrompt(allRecordings, latestSnapshot);
    const box = $("dictation-recovery");
    if (!prompt) {
      box.hidden = true;
      return;
    }
    box.hidden = false;
    box.classList.toggle("is-recoverable", prompt.kind === "recoverable");
    $("dictation-recovery-title").textContent = prompt.title;
    $("dictation-recovery-message").textContent = prompt.message;
    $("dictation-recovery-action").textContent = prompt.actionLabel;
    $("dictation-recovery-action").dataset.targetFilter =
      F.needsRecovery(prompt.recording) ? "recovery" : "all";
  }

  function setJournalFilter(filter) {
    journalFilter = filter;
    for (const chip of document.querySelectorAll("[data-journal-filter]")) {
      const active = chip.dataset.journalFilter === filter;
      chip.classList.toggle("is-active", active);
      chip.setAttribute("aria-pressed", String(active));
    }
    renderFilteredJournal();
  }

  function mountAudioPlayer(slot, recordingId, preview, onClose) {
    const bytes = decodeBase64ToBytes(preview.wav_base64);
    const blob = new Blob([bytes], { type: "audio/wav" });
    const url = URL.createObjectURL(blob);
    audioRegistry.register(recordingId, url);

    const wrap = document.createElement("div");
    wrap.className = "audio-player-box";

    const top = document.createElement("div");
    top.className = "audio-player-meta";

    const info = document.createElement("div");
    info.className = "audio-player-labels";
    const summary = document.createElement("span");
    summary.className = "audio-player-duration";
    summary.textContent = `${F.formatDuration(preview.duration_ms)} · ${Math.round((preview.sample_rate || 16000) / 1000)} kHz WAV`;
    info.append(summary);

    if (preview.recovered) {
      const pill = document.createElement("span");
      pill.className = "status-pill tone-recovered";
      pill.textContent = "Recovered";
      info.append(pill);
    }
    if (preview.truncated) {
      const pill = document.createElement("span");
      pill.className = "status-pill tone-warning";
      pill.textContent = "Truncated";
      info.append(pill);
    }

    const close = document.createElement("button");
    close.type = "button";
    close.className = "quiet-button compact-button";
    close.textContent = "Close audio";

    top.append(info, close);

    const audio = document.createElement("audio");
    audio.controls = true;
    audio.preload = "metadata";
    audio.className = "audio-element";
    audio.src = url;

    close.addEventListener("click", () => {
      audio.pause();
      audio.removeAttribute("src");
      audio.load();
      audioRegistry.revoke(recordingId);
      slot.replaceChildren();
      slot.hidden = true;
      if (typeof onClose === "function") onClose();
    });

    wrap.append(top, audio);
    slot.replaceChildren(wrap);
    slot.hidden = false;
  }

  function buildDisclosureSummary(text) {
    const summary = document.createElement("summary");
    const label = document.createElement("span");
    label.textContent = text;
    summary.append(label, createIcon("icon-chevron-down", "icon icon-xs disclosure-chevron"));
    return summary;
  }

  function buildDiagnosticsDisclosure(recording) {
    const diag = recording.capture_diagnostics;
    if (
      !diag &&
      !(recording.available_frames > 0) &&
      !(recording.accepted_frames > 0)
    ) {
      return null;
    }

    const details = document.createElement("details");
    details.className = "card-disclosure";
    details.append(buildDisclosureSummary("Capture diagnostics"));

    const grid = document.createElement("dl");
    grid.className = "diag-grid";

    const addMetric = (label, value, isError = false) => {
      if (value === null || value === undefined || value === "") return;
      const item = document.createElement("div");
      item.className = "diag-item";
      const dt = document.createElement("dt");
      dt.textContent = label;
      const dd = document.createElement("dd");
      if (isError) dd.className = "diag-error";
      dd.textContent = String(value);
      item.append(dt, dd);
      grid.append(item);
    };

    const durationMs = diag?.duration_ms || recording.duration_ms;
    addMetric("Duration", F.formatDuration(durationMs, recording.available_frames));
    addMetric(
      "Frames (avail / acc / sync)",
      `${F.formatFrames(recording.available_frames)} / ${F.formatFrames(recording.accepted_frames)} / ${F.formatFrames(recording.synced_frames)}`
    );
    addMetric("Availability", recording.availability);

    if (diag) {
      if (diag.ring_capacity_frames > 0) {
        addMetric(
          "Ring high-water",
          `${F.formatFrames(diag.ring_high_water_frames)} / ${F.formatFrames(diag.ring_capacity_frames)}`
        );
      }
      addMetric(
        "PipeWire callbacks",
        `${F.formatFrames(diag.callbacks)} (${F.formatFrames(diag.empty_buffers)} empty)`
      );
      if (diag.negotiated_rate || diag.negotiated_channels) {
        addMetric(
          "Negotiated stream",
          `${diag.negotiated_rate ?? "?"} Hz · ${diag.negotiated_channels ?? "?"} ch`
        );
      }
      if (diag.first_frame_ms !== null && diag.first_frame_ms !== undefined) {
        addMetric("First audio frame", F.formatLatencyMs(diag.first_frame_ms));
      }
      if (Array.isArray(diag.stream_states) && diag.stream_states.length > 0) {
        addMetric("Stream states", diag.stream_states.join(" -> "));
      }
      addMetric("Discontinuity flags", String(diag.discontinuity_flags ?? 0));
      if (diag.pipewire_error) {
        addMetric("PipeWire error", diag.pipewire_error, true);
      }
    }

    if (recording.release_to_result_ms !== null && recording.release_to_result_ms !== undefined) {
      addMetric("Release to result", F.formatLatencyMs(recording.release_to_result_ms));
    }

    details.append(grid);
    return details;
  }

  function buildAttemptsDisclosure(attempts) {
    if (!Array.isArray(attempts) || attempts.length === 0) return null;

    const details = document.createElement("details");
    details.className = "card-disclosure";
    details.append(buildDisclosureSummary(`Attempts (${attempts.length})`));

    const list = document.createElement("div");
    list.className = "attempts-list";

    attempts.forEach((attempt, idx) => {
      const item = document.createElement("div");
      item.className = "attempt-item";

      const head = document.createElement("div");
      head.className = "attempt-head";

      const num = document.createElement("span");
      num.className = "attempt-index";
      num.textContent = `#${attempts.length - idx}`;

      const statePill = document.createElement("span");
      const stateTone =
        attempt.state === "complete"
          ? "tone-success"
          : attempt.state === "failed"
            ? "tone-danger"
            : "tone-warning";
      statePill.className = `status-pill ${stateTone}`;
      statePill.textContent = String(attempt.state || "unknown").replace(/_/g, " ");

      head.append(num, statePill);

      if (attempt.retry_from_journal) {
        const recoveryTag = document.createElement("span");
        recoveryTag.className = "status-pill tone-recovered";
        recoveryTag.textContent = "Journal recovery";
        head.append(recoveryTag);
      }

      if (attempt.model_profile) {
        const modelTag = document.createElement("span");
        modelTag.className = "meta-tag";
        modelTag.textContent = F.getModelProfileMeta(attempt.model_profile).shortLabel;
        head.append(modelTag);
      }

      const prep = F.formatLatencyMs(attempt.preparation_ms);
      const inf = F.formatLatencyMs(attempt.inference_ms);
      if (prep || inf) {
        const timing = document.createElement("span");
        timing.className = "attempt-timing";
        timing.textContent = [prep ? `Prep ${prep}` : null, inf ? `Inference ${inf}` : null]
          .filter(Boolean)
          .join(" · ");
        head.append(timing);
      }

      item.append(head);

      if (attempt.raw_text) {
        const text = document.createElement("p");
        text.className = "attempt-text";
        text.textContent = attempt.raw_text;
        item.append(text);
      } else if (attempt.error) {
        const err = document.createElement("p");
        err.className = "attempt-error";
        err.textContent = attempt.error;
        item.append(err);
      }

      list.append(item);
    });

    details.append(list);
    return details;
  }

  function buildRecordingCard(recording) {
    const recovered = F.isRecoveredRecording(recording);
    const failure = F.getFailurePresentation(recording);
    const card = document.createElement("article");
    card.className = "journal-card";
    card.dataset.recordingId = recording.id;
    card.tabIndex = -1;
    if (recovered) card.classList.add("is-recovered");
    else if (failure) card.classList.add(`is-${failure.tone}`);

    const top = document.createElement("div");
    top.className = "journal-card-top";

    const metaBlock = document.createElement("div");
    metaBlock.className = "journal-card-meta";

    const ts = F.formatTimestamp(recording.started_at);
    const time = document.createElement("time");
    if (ts.iso) time.dateTime = ts.iso;
    if (ts.title) time.title = ts.title;
    time.textContent = ts.label;
    metaBlock.append(time);

    const subMeta = document.createElement("div");
    subMeta.className = "journal-submeta";

    const durationSpan = document.createElement("span");
    durationSpan.className = "meta-tag tabular";
    durationSpan.textContent = F.formatDuration(recording.duration_ms, recording.available_frames);
    subMeta.append(durationSpan);

    const frameSpan = document.createElement("span");
    frameSpan.className = "meta-tag tabular";
    frameSpan.textContent = F.formatFrameSummary(recording);
    subMeta.append(frameSpan);

    if (recording.model_profile) {
      const modelTag = document.createElement("span");
      modelTag.className = "meta-tag";
      modelTag.textContent = F.getModelProfileMeta(recording.model_profile).shortLabel;
      subMeta.append(modelTag);
    }

    const inferenceLabel = F.formatLatencyMs(recording.inference_ms);
    if (inferenceLabel) {
      const infTag = document.createElement("span");
      infTag.className = "meta-tag tabular";
      infTag.textContent = `Inference ${inferenceLabel}`;
      subMeta.append(infTag);
    }

    if (recording.input_device?.description) {
      const devTag = document.createElement("span");
      devTag.className = "meta-tag meta-device";
      devTag.textContent = recording.input_device.description;
      devTag.title = recording.input_device.name || recording.input_device.description;
      subMeta.append(devTag);
    }

    metaBlock.append(subMeta);

    const actions = document.createElement("div");
    actions.className = "journal-card-actions";

    const audioSlot = document.createElement("div");
    audioSlot.className = "audio-player-slot";
    audioSlot.hidden = true;

    if (recording.has_audio) {
      const listen = document.createElement("button");
      listen.type = "button";
      listen.className = "quiet-button compact-button listen-button";
      listen.append(createIcon("icon-play", "icon icon-xs"));
      const listenLabel = document.createElement("span");
      listenLabel.textContent = "Listen";
      listen.append(listenLabel);

      listen.addEventListener("click", async () => {
        if (!audioSlot.hidden && audioRegistry.has(recording.id)) {
          const existing = audioSlot.querySelector("audio");
          if (existing) {
            existing.pause();
            existing.removeAttribute("src");
            existing.load();
          }
          audioRegistry.revoke(recording.id);
          audioSlot.replaceChildren();
          audioSlot.hidden = true;
          listenLabel.textContent = "Listen";
          return;
        }
        if (listen.disabled) return;
        listen.disabled = true;
        listenLabel.textContent = "Loading…";
        try {
          const preview = await call("get_recording_audio", { recordingId: recording.id });
          if (!audioSlot.isConnected) return;
          mountAudioPlayer(audioSlot, recording.id, preview, () => {
            listenLabel.textContent = "Listen";
          });
          listenLabel.textContent = "Hide audio";
        } catch (error) {
          if (audioSlot.isConnected) {
            feedback(`Could not load audio preview: ${String(error)}`, true);
            listenLabel.textContent = "Listen";
          }
        } finally {
          if (audioSlot.isConnected) {
            listen.disabled = false;
          }
        }
      });
      actions.append(listen);
    }

    const hasTranscriptText =
      typeof recording.raw_text === "string" && recording.raw_text.trim().length > 0;
    const recoverState = F.getRecoverActionState(recording, {
      busy: busy || pending.has("retry-recovery"),
      pendingRecordingId: pendingRecoveryId,
    });
    if (recoverState.visible) {
      const recoverBtn = document.createElement("button");
      recoverBtn.type = "button";
      recoverBtn.className = hasTranscriptText
        ? "quiet-button compact-button"
        : "secondary-button compact-button recover-button";
      recoverBtn.dataset.recoverBtn = "true";
      recoverBtn.dataset.recordingId = recording.id;
      recoverBtn.textContent = recoverState.label;
      recoverBtn.disabled = recoverState.disabled;
      recoverBtn.title = recoverState.title;

      recoverBtn.addEventListener("click", async () => {
        if (recoverBtn.disabled || busy || pendingRecoveryId || pending.has("retry-recovery")) return;
        pendingRecoveryId = recording.id;
        synchronizeControls();
        feedback("Transcribing retained audio in Journal without touching clipboard…");
        try {
          await call("retry_recognition", { recordingId: recording.id });
          await refreshJournal();
          // A transcribed row leaves the Recovery filter; keep it in view.
          if (journalFilter === "recovery") setJournalFilter("all");
          document
            .querySelector(`.journal-card[data-recording-id="${CSS.escape(recording.id)}"]`)
            ?.focus();
          scheduleSetupCheck(0);
          feedback("Recording transcribed in Journal. Clipboard unchanged.");
        } catch (error) {
          await refreshJournal().catch(() => {});
          feedback(String(error), true);
        } finally {
          pendingRecoveryId = null;
          synchronizeControls();
        }
      });
      actions.append(recoverBtn);
    }

    if (hasTranscriptText) {
      const copy = document.createElement("button");
      copy.type = "button";
      copy.className = "secondary-button compact-button";
      copy.textContent = "Copy";
      copy.addEventListener("click", async () => {
        if (copy.disabled) return;
        copy.disabled = true;
        try {
          await call("copy_transcript", { recordingId: recording.id });
          feedback("Transcript copied.");
        } catch (error) {
          feedback(String(error), true);
        } finally {
          copy.disabled = false;
        }
      });
      actions.append(copy);
    }

    top.append(metaBlock, actions);
    card.append(top);

    const pillsRow = document.createElement("div");
    pillsRow.className = "pills-row";
    for (const pill of F.getRecordingPills(recording)) {
      const span = document.createElement("span");
      span.className = `status-pill tone-${pill.tone}`;
      span.textContent = pill.text;
      pillsRow.append(span);
    }
    card.append(pillsRow);

    if (failure) {
      const banner = document.createElement("div");
      banner.className = `failure-banner tone-${failure.tone}`;

      const badge = document.createElement("span");
      badge.className = `status-pill tone-${failure.tone}`;
      badge.textContent = failure.label;

      const hint = document.createElement("p");
      hint.className = "failure-hint";
      hint.textContent = failure.hint;

      banner.append(badge, hint);
      card.append(banner);
    }

    if (hasTranscriptText) {
      const transcript = document.createElement("p");
      transcript.className = "transcript";
      transcript.textContent = recording.raw_text;
      card.append(transcript);
    }

    if (recording.detail && (!failure || recording.detail !== failure.hint)) {
      const detail = document.createElement("p");
      detail.className = "entry-detail";
      detail.textContent = recording.detail;
      card.append(detail);
    }

    card.append(audioSlot);

    const diagDisclosure = buildDiagnosticsDisclosure(recording);
    const attemptsDisclosure = buildAttemptsDisclosure(recording.attempts);
    if (diagDisclosure || attemptsDisclosure) {
      const disclosures = document.createElement("div");
      disclosures.className = "card-disclosures";
      if (diagDisclosure) disclosures.append(diagDisclosure);
      if (attemptsDisclosure) disclosures.append(attemptsDisclosure);
      card.append(disclosures);
    }

    return card;
  }

  function renderFilteredJournal() {
    const root = $("journal");
    if (root) {
      for (const audio of root.querySelectorAll("audio")) {
        audio.pause();
        audio.removeAttribute("src");
        audio.load();
      }
    }
    audioRegistry.revokeAll();

    const recoveryCount = allRecordings.filter((row) => F.needsRecovery(row)).length;
    const completedCount = allRecordings.filter((row) => F.isCompletedRecording(row)).length;

    $("filter-count-all").textContent = String(allRecordings.length);
    $("filter-count-recovery").textContent = String(recoveryCount);
    $("filter-count-completed").textContent = String(completedCount);

    const navBadge = $("nav-recovery-badge");
    navBadge.textContent = String(recoveryCount);
    navBadge.hidden = recoveryCount === 0;

    const filtered = F.filterJournalRecordings(allRecordings, {
      filter: journalFilter,
      query: journalSearch,
    });

    const isFiltered = journalFilter !== "all" || journalSearch.trim().length > 0;
    $("journal-count").textContent = isFiltered
      ? `Showing ${filtered.length} of ${allRecordings.length} recording${allRecordings.length === 1 ? "" : "s"}`
      : `${allRecordings.length} recent recording${allRecordings.length === 1 ? "" : "s"}`;

    if (!allRecordings.length) {
      const empty = document.createElement("div");
      empty.className = "empty";
      const message = document.createElement("p");
      message.textContent = "No recordings yet. Hold your configured key in any text field to dictate.";
      const link = document.createElement("a");
      link.href = "#dictation";
      link.className = "text-link empty-action";
      link.textContent = "Go to Dictation";
      empty.append(message, link);
      root.replaceChildren(empty);
      return;
    }

    if (!filtered.length) {
      const empty = document.createElement("div");
      empty.className = "empty";
      const message = document.createElement("p");
      message.textContent = "No recordings match your current filter or search.";
      const reset = document.createElement("button");
      reset.type = "button";
      reset.className = "secondary-button empty-action";
      reset.textContent = "Clear filters";
      reset.addEventListener("click", () => {
        journalSearch = "";
        $("journal-search").value = "";
        setJournalFilter("all");
      });
      empty.append(message, reset);
      root.replaceChildren(empty);
      return;
    }

    // Rebuilding replaces every card; keep focus on the same recording.
    const focusedId = document.activeElement?.closest?.(".journal-card")?.dataset.recordingId;
    const cards = filtered.map((recording) => buildRecordingCard(recording));
    root.replaceChildren(...cards);
    if (focusedId) cards.find((card) => card.dataset.recordingId === focusedId)?.focus();
  }

  function renderJournal(recordings) {
    allRecordings = Array.isArray(recordings) ? recordings : [];
    lastTranscript.update(allRecordings);
    updateLatestRecoveryCallout();
    renderFilteredJournal();
  }

  // Callers share one in-flight refresh and resolve only after its final
  // render, including reruns requested while it was loading.
  function refreshJournal() {
    journalDirty = true;
    journalRefresh ??= drainJournalRefresh().finally(() => {
      journalRefresh = null;
    });
    return journalRefresh;
  }

  async function drainJournalRefresh() {
    $("journal").setAttribute("aria-busy", "true");
    try {
      while (journalDirty) {
        journalDirty = false;
        renderJournal(await call("recent_recordings"));
      }
    } catch (error) {
      allRecordings = [];
      lastTranscript.unavailable("Open Journal and retry loading.");
      updateLatestRecoveryCallout();
      $("journal-count").textContent = "Recordings unavailable";
      const message = document.createElement("p");
      message.className = "empty";
      message.textContent = `Could not load recordings: ${String(error)}`;
      const retry = document.createElement("button");
      retry.className = "secondary-button";
      retry.textContent = "Retry loading";
      retry.type = "button";
      retry.addEventListener("click", () =>
        refreshJournal()
          .then(() => feedback("Recordings loaded."))
          .catch((failure) => feedback(String(failure), true))
      );
      $("journal").replaceChildren(message, retry);
      throw error;
    } finally {
      $("journal").setAttribute("aria-busy", "false");
    }
  }

  window.addEventListener("hashchange", navigate);
  navigate();
  buildWaveformBars();
  renderModelCards();
  renderSetupStatus(null);

  document.querySelector(".skip-link").addEventListener("click", (event) => {
    event.preventDefault();
    $("main-content").focus();
  });

  for (const chip of document.querySelectorAll("[data-journal-filter]")) {
    chip.addEventListener("click", () => {
      setJournalFilter(chip.dataset.journalFilter);
    });
  }

  $("journal-search").addEventListener("input", (event) => {
    journalSearch = event.target.value;
    renderFilteredJournal();
  });

  $("dictation-recovery-action").addEventListener("click", () => {
    const targetFilter = $("dictation-recovery-action").dataset.targetFilter || "recovery";
    journalSearch = "";
    $("journal-search").value = "";
    setJournalFilter(targetFilter);
    location.hash = "#journal";
  });

  $("retry-recovery").addEventListener("click", () =>
    action(
      "retry-recovery",
      async () => {
        await call("retry_recovery");
        await refreshJournal();
        await runSetupCheck();
      },
      "Storage reconciled. No text was inserted."
    )
  );

  $("cancel").addEventListener("click", () => action("cancel", () => call("cancel_dictation")));
  $("hero-cancel").addEventListener("click", () => action("cancel", () => call("cancel_dictation")));

  $("refresh").addEventListener("click", () =>
    action("refresh", () => refreshInputs(), "Input devices refreshed.")
  );

  $("recheck-setup").addEventListener("click", () =>
    action("recheck-setup", () => runSetupCheck(), "Setup paths checked.")
  );

  $("model").addEventListener("input", () => {
    $("model").removeAttribute("aria-invalid");
    $("model").removeAttribute("aria-errormessage");
    scheduleSetupCheck(250);
  });
  $("model").addEventListener("change", () => scheduleSetupCheck(0));
  $("runtime").addEventListener("input", () => {
    $("runtime").removeAttribute("aria-invalid");
    $("runtime").removeAttribute("aria-errormessage");
    scheduleSetupCheck(250);
  });
  $("runtime").addEventListener("change", () => scheduleSetupCheck(0));
  $("hold").addEventListener("input", () => {
    $("hold").removeAttribute("aria-invalid");
    $("hold").removeAttribute("aria-errormessage");
  });

  $("settings").addEventListener("submit", (event) => {
    event.preventDefault();
    if (busy || configurationActions.some((id) => pending.has(id))) return;
    for (const input of $("settings").querySelectorAll("input")) {
      input.removeAttribute("aria-invalid");
      input.removeAttribute("aria-errormessage");
    }
    const invalid = [...$("settings").querySelectorAll("input")].find(
      (input) => !input.checkValidity()
    );
    if (invalid) {
      invalid.setAttribute("aria-invalid", "true");
      invalid.setAttribute("aria-errormessage", "feedback");
      feedback(
        `Enter ${invalid.labels[0].textContent.toLowerCase()}${invalid.validity.patternMismatch ? " as an absolute path" : ""}.`,
        true
      );
      invalid.reportValidity();
      invalid.focus();
      return;
    }
    const settings = {
      input_name: $("input").value || null,
      model_profile: selectedModelProfile,
      model_path: $("model").value.trim(),
      native_lib_dir: $("runtime").value.trim(),
      preferred_hold: $("hold").value.trim(),
    };
    action(
      "save",
      async () => {
        await call("save_settings", { settings });
        savedSettings = settings;
        updateInputSummary();
        updateRecognitionSummary();
        await runSetupCheck();
      },
      "Settings saved. Changes apply to the next dictation.",
      true
    );
  });

  $("configure-hold").addEventListener("click", () => {
    $("hold").removeAttribute("aria-invalid");
    $("hold").removeAttribute("aria-errormessage");
    action(
      "configure-hold",
      async () => {
        binding = await call("configure_hold", { preferred: $("hold").value.trim() });
        updatePrompt();
        await runSetupCheck();
      },
      "Hold shortcut configured.",
      true
    );
  });

  $("kde-shortcuts").addEventListener("click", () =>
    action("kde-shortcuts", () => call("open_shortcut_settings"))
  );

  async function initialize() {
    await window.__TAURI__?.event.listen("navigate-view", (event) => {
      if (Object.hasOwn(titles, event.payload)) {
        location.hash = event.payload;
      }
    });
    await window.__TAURI__?.event.listen("dictation-state", (event) => {
      snapshotSeq += 1;
      applySnapshot(event.payload);
    });
    await window.__TAURI__?.event.listen("dictation-level", (event) => {
      const level = Number(event.payload);
      targetAudioLevel = Number.isFinite(level) ? Math.min(1, Math.max(0, level)) : 0;
      if (canAnimateLiveVisuals() && (latestSnapshot.recording || targetAudioLevel > 0)) {
        ensureWaveformRunning();
      }
    });

    savedSettings = await call("get_settings");
    selectedModelProfile = savedSettings.model_profile || "large_v3_turbo_q8_0";
    renderModelCards();
    updateRecognitionSummary();

    const profileMeta = F.getModelProfileMeta(selectedModelProfile);
    $("model").placeholder = `/path/to/${profileMeta.defaultFilename}`;
    $("model").value = savedSettings.model_path || "";
    $("runtime").value = savedSettings.native_lib_dir || "";
    $("hold").value = savedSettings.preferred_hold || "F2";

    await refreshInputs(savedSettings.input_name ?? "").catch((error) => {
      $("input-summary").textContent = "Input devices unavailable. Refresh in Settings.";
      if (savedSettings.input_name) {
        $("input").append(
          new Option(savedSettings.input_name + " (availability unknown)", savedSettings.input_name)
        );
        $("input").value = savedSettings.input_name;
      }
      feedback(`Could not list input devices: ${String(error)}. Use Refresh devices to retry.`, true);
    });

    binding = await call("effective_binding");
    const initSeq = snapshotSeq;
    const initialSnapshot = await call("get_snapshot");
    if (initSeq === snapshotSeq) {
      applySnapshot(initialSnapshot);
    }
    initialized = true;

    await Promise.all([
      refreshJournal().catch((error) => feedback(String(error), true)),
      runSetupCheck(),
    ]);
  }

  document.addEventListener("visibilitychange", syncLiveVisuals);

  initialize().catch((error) => {
    $("phase").textContent = "Setup needed";
    $("detail").textContent = String(error);
    feedback(String(error), true);
  });

  window.addEventListener("dipper-window-error", (event) => feedback(event.detail, true));
  window.addEventListener("pagehide", () => {
    audioRegistry.revokeAll();
    if (timerIntervalId !== null) clearInterval(timerIntervalId);
    if (waveRafId !== null) cancelAnimationFrame(waveRafId);
  });

  // Read the packaged host version; do not maintain a separate frontend version.
  window.__TAURI__?.app
    .getVersion()
    .then((version) => {
      $("app-version").textContent = `v${version}`;
      $("app-version").setAttribute("aria-label", `Dipper version ${version}`);
      $("app-version").title = `Dipper ${version}`;
    })
    .catch(() => {
      $("app-version").textContent = "Version unavailable";
    });
})();
