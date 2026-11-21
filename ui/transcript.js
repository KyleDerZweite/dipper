/* Shared latest-transcript card for the main Dictation view and tray Quick Panel. */
window.DipperTranscript = {
  mount(root, invoke, feedback, variant = "full") {
    const header = document.createElement("div");
    header.className = "last-header";

    const titleGroup = document.createElement("div");
    titleGroup.className = "last-title-group";

    const label = document.createElement("span");
    label.className = "last-label";
    label.textContent = "Latest transcript";

    const recoveredBadge = document.createElement("span");
    recoveredBadge.className = "status-pill tone-recovered";
    recoveredBadge.textContent = "Recovered";
    recoveredBadge.hidden = true;

    titleGroup.append(label, recoveredBadge);

    const copy = document.createElement("button");
    copy.type = "button";
    copy.className = "secondary-button copy-button";
    copy.textContent = "Copy";
    copy.disabled = true;

    header.append(titleGroup, copy);

    const text = document.createElement("p");
    text.className = "transcript-preview";
    text.textContent = "Loading transcript…";

    const outcome = document.createElement("p");
    outcome.className = "last-outcome";

    root.append(header, text, outcome);

    let recording = null;
    let copying = false;

    const hasNonEmptyText = (row) =>
      typeof row?.raw_text === "string" && row.raw_text.trim().length > 0;

    const synchronize = () => {
      copy.disabled = copying || !hasNonEmptyText(recording);
    };

    copy.addEventListener("click", async () => {
      if (copy.disabled || !recording) return;
      copying = true;
      synchronize();
      const id = recording.id;
      try {
        await invoke("copy_transcript", { recordingId: id });
        feedback("Transcript copied.");
      } catch (error) {
        feedback(String(error), true);
      } finally {
        copying = false;
        synchronize();
      }
    });

    return {
      update(rows) {
        const list = Array.isArray(rows) ? rows : [];
        recording = list.find((row) => hasNonEmptyText(row)) || list[0] || null;
        const hasText = hasNonEmptyText(recording);
        text.textContent = hasText ? recording.raw_text : "No transcript yet.";
        text.className = hasText ? "transcript-preview" : "transcript-preview is-empty";

        const isRecovered = Boolean(
          recording &&
            typeof recording.audio_state === "string" &&
            recording.audio_state !== "complete" &&
            hasText
        );
        recoveredBadge.hidden = !isRecovered;

        const state = recording?.state || "Hold your key to make the first recording.";
        outcome.textContent =
          variant === "quick"
            ? state.split(" · ").find((part) => part.startsWith("Insertion")) ||
              state.split(" · ").at(-1)
            : state;
        outcome.title = state;
        synchronize();
      },
      unavailable(message) {
        recording = null;
        recoveredBadge.hidden = true;
        text.textContent = "Transcript unavailable.";
        text.className = "transcript-preview is-empty";
        outcome.textContent = message;
        outcome.title = message;
        synchronize();
      },
    };
  },
};
