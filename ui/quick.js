(() => {
  "use strict";

  const F = window.DipperFormat;
  const $ = (id) => document.getElementById(id);
  const invoke = window.__TAURI__?.core.invoke;
  const QUICK_BAR_COUNT = 16;

  let refreshing = false;
  let dirty = false;
  let stateVersion = 0;
  let latestSnapshot = { phase: "Connecting", busy: false, recording: false, error: false };
  let latestRows = [];
  let currentBinding = null;

  const waveBars = [];
  let targetAudioLevel = 0;
  let smoothedAudioLevel = 0;
  let waveRafId = null;
  let timerIntervalId = null;
  let fallbackStartMs = null;

  function feedback(message, error = false) {
    const node = $("feedback");
    node.hidden = !message;
    node.textContent = message;
    node.setAttribute("role", error ? "alert" : "status");
    node.classList.toggle("error-detail", error);
  }

  async function call(name, args = {}) {
    if (!invoke) {
      throw new Error("Open the tray panel in Dipper to use native controls.");
    }
    return invoke(name, args);
  }

  const lastTranscript = window.DipperTranscript.mount(
    $("last-transcript"),
    call,
    feedback,
    "quick"
  );

  function buildWaveform() {
    const root = $("quick-waveform");
    if (!root) return;
    const nodes = [];
    waveBars.length = 0;
    for (let i = 0; i < QUICK_BAR_COUNT; i += 1) {
      const bar = document.createElement("span");
      bar.className = "wave-bar";
      bar.style.transform = "scaleY(0.160)";
      waveBars.push(bar);
      nodes.push(bar);
    }
    root.replaceChildren(...nodes);
  }

  function updateQuickTimer() {
    const timer = $("quick-timer");
    if (!timer) return;
    if (!latestSnapshot.recording) {
      timer.hidden = true;
      timer.textContent = "00:00";
      return;
    }
    timer.hidden = false;
    const startMs = latestSnapshot.recording_started_at_ms ?? fallbackStartMs;
    timer.textContent = F.formatElapsedTimer(F.computeElapsedMs(startMs, Date.now()));
  }

  function stepWaveform(now) {
    waveRafId = null;
    const isRecording = Boolean(latestSnapshot.recording);
    if (isRecording) {
      const rate = targetAudioLevel > smoothedAudioLevel ? 0.44 : 0.17;
      smoothedAudioLevel += (targetAudioLevel - smoothedAudioLevel) * rate;
      updateQuickTimer();
    } else {
      targetAudioLevel = 0;
      smoothedAudioLevel *= 0.78;
    }

    const active = isRecording || smoothedAudioLevel >= 0.008;
    const half = (QUICK_BAR_COUNT - 1) / 2;
    for (let i = 0; i < waveBars.length; i += 1) {
      if (!active) {
        waveBars[i].style.transform = "scaleY(0.160)";
        continue;
      }
      const centerDist = Math.abs(i - half) / half;
      const envelope = 1 - centerDist * centerDist * 0.54;
      const ripple = 0.72 + 0.28 * Math.sin(now * 0.012 + i * 0.8);
      const scale = Math.min(1, 0.16 + smoothedAudioLevel * envelope * ripple * 0.9);
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

  function syncQuickLiveVisuals() {
    if (latestSnapshot.recording && !document.hidden) {
      if (timerIntervalId === null) {
        timerIntervalId = setInterval(updateQuickTimer, 250);
      }
      updateQuickTimer();
      ensureWaveformRunning();
    } else {
      if (timerIntervalId !== null) {
        clearInterval(timerIntervalId);
        timerIntervalId = null;
      }
      if (document.hidden) {
        if (waveRafId !== null) {
          cancelAnimationFrame(waveRafId);
          waveRafId = null;
        }
        smoothedAudioLevel = 0;
        for (const bar of waveBars) {
          bar.style.transform = "scaleY(0.160)";
        }
      } else {
        updateQuickTimer();
        if (smoothedAudioLevel > 0) {
          ensureWaveformRunning();
        }
      }
    }
  }

  function updateMiddleCard() {
    const prompt = F.getLatestRecoveryPrompt(latestRows, latestSnapshot);
    const recoveryEl = $("quick-recovery");
    const transcriptEl = $("last-transcript");
    if (prompt) {
      recoveryEl.hidden = false;
      transcriptEl.hidden = true;
      recoveryEl.classList.toggle("is-recoverable", prompt.kind === "recoverable");
      $("quick-recovery-title").textContent = prompt.title;
      $("quick-recovery-message").textContent = prompt.message;
      $("open-journal-quick").textContent = prompt.quickActionLabel || "Open in Journal";
    } else {
      recoveryEl.hidden = true;
      transcriptEl.hidden = false;
    }
  }

  function updateHoldPrompt() {
    $("binding").textContent = currentBinding || "Unassigned";
    $("quick-hold-instruction").textContent = latestSnapshot.recording
      ? "Release to transcribe"
      : latestSnapshot.busy
        ? "Processing dictation"
        : currentBinding
          ? "Hold to dictate"
          : "Configure Hold key";
  }

  function state(snapshot) {
    const wasRecording = Boolean(latestSnapshot.recording);
    latestSnapshot = snapshot;

    if (snapshot.recording && !wasRecording) {
      fallbackStartMs = snapshot.recording_started_at_ms || Date.now();
    } else if (!snapshot.recording) {
      fallbackStartMs = null;
      targetAudioLevel = 0;
    }

    $("phase").textContent = snapshot.phase;
    const toneClass = snapshot.recording
      ? "recording"
      : snapshot.busy
        ? "processing"
        : snapshot.error
          ? "error"
          : "ready";
    $("status-light").className = `status-light ${toneClass}`;
    $("quick-hero").classList.toggle("is-recording", Boolean(snapshot.recording));
    $("quick-hero").classList.toggle(
      "is-processing",
      Boolean(snapshot.busy && !snapshot.recording)
    );
    $("quick-hero").classList.toggle("is-error", Boolean(snapshot.error && !snapshot.busy));

    const busy = Boolean(snapshot.busy);
    $("open-app").disabled = busy;
    $("open-settings").disabled = busy;
    $("open-journal-quick").disabled = busy;
    $("microphone").disabled = busy;
    $("cancel-quick").hidden = !busy;
    $("cancel-quick").disabled = !busy;

    syncQuickLiveVisuals();

    updateHoldPrompt();
    updateMiddleCard();
  }

  async function refresh() {
    dirty = true;
    if (refreshing) return;
    refreshing = true;
    try {
      do {
        dirty = false;
        const version = stateVersion;
        const snapshot = await call("get_snapshot");
        if (version === stateVersion) state(snapshot);

        const settings = await call("get_settings");
        currentBinding = await call("effective_binding");
        updateHoldPrompt();

        try {
          const catalog = await call("list_inputs");
          const name = settings.input_name || catalog.default_name;
          const device = catalog.devices.find((input) => input.node_name === name);
          $("input-summary").textContent =
            device?.description ||
            (settings.input_name ? `${name} (unavailable)` : "System default (unavailable)");
        } catch {
          $("input-summary").textContent = "Input devices unavailable";
        }
        $("input-summary").title = $("input-summary").textContent;

        try {
          latestRows = await call("recent_recordings");
          lastTranscript.update(latestRows);
          updateMiddleCard();
        } catch (error) {
          latestRows = [];
          lastTranscript.unavailable("Open Journal to retry.");
          updateMiddleCard();
          feedback(String(error), true);
        }
      } while (dirty);
    } catch (error) {
      feedback(String(error), true);
    } finally {
      refreshing = false;
    }
  }

  async function action(button, name, args = {}) {
    if (button.disabled) return;
    button.disabled = true;
    try {
      await call(name, args);
    } catch (error) {
      feedback(String(error), true);
    } finally {
      button.disabled = false;
      state(latestSnapshot);
    }
  }

  buildWaveform();

  $("quick-close").addEventListener("click", () => action($("quick-close"), "hide_quick"));
  $("microphone").addEventListener("click", () =>
    action($("microphone"), "open_main", { view: "settings" })
  );
  $("open-journal-quick").addEventListener("click", () =>
    action($("open-journal-quick"), "open_main", { view: "journal" })
  );
  $("open-app").addEventListener("click", () => action($("open-app"), "open_main", { view: null }));
  $("open-settings").addEventListener("click", () =>
    action($("open-settings"), "open_main", { view: "settings" })
  );
  $("quit-app").addEventListener("click", () => action($("quit-app"), "quit_app"));
  $("cancel-quick").addEventListener("click", () =>
    action($("cancel-quick"), "cancel_dictation")
  );

  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      call("hide_quick").catch((error) => feedback(String(error), true));
    }
  });

  window.addEventListener("pagehide", () => {
    if (timerIntervalId !== null) clearInterval(timerIntervalId);
    if (waveRafId !== null) cancelAnimationFrame(waveRafId);
  });

  (async () => {
    await window.__TAURI__?.event.listen("dictation-state", (event) => {
      stateVersion += 1;
      state(event.payload);
    });
    await window.__TAURI__?.event.listen("dictation-level", (event) => {
      const level = Number(event.payload);
      targetAudioLevel = Number.isFinite(level) ? Math.min(1, Math.max(0, level)) : 0;
      if (!document.hidden && (latestSnapshot.recording || targetAudioLevel > 0)) {
        ensureWaveformRunning();
      }
    });
    document.addEventListener("visibilitychange", syncQuickLiveVisuals);
    await window.__TAURI__?.event.listen("quick-opened", () => {
      feedback("");
      refresh();
      $("quick-panel")?.focus();
    });
    await refresh();
    $("quick-panel")?.focus();
  })().catch((error) => feedback(String(error), true));
})();
