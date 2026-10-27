const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);

let S = null;            // latest media state
let recvAt = 0;          // performance.now() when S arrived
let cfg = { auto_hide_secs: 5, lyrics: true, accent: true };
let seeking = false, volDrag = false, volTouched = 0, hovering = false;
let hideTimer = null, artLoaded = false, lyrics = null, lastKey = "";
let lyricShown = "", lyricTimer = null, lyricNote = "";

// Cross-fade between lyric lines (CSS handles the opacity transition).
function setLyric(text) {
  const el = $("lyric");
  if (text === lyricShown) return;
  lyricShown = text;
  clearTimeout(lyricTimer);
  if (!text) { el.textContent = ""; el.style.opacity = 1; return; }
  el.style.opacity = 0;
  lyricTimer = setTimeout(() => { el.textContent = text; el.style.opacity = 1; }, 200);
}

const fmt = (ms) => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

// ---------- auto-hide ----------
function arm() {
  clearTimeout(hideTimer);
  if (hovering || seeking || volDrag || !cfg.auto_hide_secs) return;
  hideTimer = setTimeout(() => invoke("hide_flyout"), cfg.auto_hide_secs * 1000);
}
document.addEventListener("mousemove", () => { hovering = true; clearTimeout(hideTimer); });
document.addEventListener("mouseleave", () => { hovering = false; arm(); });
document.addEventListener("pointerup", () => { seeking = false; volDrag = false; arm(); });

// ---------- accent colour from album art ----------
function accentFrom(img) {
  try {
    const c = document.createElement("canvas");
    c.width = c.height = 24;
    const x = c.getContext("2d", { willReadFrequently: true });
    x.drawImage(img, 0, 0, 24, 24);
    const d = x.getImageData(0, 0, 24, 24).data;
    let R = 0, G = 0, B = 0, W = 0;
    for (let i = 0; i < d.length; i += 4) {
      const r = d[i], g = d[i + 1], b = d[i + 2];
      const mx = Math.max(r, g, b), mn = Math.min(r, g, b);
      const sat = mx ? (mx - mn) / mx : 0;
      const w = sat * sat * (mx / 255) + 0.001;
      R += r * w; G += g * w; B += b * w; W += w;
    }
    R /= W; G /= W; B /= W;
    const m = Math.max(R, G, B) || 1;
    if (m < 150) { const k = 150 / m; R *= k; G *= k; B *= k; }
    const f = (v) => Math.min(255, Math.round(v));
    document.documentElement.style.setProperty("--accent", `${f(R)} ${f(G)} ${f(B)}`);
  } catch (_) {}
}
function setArt(url) {
  const img = $("art");
  if (!url) { img.removeAttribute("src"); return; }
  img.onload = () => { if (cfg.accent) accentFrom(img); };
  img.src = url;
}

// ---------- synced lyrics ----------
function parseLrc(text) {
  const out = [];
  for (const line of text.split("\n")) {
    const stamps = [...line.matchAll(/\[(\d+):(\d+(?:\.\d+)?)\]/g)];
    if (!stamps.length) continue;
    const t = line.replace(/\[[^\]]*\]/g, "").trim();
    for (const m of stamps) out.push({ t: (+m[1] * 60 + +m[2]) * 1000, text: t });
  }
  return out.sort((a, b) => a.t - b.t);
}
const lyricCache = new Map(); // "artist|title" -> raw LRC text ("" = none found)
let lyricFetchTimer = null;

function applyLyrics(raw, key) {
  lyrics = raw ? parseLrc(raw) : null;
  lyricNote = lyrics && lyrics.length ? "" : "No synced lyrics found";
  if (lyricNote) setTimeout(() => { if (lastKey === key) lyricNote = ""; }, 4000);
}
// Called on every track change. Waits briefly so players that update their
// metadata a few times in a row don't trigger a search for each change.
function scheduleLyrics(s) {
  clearTimeout(lyricFetchTimer);
  lyrics = null;
  lyricNote = "";
  setLyric("");
  if (!cfg.lyrics || !s.title) return;
  lyricNote = "Searching lyrics…";
  lyricFetchTimer = setTimeout(() => loadLyrics(s), 400);
}
async function loadLyrics(s) {
  const key = s.key;
  const ck = `${s.artist}|${s.title}`.toLowerCase();
  if (lyricCache.has(ck)) { applyLyrics(lyricCache.get(ck), key); return; }
  try {
    // Hard client-side backstop so "Searching…" can never hang forever.
    const raw = await Promise.race([
      invoke("fetch_lyrics", { artist: s.artist, title: s.title, album: s.album, durationS: Math.round(s.duration_ms / 1000) }),
      new Promise((_, rej) => setTimeout(() => rej(new Error("timeout")), 15000)),
    ]);
    if (lastKey !== key) return;
    lyricCache.set(ck, raw || "");
    applyLyrics(raw || "", key);
  } catch (e) {
    if (lastKey !== key) return;
    console.error("lyrics:", e);
    lyricNote = "Couldn't reach the lyrics service";
    setTimeout(() => { if (lastKey === key) lyricNote = ""; }, 4000);
  }
}
function lyricLine(pos) {
  if (!lyrics || !lyrics.length) return "";
  let lo = 0, hi = lyrics.length - 1, ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (lyrics[mid].t <= pos + 250) { ans = mid; lo = mid + 1; } else hi = mid - 1;
  }
  return ans < 0 ? "" : lyrics[ans].text || "♪";
}

