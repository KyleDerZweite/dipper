(function (root) {
  "use strict";

  const SAMPLE_RATE = 48000;

  const MODEL_PROFILES = [
    {
      id: "large_v3_turbo_q8_0",
      label: "Whisper large-v3-turbo Q8_0",
      shortLabel: "Turbo Q8_0",
      badge: "Recommended · Efficient",
      metrics: "~874 MB · ~2.2 s med · ~156 MB RAM",
      description:
        "Near-identical accuracy to F16 on German, English & code-switching with almost half the disk footprint and faster cold load.",
      defaultFilename: "ggml-large-v3-turbo-q8_0.bin",
      recommended: true,
    },
    {
      id: "large_v3_turbo_q5_0",
      label: "Whisper large-v3-turbo Q5_0",
      shortLabel: "Turbo Q5_0",
      badge: "Compact",
      metrics: "~574 MB · ~2.0 s med · ~135 MB RAM",
      description:
        "Smallest Whisper Turbo footprint when disk or VRAM headroom is tight.",
      defaultFilename: "ggml-large-v3-turbo-q5_0.bin",
      recommended: false,
    },
    {
      id: "large_v3_turbo",
      label: "Whisper large-v3-turbo",
      shortLabel: "Turbo F16",
      badge: "Full Precision F16",
      metrics: "~1.62 GB · ~2.7 s med · ~221 MB RAM",
      description: "Unquantized reference Turbo weights.",
      defaultFilename: "ggml-large-v3-turbo.bin",
      recommended: false,
    },
    {
      id: "large_v3",
      label: "Whisper large-v3",
      shortLabel: "Large v3",
      badge: "Legacy Baseline",
      metrics: "~3.10 GB · 32 decoder layers",
      description: "Original unpruned large-v3 architecture.",
      defaultFilename: "ggml-large-v3.bin",
      recommended: false,
    },
  ];

  const FAILURE_CATEGORIES = {
    queue_overflow: {
      label: "Capture queue overflow",
      tone: "warning",
      fallbackHint:
        "Disk writes fell behind the capture ring buffer. Any preserved audio can be transcribed manually in Journal.",
    },
    device_lost: {
      label: "Microphone disconnected",
      tone: "danger",
      fallbackHint:
        "The selected microphone disconnected during capture. Retained audio can still be transcribed.",
    },
    discontinuity: {
      label: "Audio stream gap",
      tone: "warning",
      fallbackHint:
        "PipeWire reported a sample gap during capture. Listen to the preview or transcribe the retained audio.",
    },
    pipewire_error: {
      label: "PipeWire stream error",
      tone: "danger",
      fallbackHint:
        "PipeWire audio capture encountered a stream error. Check your microphone in Settings.",
    },
    invalid_format: {
      label: "Unsupported audio format",
      tone: "danger",
      fallbackHint:
        "PipeWire delivered an unsupported sample layout; Dipper expects 48 kHz mono float32.",
    },
    storage_error: {
      label: "Storage write issue",
      tone: "danger",
      fallbackHint:
        "Audio or Journal storage write failed during capture. Check available disk space; saved prefixes remain intact.",
    },
    session_lost: {
      label: "Desktop session interrupted",
      tone: "warning",
      fallbackHint:
        "Recording was interrupted before finalization. Any retained audio can be transcribed safely without insertion.",
    },
    canceled: {
      label: "Capture canceled",
      tone: "muted",
      fallbackHint:
        "Recording was canceled before completion. Retained audio remains available in Journal.",
    },
    too_short: {
      label: "Hold released quickly",
      tone: "warning",
      fallbackHint:
        "Hold key was released before audio arrived. Hold slightly longer and wait for the Recording indicator.",
    },
    no_audio_stream: {
      label: "No microphone signal",
      tone: "warning",
      fallbackHint:
        "PipeWire delivered no audio frames. Check whether your microphone is muted, disconnected, or suspended.",
    },
    partial_audio: {
      label: "Partial audio captured",
      tone: "warning",
      fallbackHint:
        "A valid prefix of capture audio was saved to disk and can be transcribed manually.",
    },
    abandoned: {
      label: "Capture closed early",
      tone: "warning",
      fallbackHint:
        "Capture session closed unexpectedly without a stop signal.",
    },
    unknown: {
      label: "Capture failed",
      tone: "danger",
      fallbackHint:
        "Capture ended unexpectedly. Check diagnostics or transcribe any retained audio.",
    },
  };

  function getModelProfileMeta(profileId) {
    return (
      MODEL_PROFILES.find((item) => item.id === profileId) || {
        id: profileId || "large_v3_turbo",
        label: profileId ? String(profileId) : "Whisper large-v3-turbo",
        shortLabel: profileId ? String(profileId) : "Turbo F16",
        badge: "Configured",
        metrics: "",
        description: "",
        defaultFilename: "ggml-large-v3-turbo.bin",
        recommended: false,
      }
    );
  }

  function computeElapsedMs(startedAtMs, nowMs = Date.now()) {
    const start = Number(startedAtMs);
    const now = Number(nowMs);
    if (!Number.isFinite(start) || !Number.isFinite(now) || start <= 0) {
      return 0;
    }
    return Math.max(0, now - start);
  }

  function formatElapsedTimer(ms) {
    const safeMs = Number.isFinite(Number(ms)) ? Math.max(0, Number(ms)) : 0;
    const totalSeconds = Math.floor(safeMs / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }

  function formatDuration(durationMs, frames = 0, sampleRate = SAMPLE_RATE) {
    const numericMs = Number(durationMs);
    const numericFrames = Number(frames);
    const rate = Number(sampleRate) > 0 ? Number(sampleRate) : SAMPLE_RATE;
    const effectiveMs =
      Number.isFinite(numericMs) && numericMs > 0
        ? numericMs
        : Number.isFinite(numericFrames) && numericFrames > 0
          ? (numericFrames * 1000) / rate
          : 0;
    if (effectiveMs <= 0) return "0 ms";
    if (effectiveMs < 1000) return `${Math.round(effectiveMs)} ms`;
    if (effectiveMs < 60000) return `${(effectiveMs / 1000).toFixed(1)} s`;
    const totalSeconds = Math.round(effectiveMs / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
  }

  function formatFrames(count) {
    const value = Number(count);
    if (!Number.isFinite(value) || value < 0) return "0";
    return new Intl.NumberFormat("en-US").format(Math.round(value));
  }

  function formatFrameSummary(row) {
    const available = Number(row?.available_frames) || 0;
    const accepted = Number(row?.accepted_frames) || 0;
    if (accepted > 0 && accepted !== available) {
      return `${formatFrames(available)} / ${formatFrames(accepted)} frames`;
    }
    return `${formatFrames(available)} frames`;
  }

  function formatBytes(bytes) {
    const value = Number(bytes);
    if (bytes === null || bytes === undefined || !Number.isFinite(value) || value < 0) {
      return null;
    }
    if (value < 1000) return `${Math.round(value)} B`;
    if (value < 1_000_000) return `${(value / 1000).toFixed(1)} KB`;
    if (value < 1_000_000_000) {
      const mb = value / 1_000_000;
      return `${mb >= 100 ? Math.round(mb) : mb.toFixed(1)} MB`;
    }
    return `${(value / 1_000_000_000).toFixed(2)} GB`;
  }

  function formatLatencyMs(ms) {
    if (ms === null || ms === undefined) return null;
    const value = Number(ms);
    if (!Number.isFinite(value) || value < 0) return null;
    if (value < 1000) return `${Math.round(value)} ms`;
    return `${(value / 1000).toFixed(2)} s`;
  }

  function formatTimestamp(startedAt) {
    if (startedAt === null || startedAt === undefined || startedAt === "") {
      return { iso: "", label: "Recording time unavailable", title: "" };
    }
    const asNumber = Number(startedAt);
    const date = Number.isFinite(asNumber) && asNumber > 0 ? new Date(asNumber) : new Date(String(startedAt));
    if (Number.isNaN(date.getTime())) {
      return { iso: "", label: "Recording time unavailable", title: "" };
    }
    return {
      iso: date.toISOString(),
      label: new Intl.DateTimeFormat("en-GB", {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(date),
      title: date.toLocaleString(),
    };
  }

  function hasNonEmptyText(value) {
    return typeof value === "string" && value.trim().length > 0;
  }

  function isRecoveredRecording(row) {
    return Boolean(
      row &&
        typeof row.audio_state === "string" &&
        row.audio_state !== "complete" &&
        hasNonEmptyText(row.raw_text)
    );
  }

  function needsRecovery(row) {
    if (!row) return false;
    if (isRecoveredRecording(row)) return false;
    if (hasNonEmptyText(row.raw_text)) return false;
    if (row.can_recover && row.has_audio) return true;
    if (row.audio_state === "failed" || row.audio_state === "interrupted") return true;
    if (row.recognition_state === "failed" || row.recognition_state === "interrupted") return true;
    return false;
  }

  function isCompletedRecording(row) {
    if (!row) return false;
    if (hasNonEmptyText(row.raw_text)) return true;
    return row.audio_state === "complete" && row.recognition_state === "complete";
  }

  function formatFailureCategory(category) {
    if (!category) {
      return {
        key: "unknown",
        label: "Needs review",
        tone: "warning",
        fallbackHint: "Capture did not finish normally. Check diagnostics for details.",
      };
    }
    const known = FAILURE_CATEGORIES[category];
    if (known) {
      return { key: category, ...known };
    }
    const clean = String(category).replace(/_/g, " ").trim();
    const label = clean ? clean.charAt(0).toUpperCase() + clean.slice(1) : "Capture issue";
    return {
      key: String(category),
      label,
      tone: "warning",
      fallbackHint: "Capture encountered an issue. Review diagnostics or retained audio.",
    };
  }

  function getFailurePresentation(row) {
    if (!row) return null;
    const recovered = isRecoveredRecording(row);
    const hasFailureState =
      row.audio_state === "failed" ||
      row.audio_state === "interrupted" ||
      row.audio_state === "canceled" ||
      row.recognition_state === "failed" ||
      row.recognition_state === "interrupted" ||
      Boolean(row.failure_category) ||
      Boolean(row.failure_hint);
    if (!hasFailureState || recovered) return null;

    const categoryMeta = formatFailureCategory(
      row.failure_category || (row.audio_state === "canceled" ? "canceled" : row.audio_state === "interrupted" ? "session_lost" : null)
    );
    const hint =
      (typeof row.failure_hint === "string" && row.failure_hint.trim()) ||
      (row.recognition_state === "failed" && typeof row.detail === "string" && row.detail.trim()) ||
      categoryMeta.fallbackHint;
    return {
      category: row.failure_category || categoryMeta.key,
      label: categoryMeta.label,
      tone: categoryMeta.tone,
      hint,
    };
  }

  function pillToneForAudio(state) {
    if (state === "complete") return "success";
    if (state === "failed") return "danger";
    if (state === "interrupted" || state === "canceled") return "warning";
    return "muted";
  }

  function pillToneForRecognition(state) {
    if (state === "complete") return "success";
    if (state === "failed") return "danger";
    if (state === "interrupted" || state === "canceled" || state === "no_text" || state === "preparing" || state === "running") {
      return "warning";
    }
    return "muted";
  }

  function pillToneForInsertion(state) {
    if (state === "confirmed") return "success";
    if (state === "failed") return "danger";
    if (state === "pending" || state === "uncertain") return "warning";
    return "muted";
  }

  function getRecordingPills(row) {
    if (!row) return [];
    const pills = [];
    const audioState = row.audio_state || "unknown";
    pills.push({
      kind: "audio",
      text: `Audio ${audioState.replace(/_/g, " ")}`,
      tone: pillToneForAudio(audioState),
    });
    if (row.recognition_state) {
      pills.push({
        kind: "recognition",
        text: `Recognition ${row.recognition_state.replace(/_/g, " ")}`,
        tone: pillToneForRecognition(row.recognition_state),
      });
    }
    if (row.insertion_state) {
      pills.push({
        kind: "insertion",
        text: `Insertion ${row.insertion_state.replace(/_/g, " ")}`,
        tone: pillToneForInsertion(row.insertion_state),
      });
    }
    if (isRecoveredRecording(row)) {
      pills.push({
        kind: "recovered",
        text: "Recovered",
        tone: "recovered",
      });
    }
    return pills;
  }

  function matchesJournalQuery(row, normalizedQuery) {
    if (!normalizedQuery) return true;
    const categoryMeta = row.failure_category ? formatFailureCategory(row.failure_category) : null;
    const modelMeta = row.model_profile ? getModelProfileMeta(row.model_profile) : null;
    const haystack = [
      row.raw_text,
      row.detail,
      row.failure_hint,
      row.failure_category,
      categoryMeta?.label,
      row.audio_state,
      row.recognition_state,
      row.insertion_state,
      row.availability,
      row.input_device?.description,
      row.input_device?.name,
      row.model_profile,
      modelMeta?.label,
      modelMeta?.shortLabel,
      row.capture_diagnostics?.pipewire_error,
      ...(Array.isArray(row.attempts)
        ? row.attempts.flatMap((a) => [a.raw_text, a.error, a.model_profile, a.state])
        : []),
    ]
      .filter((part) => typeof part === "string" && part.length > 0)
      .join(" ")
      .toLowerCase();
    return haystack.includes(normalizedQuery);
  }

  function filterJournalRecordings(rows, { filter = "all", query = "" } = {}) {
    const list = Array.isArray(rows) ? rows : [];
    const normalizedQuery = String(query || "").trim().toLowerCase();
    return list.filter((row) => {
      if (filter === "recovery" && !needsRecovery(row)) return false;
      if (filter === "completed" && !isCompletedRecording(row)) return false;
      return matchesJournalQuery(row, normalizedQuery);
    });
  }

  function getRecoverActionState(row, { busy = false, pendingRecordingId = null } = {}) {
    const visible = Boolean(row && row.can_recover && row.has_audio);
    if (!visible) {
      return {
        visible: false,
        disabled: true,
        pending: false,
        label: "Recover & transcribe",
        title: "",
      };
    }
    const pendingHere = Boolean(pendingRecordingId && pendingRecordingId === row.id);
    const disabled = Boolean(busy || pendingRecordingId);
    const hasTranscript = hasNonEmptyText(row.raw_text);
    const label = pendingHere
      ? "Transcribing…"
      : hasTranscript
        ? "Retranscribe"
        : "Recover & transcribe";
    const title = busy
      ? "Wait until active dictation or recovery finishes."
      : pendingRecordingId
        ? "Another Journal transcription is already running."
        : "Transcribe retained audio in Journal without altering your clipboard.";
    return {
      visible: true,
      disabled,
      pending: pendingHere,
      label,
      title,
    };
  }

  function getLatestRecoveryPrompt(rows, snapshot) {
    const list = Array.isArray(rows) ? rows : [];
    const latest = list[0] || null;
    if (latest && !isRecoveredRecording(latest)) {
      const hasProblem =
        latest.audio_state !== "complete" ||
        latest.recognition_state === "failed" ||
        latest.recognition_state === "interrupted" ||
        latest.recognition_state === "no_text" ||
        Boolean(latest.failure_category);
      if (hasProblem && latest.can_recover && latest.has_audio) {
        const failure = getFailurePresentation(latest);
        return {
          kind: "recoverable",
          recording: latest,
          title: "Retained audio ready to recover",
          message:
            failure?.hint ||
            latest.detail ||
            "Capture ended early, but valid audio was preserved on disk.",
          actionLabel: "Inspect & recover in Journal",
          quickActionLabel: "Open in Journal",
        };
      }
      if (
        (latest.audio_state === "failed" || latest.audio_state === "interrupted") &&
        !hasNonEmptyText(latest.raw_text)
      ) {
        const failure = getFailurePresentation(latest);
        return {
          kind: "hint",
          recording: latest,
          title: failure?.label || "Last capture did not finish",
          message: failure?.hint || latest.detail || "Check Journal diagnostics for details.",
          actionLabel: "Inspect in Journal",
          quickActionLabel: "Open in Journal",
        };
      }
    }

    if (snapshot?.error) {
      const recoverable = list.find(
        (row) =>
          !isRecoveredRecording(row) &&
          row.can_recover &&
          row.has_audio &&
          (row.audio_state !== "complete" ||
            row.recognition_state === "failed" ||
            row.recognition_state === "interrupted")
      );
      if (recoverable) {
        const failure = getFailurePresentation(recoverable);
        return {
          kind: "recoverable",
          recording: recoverable,
          title: "Retained audio ready to recover",
          message:
            failure?.hint ||
            recoverable.detail ||
            "A recent recording preserved audio that can be transcribed in Journal.",
          actionLabel: "Inspect & recover in Journal",
          quickActionLabel: "Open in Journal",
        };
      }
    }

    return null;
  }

  function summarizeSetupStatus(setupStatus, settings, fallbackBinding) {
    if (!setupStatus) {
      return {
        items: [
          { id: "model", label: "Model", status: "pending", summary: "Checking…", detail: "Inspecting model file" },
          { id: "runtime", label: "Runtime", status: "pending", summary: "Checking…", detail: "Inspecting libwhisper.so" },
          { id: "shortcut", label: "Shortcut", status: "pending", summary: fallbackBinding || "Checking…", detail: "Checking desktop shortcut" },
        ],
        recovery: { id: "recovery", label: "Journal recovery", status: "pending", summary: "Checking…", detail: "Checking SQLite state" },
        ready: false,
        issue: null,
      };
    }

    const profileMeta = getModelProfileMeta(settings?.model_profile);
    const modelSize = formatBytes(setupStatus.model_size_bytes);
    const binding = setupStatus.effective_binding || fallbackBinding || null;

    const modelItem = setupStatus.model_exists && setupStatus.model_issue
      ? {
          id: "model",
          label: "Model",
          status: "error",
          summary: "Wrong file",
          detail: setupStatus.model_issue,
        }
      : setupStatus.model_exists
      ? {
          id: "model",
          label: "Model",
          status: "ok",
          summary: modelSize ? `${profileMeta.shortLabel} · ${modelSize}` : profileMeta.shortLabel,
          detail: `${profileMeta.label}${modelSize ? ` (${modelSize})` : ""}`,
        }
      : {
          id: "model",
          label: "Model",
          status: "error",
          summary: "Missing file",
          detail: "Model file not found at configured path",
        };

    const runtimeItem = setupStatus.runtime_exists
      ? {
          id: "runtime",
          label: "Runtime",
          status: "ok",
          summary: "libwhisper.so",
          detail: "Native Whisper runtime library found",
        }
      : {
          id: "runtime",
          label: "Runtime",
          status: "error",
          summary: "Missing libwhisper.so",
          detail: "libwhisper.so not found in runtime library directory",
        };

    const shortcutItem = !setupStatus.desktop_ready
      ? {
          id: "shortcut",
          label: "Shortcut",
          status: "error",
          summary: "Portal unavailable",
          detail: setupStatus.desktop_error || "Desktop shortcut adapter unavailable",
        }
      : binding
        ? {
            id: "shortcut",
            label: "Shortcut",
            status: "ok",
            summary: binding,
            detail: `Global Hold shortcut bound to ${binding}`,
          }
        : {
            id: "shortcut",
            label: "Shortcut",
            status: "warn",
            summary: "Unassigned",
            detail: "Configure Hold key in Settings",
          };

    const recoveryItem = setupStatus.recovery_unpersisted
      ? {
          id: "recovery",
          label: "Journal recovery",
          status: "warn",
          summary: "Needs reconcile",
          detail: "Unpersisted recovery state detected. Open Journal to reconcile storage.",
        }
      : {
          id: "recovery",
          label: "Journal recovery",
          status: "ok",
          summary: "Synced",
          detail: "All recordings and audio artifacts are reconciled",
        };

    return {
      items: [modelItem, runtimeItem, shortcutItem],
      recovery: recoveryItem,
      ready: Boolean(setupStatus.ready && binding),
      issue: setupStatus.issue || null,
    };
  }

  function createAudioUrlRegistry({ revokeObjectURL } = {}) {
    const urls = new Map();
    const revokeImpl =
      typeof revokeObjectURL === "function"
        ? revokeObjectURL
        : (url) => {
            if (typeof URL !== "undefined" && typeof URL.revokeObjectURL === "function") {
              URL.revokeObjectURL(url);
            }
          };

    return {
      register(id, url) {
        if (urls.has(id)) {
          const previous = urls.get(id);
          if (previous && previous !== url) {
            revokeImpl(previous);
          }
        }
        urls.set(id, url);
        return url;
      },
      get(id) {
        return urls.get(id) || null;
      },
      has(id) {
        return urls.has(id);
      },
      revoke(id) {
        if (!urls.has(id)) return false;
        const url = urls.get(id);
        urls.delete(id);
        if (url) revokeImpl(url);
        return true;
      },
      revokeAll() {
        for (const url of urls.values()) {
          if (url) revokeImpl(url);
        }
        urls.clear();
      },
      size() {
        return urls.size;
      },
    };
  }

  const api = {
    SAMPLE_RATE,
    MODEL_PROFILES,
    getModelProfileMeta,
    computeElapsedMs,
    formatElapsedTimer,
    formatDuration,
    formatFrames,
    formatFrameSummary,
    formatBytes,
    formatLatencyMs,
    formatTimestamp,
    isRecoveredRecording,
    needsRecovery,
    isCompletedRecording,
    formatFailureCategory,
    getFailurePresentation,
    getRecordingPills,
    filterJournalRecordings,
    getRecoverActionState,
    getLatestRecoveryPrompt,
    summarizeSetupStatus,
    createAudioUrlRegistry,
  };

  if (typeof module !== "undefined" && module.exports) {
    module.exports = api;
  }
  if (root && typeof root === "object") {
    root.DipperFormat = api;
  }
})(typeof window !== "undefined" ? window : globalThis);
