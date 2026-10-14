/* App-owned chrome calls the host window API; recording stays in its owner. */
(() => {
  const appWindow = window.__TAURI__?.window.getCurrentWindow();
  const controls = ["window-minimize", "window-maximize", "window-close"];
  const report = (error) => window.dispatchEvent(new CustomEvent("dipper-window-error", {
    detail: `Window action failed: ${String(error)}. Try again.`,
  }));
  if (!appWindow) {
    for (const id of controls) document.getElementById(id).disabled = true;
    return;
  }
  let refreshing = false;
  let dirty = false;
  async function refresh() {
    dirty = true;
    if (refreshing) return;
    refreshing = true;
    try {
      do {
        dirty = false;
        try {
          const maximized = await appWindow.isMaximized();
          // A newer host event invalidates this reply. Query again before
          // changing the label or resize edges, including events during IPC.
          if (dirty) continue;
          document.documentElement.classList.toggle("window-maximized", maximized);
          const button = document.getElementById("window-maximize");
          const label = maximized ? "Restore window" : "Maximize";
          button.setAttribute("aria-label", label);
          button.title = label;
          button.querySelector("use").setAttribute("href", maximized ? "icons.svg#icon-restore" : "icons.svg#icon-maximize");
        } catch (error) { if (!dirty) report(error); }
      } while (dirty);
    } finally { refreshing = false; }
  }
  const actions = {
    "window-minimize": () => appWindow.minimize(),
    "window-maximize": async () => { await appWindow.toggleMaximize(); refresh(); },
    // CloseRequested is intercepted by the desktop host and hides to the tray.
    "window-close": () => appWindow.close(),
  };
  for (const [id, action] of Object.entries(actions)) {
    const button = document.getElementById(id);
    button.addEventListener("click", async () => {
      if (button.disabled) return;
      button.disabled = true;
      try { await action(); } catch (error) { report(error); }
      finally { button.disabled = false; }
    });
  }
  for (const edge of document.querySelectorAll("[data-resize]")) {
    edge.addEventListener("mousedown", (event) => {
      if (event.button !== 0 || document.documentElement.classList.contains("window-maximized")) return;
      event.preventDefault();
      appWindow.startResizeDragging(edge.dataset.resize).catch(report);
    });
  }
  appWindow.onResized(refresh).catch(report);
  refresh();
})();
