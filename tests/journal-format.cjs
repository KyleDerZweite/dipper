const { test } = require("node:test");
const assert = require("node:assert/strict");
const F = require("../ui/journal_format.js");

test("MODEL_PROFILES defines the 4 exact Whisper profiles with Recommended Q8_0 first", () => {
  assert.deepEqual(
    F.MODEL_PROFILES.map((p) => p.id),
    ["large_v3_turbo_q8_0", "large_v3_turbo_q5_0", "large_v3_turbo", "large_v3"]
  );
  assert.equal(F.MODEL_PROFILES[0].badge, "Recommended · Efficient");
  assert.equal(F.MODEL_PROFILES[0].metrics, "~874 MB · ~2.2 s med · ~156 MB RAM");
  assert.equal(F.MODEL_PROFILES[1].badge, "Compact");
  assert.equal(F.MODEL_PROFILES[2].badge, "Full Precision F16");
  assert.equal(F.MODEL_PROFILES[3].badge, "Legacy Baseline");
});

test("duration, frame, byte, latency, and elapsed timer formatting", () => {
  assert.equal(F.formatElapsedTimer(0), "00:00");
  assert.equal(F.formatElapsedTimer(5400), "00:05");
  assert.equal(F.formatElapsedTimer(65000), "01:05");
  assert.equal(F.formatElapsedTimer(-200), "00:00");
  assert.equal(F.computeElapsedMs(1000, 3500), 2500);
  assert.equal(F.computeElapsedMs(null, 3500), 0);

  assert.equal(F.formatDuration(450, 0), "450 ms");
  assert.equal(F.formatDuration(2450, 0), "2.5 s");
  assert.equal(F.formatDuration(65000, 0), "1m 05s");
  assert.equal(F.formatDuration(0, 48000), "1.0 s");

  assert.equal(F.formatFrames(96000), "96,000");
  assert.equal(
    F.formatFrameSummary({ available_frames: 48000, accepted_frames: 48000 }),
    "48,000 frames"
  );
  assert.equal(
    F.formatFrameSummary({ available_frames: 40000, accepted_frames: 48000 }),
    "40,000 / 48,000 frames"
  );

  assert.equal(F.formatBytes(null), null);
  assert.equal(F.formatBytes(874188075), "874 MB");
  assert.equal(F.formatBytes(1624555275), "1.62 GB");
  assert.equal(F.formatLatencyMs(340), "340 ms");
  assert.equal(F.formatLatencyMs(2240), "2.24 s");
  assert.equal(F.formatLatencyMs(null), null);
});

test("Recovered predicate and status pills reflect audio_state !== complete + non-empty raw_text", () => {
  const recoveredRow = {
    id: "r1",
    audio_state: "failed",
    recognition_state: "complete",
    insertion_state: null,
    raw_text: "Recovered words from partial capture.",
    failure_category: "partial_audio",
    failure_hint: "Partial prefix written to disk.",
  };
  assert.equal(F.isRecoveredRecording(recoveredRow), true);
  assert.equal(F.needsRecovery(recoveredRow), false);
  assert.equal(F.isCompletedRecording(recoveredRow), true);
  assert.equal(F.getFailurePresentation(recoveredRow), null);

  const pills = F.getRecordingPills(recoveredRow);
  assert.deepEqual(
    pills.map((p) => p.text),
    ["Audio failed", "Recognition complete", "Recovered"]
  );
  assert.equal(pills.at(-1).tone, "recovered");

  const normalComplete = {
    id: "r2",
    audio_state: "complete",
    recognition_state: "complete",
    insertion_state: "confirmed",
    raw_text: "Normal dictation.",
  };
  assert.equal(F.isRecoveredRecording(normalComplete), false);
  assert.deepEqual(
    F.getRecordingPills(normalComplete).map((p) => p.text),
    ["Audio complete", "Recognition complete", "Insertion confirmed"]
  );
  assert.equal(F.getRecordingPills(normalComplete)[2].tone, "success");

  const failedWithoutText = {
    id: "r3",
    audio_state: "failed",
    recognition_state: null,
    insertion_state: null,
    raw_text: "   ",
    can_recover: true,
    has_audio: true,
  };
  assert.equal(F.isRecoveredRecording(failedWithoutText), false);
  assert.equal(F.needsRecovery(failedWithoutText), true);
});