// ---------- rendering ----------
function idle() {
  $("title").textContent = "Nothing playing";
  $("artist").textContent = "Start your selected player(s)";
  setArt(null);
  for (const id of ["prev", "next", "pp", "seek", "vol"]) $(id).disabled = true;
  $("icPlay").classList.remove("hidden"); $("icPause").classList.add("hidden");
  $("t0").textContent = $("t1").textContent = "0:00";
  setLyric("");
}
function onState(s) {
  recvAt = performance.now();
  if (!s) { S = null; lastKey = ""; idle(); return; }
  const changed = s.key !== lastKey;
  S = s;
  if (changed) {
    lastKey = s.key; artLoaded = false;
    setArt(null);
    scheduleLyrics(s);
  }
  if (s.has_art && !artLoaded) { artLoaded = true; invoke("get_art").then(setArt); }

  $("title").textContent = s.title || "Unknown title";
  $("artist").textContent = [s.artist, s.app_name].filter(Boolean).join(" · ");
  $("icPlay").classList.toggle("hidden", s.playing);
  $("icPause").classList.toggle("hidden", !s.playing);
  $("prev").disabled = !s.can_prev;
  $("next").disabled = !s.can_next;
  $("pp").disabled = false;
  $("seek").disabled = !s.can_seek;
  $("seek").max = Math.max(1, s.duration_ms);
  $("t1").textContent = fmt(s.duration_ms);
  const v = $("vol");
  v.disabled = s.volume == null;
  if (s.volume != null && !volDrag && Date.now() - volTouched > 1200) v.value = Math.round(s.volume * 100);
  $("lyric").classList.toggle("hidden", !cfg.lyrics);
}
setInterval(() => {
  if (!S || seeking) return;
  let pos = S.position_ms + (S.playing ? performance.now() - recvAt : 0);
  if (S.duration_ms > 0) pos = Math.min(pos, S.duration_ms);
  $("seek").value = pos;
  $("t0").textContent = fmt(pos);
  if (cfg.lyrics) setLyric(lyricLine(pos) || lyricNote);
}, 250);

// ---------- controls ----------
const id = () => S && S.app_id;
$("pp").onclick = () => { if (!S) return; invoke("play_pause", { id: id() }); S.playing = !S.playing; recvAt = performance.now();
  $("icPlay").classList.toggle("hidden", S.playing); $("icPause").classList.toggle("hidden", !S.playing); };
$("next").onclick = () => S && invoke("next", { id: id() });
$("prev").onclick = () => S && invoke("prev", { id: id() });

$("seek").addEventListener("pointerdown", () => { seeking = true; clearTimeout(hideTimer); });
$("seek").addEventListener("input", (e) => { $("t0").textContent = fmt(+e.target.value); });
$("seek").addEventListener("change", (e) => {
  if (!S) return;
  const ms = +e.target.value;
  invoke("seek", { id: id(), ms });
  S.position_ms = ms; recvAt = performance.now();
  seeking = false; arm();
});

let volSend = 0;
function sendVol(v) {
  volTouched = Date.now();
  const now = performance.now();
  if (now - volSend < 40) return; // light throttle
  volSend = now;
  if (S) invoke("set_volume", { id: id(), level: v / 100 });
}
$("vol").addEventListener("pointerdown", () => { volDrag = true; clearTimeout(hideTimer); });
$("vol").addEventListener("input", (e) => sendVol(+e.target.value));
$("vol").addEventListener("change", (e) => { volTouched = Date.now(); if (S) invoke("set_volume", { id: id(), level: +e.target.value / 100 }); });
document.addEventListener("wheel", (e) => {
  e.preventDefault();
  const v = $("vol");
  if (v.disabled) return;
  v.value = Math.max(0, Math.min(100, +v.value + (e.deltaY < 0 ? 5 : -5)));
  sendVol(+v.value);
  clearTimeout(hideTimer); arm();
}, { passive: false });
document.addEventListener("contextmenu", (e) => e.preventDefault());

// ---------- events from Rust ----------
async function loadCfg() {
  cfg = await invoke("get_settings");
  $("lyric").classList.toggle("hidden", !cfg.lyrics);
}
listen("flyout-shown", async () => {
  await loadCfg();
  onState(await invoke("get_state"));
  document.body.classList.add("show");
  hovering = false; arm();
});
listen("flyout-hiding", () => document.body.classList.remove("show"));
listen("media-state", (e) => onState(e.payload));
listen("settings-changed", loadCfg);
loadCfg();
idle();
