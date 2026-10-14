/* One last-transcript presentation for the app and its tray quick panel. */
window.DipperTranscript = {
  mount(root, invoke, feedback, variant = "full") {
    const header = document.createElement('div'); header.className = 'last-header';
    const label = document.createElement('span'); label.textContent = 'Last dictation';
    const copy = document.createElement('button'); copy.type = 'button'; copy.className = 'secondary-button'; copy.textContent = 'Copy'; copy.disabled = true;
    header.append(label, copy);
    const text = document.createElement('p'); text.className = 'transcript-preview';
    const outcome = document.createElement('p'); outcome.className = 'last-outcome';
    root.append(header, text, outcome);
    text.textContent = "Loading transcript…";
    let recording = null;
    let copying = false;
    const synchronize = () => { copy.disabled = copying || !recording?.raw_text; };
    copy.addEventListener('click', async () => {
      if (copy.disabled) return;
      copying = true; synchronize();
      const id = recording.id;
      try { await invoke('copy_transcript', { recordingId: id }); feedback('Transcript copied.'); }
      catch (error) { feedback(String(error), true); }
      finally { copying = false; synchronize(); }
    });
    return {
      update(rows) {
        recording = rows.find((row) => row.raw_text) || rows[0] || null;
        text.textContent = recording?.raw_text || 'No transcript yet.';
        const state = recording?.state || 'Hold your key to make the first recording.';
        outcome.textContent = variant === 'quick' ? (state.split(' · ').find((part) => part.startsWith('Insertion')) || state.split(' · ').at(-1)) : state;
        outcome.title = state;
        synchronize();
      },
      unavailable(message) {
        recording = null;
        text.textContent = 'Transcript unavailable.';
        outcome.textContent = message;
        synchronize();
      },
    };
  },
};
