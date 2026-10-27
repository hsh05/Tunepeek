//! Reads and controls whatever the Windows media session (SMTC) exposes.
//! Spotify desktop and browsers playing YouTube Music both show up here.
use crate::audio;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as Props,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;

#[derive(Serialize, Clone, Default)]
pub struct MediaState {
    pub app_id: String,
    pub app_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub can_seek: bool,
    pub can_prev: bool,
    pub can_next: bool,
    pub has_art: bool,
    pub key: String,
    pub volume: Option<f32>,
}

#[derive(Default)]
pub struct Tracker {
    last_active: Option<String>,
    playing: HashSet<String>,
    art_key: String,
    art: Option<String>,
    art_tries: u8,
}

impl Tracker {
    pub fn art(&self) -> Option<String> {
        self.art.clone()
    }
}

fn friendly(id: &str) -> String {
    let l = id.to_lowercase();
    if l.contains("spotify") {
        "Spotify".into()
    } else if l.contains("brave") {
        "Brave".into()
    } else if l.contains("chrome") {
        "Chrome".into()
    } else if l.contains("msedge") {
        "Edge".into()
    } else if l.contains("firefox") {
        "Firefox".into()
    } else {
        id.trim_end_matches(".exe").to_string()
    }
}

fn read_thumb(props: &Props) -> Option<String> {
    let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.get().ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size > 4_000_000 {
        return None;
    }
    let mime = stream.ContentType().map(|s| s.to_string()).unwrap_or_default();
    let mime = if mime.is_empty() { "image/jpeg".to_string() } else { mime };
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size as u32).ok()?.get().ok()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;
    Some(format!("data:{};base64,{}", mime, STANDARD.encode(buf)))
}

/// Pick the session to show and describe it. Rule: the session that most
/// recently started playing wins; if nothing is playing, the last one that was.
pub fn snapshot(t: &mut Tracker, sources: &[String]) -> Option<MediaState> {
    audio::com_init();
    let mgr = Manager::RequestAsync().ok()?.get().ok()?;
    let mut list: Vec<(String, Session, bool)> = Vec::new();
    for s in mgr.GetSessions().ok()? {
        let Ok(h) = s.SourceAppUserModelId() else { continue };
        let id = h.to_string();
        let lid = id.to_lowercase();
        if !sources.is_empty() && !sources.iter().any(|x| lid.contains(x.as_str())) {
            continue;
        }
        let playing = s
            .GetPlaybackInfo()
            .ok()
            .and_then(|i| i.PlaybackStatus().ok())
            .map(|p| p == Status::Playing)
            .unwrap_or(false);
        list.push((id, s, playing));
    }

    let now_playing: HashSet<String> = list.iter().filter(|x| x.2).map(|x| x.0.clone()).collect();
    for id in &now_playing {
        if !t.playing.contains(id) {
            t.last_active = Some(id.clone());
        }
    }
    t.playing = now_playing;

    let last = t.last_active.clone();
    let chosen = list
        .iter()
        .find(|x| x.2 && Some(&x.0) == last.as_ref())
        .or_else(|| list.iter().find(|x| x.2))
        .or_else(|| list.iter().find(|x| Some(&x.0) == last.as_ref()))
        .or_else(|| list.first())?;
    let (id, s, playing) = (&chosen.0, &chosen.1, chosen.2);
    if playing {
        t.last_active = Some(id.clone());
    }

    let props = s.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let title = props.Title().map(|h| h.to_string()).unwrap_or_default();
    let artist = props.Artist().map(|h| h.to_string()).unwrap_or_default();
    let album = props.AlbumTitle().map(|h| h.to_string()).unwrap_or_default();
    let key = format!("{id}|{title}|{artist}|{album}");

    if t.art_key != key {
        t.art_key = key.clone();
        t.art = None;
        t.art_tries = 0;
    }
    if t.art.is_none() && t.art_tries < 8 {
        t.art_tries += 1;
        t.art = read_thumb(&props);
    }

    let (mut pos, mut dur) = (0i64, 0i64);
    if let Ok(tl) = s.GetTimelineProperties() {
        let start = tl.StartTime().map(|x| x.Duration).unwrap_or(0);
        let end = tl.EndTime().map(|x| x.Duration).unwrap_or(0);
        let p = tl.Position().map(|x| x.Duration).unwrap_or(0);
        let upd = tl.LastUpdatedTime().map(|x| x.UniversalTime).unwrap_or(0);
        dur = (end - start) / 10_000;
        pos = (p - start) / 10_000;
        if playing && upd > 0 {
            // Windows FILETIME ticks (100ns since 1601) for "now".
            let unix_ns = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            let now_ticks = (unix_ns / 100) as i64 + 116_444_736_000_000_000;
            pos += ((now_ticks - upd) / 10_000).max(0);
        }
        pos = if dur > 0 { pos.clamp(0, dur) } else { pos.max(0) };
    }

    let ctrl = s.GetPlaybackInfo().ok().and_then(|i| i.Controls().ok());
    let can_next = ctrl.as_ref().and_then(|c| c.IsNextEnabled().ok()).unwrap_or(true);
    let can_prev = ctrl.as_ref().and_then(|c| c.IsPreviousEnabled().ok()).unwrap_or(true);
    let can_seek = ctrl.as_ref().and_then(|c| c.IsPlaybackPositionEnabled().ok()).unwrap_or(false) && dur > 0;

    Some(MediaState {
        app_id: id.clone(),
        app_name: friendly(id),
        title,
        artist,
        album,
        playing,
        position_ms: pos,
        duration_ms: dur,
        can_seek,
        can_prev,
        can_next,
        has_art: t.art.is_some(),
        key,
        volume: audio::get_volume(id),
    })
}

fn with_session<R>(id: &str, f: impl FnOnce(&Session) -> Option<R>) -> Option<R> {
    audio::com_init();
    let mgr = Manager::RequestAsync().ok()?.get().ok()?;
    for s in mgr.GetSessions().ok()? {
        if let Ok(h) = s.SourceAppUserModelId() {
            if h.to_string() == id {
                return f(&s);
            }
        }
    }
    None
}

pub fn play_pause(id: &str) -> bool {
    with_session(id, |s| s.TryTogglePlayPauseAsync().ok()?.get().ok()).unwrap_or(false)
}
pub fn next(id: &str) -> bool {
    with_session(id, |s| s.TrySkipNextAsync().ok()?.get().ok()).unwrap_or(false)
}
pub fn prev(id: &str) -> bool {
    with_session(id, |s| s.TrySkipPreviousAsync().ok()?.get().ok()).unwrap_or(false)
}
pub fn seek(id: &str, ms: i64) -> bool {
    with_session(id, |s| s.TryChangePlaybackPositionAsync(ms.max(0) * 10_000).ok()?.get().ok()).unwrap_or(false)
}
