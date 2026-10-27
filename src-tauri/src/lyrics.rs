//! Synced lyrics from LRCLIB (https://lrclib.net). Free, no API key.
//! Ok(None) = the song isn't there; Err = service unreachable / out of time.
//! The whole lookup is capped at ~12 seconds.
use serde_json::Value;
use std::time::{Duration, Instant};

const BUDGET: Duration = Duration::from_secs(12);

fn get_json(agent: &ureq::Agent, url: &str, q: &[(&str, &str)]) -> Result<Option<Value>, String> {
    let mut req = agent.get(url);
    for (k, v) in q {
        req = req.query(k, v);
    }
    match req.call() {
        Ok(r) => r.into_json::<Value>().map(Some).map_err(|e| e.to_string()),
        Err(ureq::Error::Status(_, _)) => Ok(None), // reached the server, no result
        Err(e) => Err(e.to_string()),
    }
}

fn synced(v: &Value) -> Option<String> {
    v["syncedLyrics"].as_str().filter(|s| !s.is_empty()).map(String::from)
}

/// "Song (Official Video) [HD] - Remastered 2011" -> "Song"
fn clean_title(t: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in t.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split(" - ").next().unwrap_or("").trim().to_string()
}

pub fn fetch(artist: &str, title: &str, album: &str, duration_s: u32) -> Result<Option<String>, String> {
    let started = Instant::now();
    let artist = artist.trim().trim_end_matches(" - Topic").to_string();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(4))
        .timeout(Duration::from_secs(6))
        .user_agent("Tunepeek/0.1 (personal open-source project)")
        .build();
    let mut err: Option<String> = None;
    let mut reached = false;
    let mut out_of_time = || started.elapsed() > BUDGET;

    // 1) exact match from LRCLIB's own database (fast; never goes to slow external sources)
    if duration_s > 0 && !out_of_time() {
        let dur = duration_s.to_string();
        match get_json(
            &agent,
            "https://lrclib.net/api/get-cached",
            &[("artist_name", artist.as_str()), ("track_name", title), ("album_name", album), ("duration", dur.as_str())],
        ) {
            Ok(Some(v)) => {
                reached = true;
                if let Some(s) = synced(&v) {
                    return Ok(Some(s));
                }
            }
            Ok(None) => reached = true,
            Err(e) => err = Some(e),
        }
    }

    // 2) fuzzy search: title as-is, then with junk like "(Official Video)" stripped
    let mut titles = vec![title.to_string()];
    let cleaned = clean_title(title);
    if !cleaned.is_empty() && cleaned != title {
        titles.push(cleaned);
    }
    for t in titles {
        if out_of_time() {
            break;
        }
        match get_json(&agent, "https://lrclib.net/api/search", &[("track_name", t.as_str()), ("artist_name", artist.as_str())]) {
            Ok(Some(v)) => {
                reached = true;
                if let Some(s) = v.as_array().and_then(|a| a.iter().find_map(synced)) {
                    return Ok(Some(s));
                }
            }
            Ok(None) => reached = true,
            Err(e) => err = Some(e),
        }
    }

    match (reached, err) {
        (false, Some(e)) => Err(e),
        (false, None) => Err("timed out".into()),
        _ => Ok(None),
    }
}
