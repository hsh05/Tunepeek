const { invoke } = window.__TAURI__.core;
const $ = (id) => document.getElementById(id);
const say = (t, err) => { const s = $("status"); s.textContent = t; s.className = err ? "err" : ""; };

async function load() {
  const s = await invoke("get_settings");
  $("hotkey").value = s.hotkey;
  $("corner").value = s.corner;
  $("monitor").value = s.monitor;
  $("margin").value = s.margin;
  $("hide").value = s.auto_hide_secs;
  $("sources").value = s.sources;
  $("lyrics").checked = s.lyrics;
  $("accent").checked = s.accent;
  $("autostart").checked = s.autostart;
}

const hk = $("hotkey");
// While recording, release the current global hotkey so pressing it (even the
// same combination) reaches this page instead of toggling the flyout.
hk.addEventListener("focus", () => { hk.classList.add("rec"); say("Press your shortcut or a mouse side button…"); invoke("pause_hotkey"); });
hk.addEventListener("blur", () => { hk.classList.remove("rec"); say(""); invoke("resume_hotkey"); });

// Mouse side buttons: e.button 3 = back (Mouse4), 4 = forward (Mouse5).
const sideBtn = (e) => (e.button === 3 || e.button === 4);
hk.addEventListener("mousedown", (e) => {
  if (!sideBtn(e)) return;
  e.preventDefault();
  const mods = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  hk.value = [...mods, e.button === 3 ? "Mouse4" : "Mouse5"].join("+");
  say("");
  hk.blur();
});
for (const ev of ["mouseup", "auxclick"]) hk.addEventListener(ev, (e) => { if (sideBtn(e)) e.preventDefault(); });
hk.addEventListener("keydown", (e) => {
  e.preventDefault();
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;
  const mods = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  if (!mods.length) { say("Use at least one modifier (Ctrl, Alt, Shift).", true); return; }
  const key = e.code.startsWith("Key") ? e.code.slice(3) : e.code.startsWith("Digit") ? e.code.slice(5) : e.code;
  hk.value = [...mods, key].join("+");
  say("");
  hk.blur();
});

$("save").onclick = async () => {
  const s = {
    hotkey: hk.value,
    corner: $("corner").value,
    monitor: $("monitor").value,
    margin: Math.max(0, parseInt($("margin").value || "0", 10)),
    auto_hide_secs: Math.max(0, parseInt($("hide").value || "0", 10)),
    sources: $("sources").value,
    lyrics: $("lyrics").checked,
    accent: $("accent").checked,
    autostart: $("autostart").checked,
  };
  try { await invoke("save_settings", { s }); say("Saved ✓"); }
  catch (e) { say(String(e), true); await load(); }
};
$("preview").onclick = () => invoke("toggle_flyout");
load();