test("Failure category presentation provides plain-English hints for capture errors", () => {
  const overflow = F.getFailurePresentation({
    audio_state: "failed",
    failure_category: "queue_overflow",
    failure_hint: "Disk writes fell behind (3.2 s preserved).",
  });
  assert.equal(overflow.label, "Capture queue overflow");
  assert.equal(overflow.tone, "warning");
  assert.equal(overflow.hint, "Disk writes fell behind (3.2 s preserved).");

  const custom = F.getFailurePresentation({
    audio_state: "failed",
    failure_category: "device_lost",
    failure_hint: null,
  });
  assert.equal(custom.label, "Microphone disconnected");
  assert.equal(custom.tone, "danger");
  assert.match(custom.hint, /microphone disconnected/i);
});

test("Journal filter and live search match transcripts, errors, failure categories, and devices", () => {
  const rows = [
    {
      id: "1",
      audio_state: "complete",
      recognition_state: "complete",
      raw_text: "Deploy release notes to Fedora",
      input_device: { name: "alsa_input.usb", description: "Shure MV7 USB" },
      attempts: [],
    },
    {
      id: "2",
      audio_state: "failed",
      recognition_state: null,
      raw_text: null,
      can_recover: true,
      has_audio: true,
      failure_category: "queue_overflow",
      failure_hint: "Disk writes fell behind ring buffer.",
      input_device: { name: "alsa_input.pci", description: "Built-in Mic" },
      attempts: [{ raw_text: null, error: "pipewire timeout", model_profile: "large_v3_turbo_q8_0" }],
    },
    {
      id: "3",
      audio_state: "interrupted",
      recognition_state: "complete",
      raw_text: "Recovered meeting summary",
      can_recover: true,
      has_audio: true,
      input_device: { name: "alsa_input.bt", description: "Headset Mic" },
      attempts: [],
    },
  ];

  assert.equal(F.filterJournalRecordings(rows, { filter: "all" }).length, 3);
  assert.deepEqual(
    F.filterJournalRecordings(rows, { filter: "recovery" }).map((r) => r.id),
    ["2"]
  );
  assert.deepEqual(
    F.filterJournalRecordings(rows, { filter: "completed" }).map((r) => r.id),
    ["1", "3"]
  );
  assert.deepEqual(
    F.filterJournalRecordings(rows, { query: "shure" }).map((r) => r.id),
    ["1"]
  );
  assert.deepEqual(
    F.filterJournalRecordings(rows, { query: "overflow" }).map((r) => r.id),
    ["2"]
  );
  assert.deepEqual(
    F.filterJournalRecordings(rows, { query: "pipewire timeout" }).map((r) => r.id),
    ["2"]
  );
  assert.deepEqual(
    F.filterJournalRecordings(rows, { filter: "completed", query: "recovered" }).map((r) => r.id),
    ["3"]
  );
});

test("Recover & transcribe gating enforces single-worker busy rule", () => {
  const row = { id: "rec-1", can_recover: true, has_audio: true, raw_text: null };
  const idle = F.getRecoverActionState(row, { busy: false, pendingRecordingId: null });
  assert.equal(idle.visible, true);
  assert.equal(idle.disabled, false);
  assert.equal(idle.label, "Recover & transcribe");

  const whenBusy = F.getRecoverActionState(row, { busy: true, pendingRecordingId: null });
  assert.equal(whenBusy.visible, true);
  assert.equal(whenBusy.disabled, true);

  const whenPendingHere = F.getRecoverActionState(row, { busy: true, pendingRecordingId: "rec-1" });
  assert.equal(whenPendingHere.disabled, true);
  assert.equal(whenPendingHere.label, "Transcribing…");

  const whenPendingOther = F.getRecoverActionState(row, { busy: false, pendingRecordingId: "rec-2" });
  assert.equal(whenPendingOther.disabled, true);

  const noAudio = F.getRecoverActionState({ id: "rec-3", can_recover: false, has_audio: false }, {});
  assert.equal(noAudio.visible, false);
});

