//! Synced lyrics from LRCLIB (https://lrclib.net). Free, no API key.
//! Returns Ok(None) when the song simply isn't there, Err when the service
//! couldn't be reached (so the UI can tell the difference).
use serde_json::Value;
use std::time::Duration;

fn get_json(agent: &ureq::Agent, url: &str, q: &[(&str, &str)]) -> Result<Option<Value>, String> {
    let mut last = String::new();
    for _ in 0..2 {
        // one retry on network trouble
        let mut req = agent.get(url);
        for (k, v) in q {
            req = req.query(k, v);
        }
        match req.call() {
            Ok(r) => return r.into_json::<Value>().map(Some).map_err(|e| e.to_string()),
            Err(ureq::Error::Status(_, _)) => return Ok(None), // 404 etc: reached the server, no result
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
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
    let artist = artist.trim().trim_end_matches(" - Topic").to_string();
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent("Tunepeek/0.1 (personal open-source project)")
        .build();
    let mut err: Option<String> = None;
    let mut reached = false;

    // 1) exact match (needs a duration to disambiguate versions)
    if duration_s > 0 {
        let dur = duration_s.to_string();
        match get_json(
            &agent,
            "https://lrclib.net/api/get",
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

    // 2) fuzzy search, with the title as-is and then with junk stripped
    let mut titles = vec![title.to_string()];
    let cleaned = clean_title(title);
    if !cleaned.is_empty() && cleaned != title {
        titles.push(cleaned);
    }
    for t in titles {
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
        _ => Ok(None),
    }
}
