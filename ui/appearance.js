/* One appearance owner, applied before paint and shared by every local view. */
(() => {
  const modes = ["dark", "light", "system"];
  const preference = window.matchMedia("(prefers-color-scheme: dark)");
  let mode = "dark";
  try {
    const stored = localStorage.getItem("dipper.appearance");
    if (modes.includes(stored)) mode = stored;
  } catch { /* Keep a usable default when storage is unavailable. */ }
  function apply() {
    const effective = mode === "system" ? (preference.matches ? "dark" : "light") : mode;
    document.documentElement.dataset.theme = effective;
    document.documentElement.dataset.appearance = mode;
    document.documentElement.style.colorScheme = effective;
    const button = document.getElementById("appearance");
    if (!button) return;
    const next = modes[(modes.indexOf(mode) + 1) % modes.length];
    const label = `${mode[0].toUpperCase() + mode.slice(1)} appearance. Switch to ${next}.`;
    button.setAttribute("aria-label", label);
    button.title = label;
    button.querySelector("use").setAttribute("href", `#icon-${mode === "dark" ? "moon" : mode === "light" ? "sun" : "system"}`);
  }
  apply();
  preference.addEventListener("change", () => { if (mode === "system") apply(); });
  window.DipperAppearance = {
    mount() {
      apply();
      document.getElementById("appearance").addEventListener("click", () => {
        mode = modes[(modes.indexOf(mode) + 1) % modes.length];
        try { localStorage.setItem("dipper.appearance", mode); } catch { /* Current choice still works. */ }
        apply();
      });
    },
  };
})();