test("getLatestRecoveryPrompt flags latest recoverable capture or error until recovered", () => {
  const failedRow = {
    id: "f1",
    audio_state: "failed",
    can_recover: true,
    has_audio: true,
    raw_text: null,
    failure_category: "partial_audio",
    failure_hint: "1.8 s were safely written to disk.",
  };
  const prompt = F.getLatestRecoveryPrompt([failedRow], { error: true });
  assert.equal(prompt.kind, "recoverable");
  assert.equal(prompt.actionLabel, "Inspect & recover in Journal");

  const recoveredRow = { ...failedRow, raw_text: "Hello world" };
  assert.equal(F.getLatestRecoveryPrompt([recoveredRow], { error: false }), null);
});

test("summarizeSetupStatus formats model size, runtime, shortcut, and recovery states", () => {
  const summary = F.summarizeSetupStatus(
    {
      model_exists: true,
      model_size_bytes: 874188075,
      runtime_exists: true,
      desktop_ready: true,
      desktop_error: null,
      effective_binding: "F2",
      recovery_unpersisted: false,
      ready: true,
      issue: null,
    },
    { model_profile: "large_v3_turbo_q8_0" },
    null
  );
  assert.equal(summary.ready, true);
  assert.equal(summary.items[0].status, "ok");
  assert.equal(summary.items[0].summary, "Turbo Q8_0 · 874 MB");
  assert.equal(summary.items[1].status, "ok");
  assert.equal(summary.items[2].summary, "F2");
  assert.equal(summary.recovery.status, "ok");

  const unassigned = F.summarizeSetupStatus(
    {
      model_exists: true,
      model_size_bytes: 874188075,
      runtime_exists: true,
      desktop_ready: true,
      desktop_error: null,
      effective_binding: null,
      recovery_unpersisted: false,
      ready: true,
      issue: null,
    },
    { model_profile: "large_v3_turbo_q8_0" },
    null
  );
  assert.equal(unassigned.ready, false);
  assert.equal(unassigned.items[2].status, "warn");

  const mismatch = F.summarizeSetupStatus(
    {
      model_exists: true,
      model_size_bytes: 1624555275,
      model_issue: "The model file does not match Whisper large-v3-turbo Q8_0.",
      runtime_exists: true,
      desktop_ready: true,
      desktop_error: null,
      effective_binding: "F2",
      recovery_unpersisted: false,
      ready: false,
      issue: "The model file does not match Whisper large-v3-turbo Q8_0.",
    },
    { model_profile: "large_v3_turbo_q8_0" },
    null
  );
  assert.equal(mismatch.ready, false);
  assert.equal(mismatch.items[0].status, "error");
  assert.match(mismatch.items[0].detail, /does not match/);
});

test("AudioUrlRegistry tracks and revokes Blob URLs without leaks", () => {
  const revoked = [];
  const registry = F.createAudioUrlRegistry({
    revokeObjectURL: (url) => revoked.push(url),
  });
  registry.register("rec-1", "blob:1");
  registry.register("rec-1", "blob:2");
  assert.deepEqual(revoked, ["blob:1"]);
  assert.equal(registry.size(), 1);

  registry.register("rec-2", "blob:3");
  assert.equal(registry.revoke("rec-1"), true);
  assert.deepEqual(revoked, ["blob:1", "blob:2"]);

  registry.revokeAll();
  assert.deepEqual(revoked, ["blob:1", "blob:2", "blob:3"]);
  assert.equal(registry.size(), 0);
});

test("modelPathForProfile swaps standard filenames and keeps custom ones", () => {
  const dir = "/home/user/models/";
  assert.equal(
    F.modelPathForProfile(`${dir}ggml-large-v3-turbo.bin`, "large_v3_turbo_q8_0"),
    `${dir}ggml-large-v3-turbo-q8_0.bin`
  );
  assert.equal(
    F.modelPathForProfile(`${dir}ggml-large-v3-turbo-q8_0.bin`, "large_v3"),
    `${dir}ggml-large-v3.bin`
  );
  assert.equal(
    F.modelPathForProfile(`${dir}my-model.bin`, "large_v3_turbo_q8_0"),
    `${dir}my-model.bin`
  );
  assert.equal(F.modelPathForProfile("", "large_v3_turbo_q8_0"), "");
});
