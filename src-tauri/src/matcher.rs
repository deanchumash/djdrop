//! Duration/title/artist-verified YouTube matching for Spotify tracks that
//! spotDL's own filter rejects (e.g. an artist upload titled
//! "Remedy (feat. Jared Brady)" for the Spotify track "Remedy").

use serde_json::Value;

/// A Spotify track's title, artists and duration from its embed page.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackMeta {
    pub name: String,
    pub artists: Vec<String>,
    pub duration_secs: f64,
}

/// One YouTube search result from `yt-dlp --flat-playlist --dump-json`.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub title: String,
    pub channel: String,
    pub duration_secs: Option<f64>,
    pub url: String,
}

/// Allow a small difference between Spotify's millisecond duration and the upload's.
const DURATION_TOLERANCE_SECS: f64 = 3.0;

pub fn parse_candidates(output: &str) -> Vec<Candidate> {
    output.lines().filter_map(|line| {
        let v: Value = serde_json::from_str(line.trim()).ok()?;
        let text = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
        let url = text("url").filter(|u| u.starts_with("http"))
            .or_else(|| text("webpage_url"))
            .or_else(|| text("id").map(|id| format!("https://www.youtube.com/watch?v={id}")))?;
        Some(Candidate {
            title: text("title")?,
            channel: text("channel").or_else(|| text("uploader")).unwrap_or_default(),
            duration_secs: v.get("duration").and_then(Value::as_f64),
            url,
        })
    }).collect()
}

pub fn search_query(meta: &TrackMeta) -> String {
    format!("{} {}", meta.artists.join(" "), meta.name)
}

/// Lower-case alphanumerics separated by single spaces.
pub(crate) fn normalize(s: &str) -> String {
    let spaced: String = s.chars()
        .map(|c| if c.is_alphanumeric() { c.to_lowercase().next().unwrap_or(c) } else { ' ' })
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn contains_phrase(haystack: &str, phrase: &str) -> bool {
    !phrase.is_empty() && format!(" {haystack} ").contains(&format!(" {phrase} "))
}

/// Spotify titles may carry "(feat. X)" / "[Remix]" decorations that uploads omit.
pub(crate) fn core_title(name: &str) -> String {
    let mut depth = 0usize;
    let stripped: String = name.chars().filter(|&c| match c {
        '(' | '[' => { depth += 1; false }
        ')' | ']' => { depth = depth.saturating_sub(1); false }
        _ => depth == 0,
    }).collect();
    let core = normalize(&stripped);
    if core.is_empty() { normalize(name) } else { core }
}

/// The closest-duration candidate whose title contains the track title, whose
/// title or channel names the lead artist, and whose length matches Spotify's.
/// Anything else is a different recording and is never returned.
pub fn pick_candidate<'a>(meta: &TrackMeta, candidates: &'a [Candidate]) -> Option<&'a Candidate> {
    let title = core_title(&meta.name);
    let artist = normalize(meta.artists.first()?);
    candidates.iter()
        .filter_map(|c| {
            let diff = (c.duration_secs? - meta.duration_secs).abs();
            let described = normalize(&format!("{} {}", c.title, c.channel));
            let ok = diff <= DURATION_TOLERANCE_SECS
                && contains_phrase(&normalize(&c.title), &title)
                && contains_phrase(&described, &artist);
            ok.then_some((diff, c))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(title: &str, channel: &str, secs: f64) -> Candidate {
        Candidate {
            title: title.into(), channel: channel.into(),
            duration_secs: Some(secs), url: format!("https://www.youtube.com/watch?v={}", title.len()),
        }
    }
    fn remedy() -> TrackMeta {
        TrackMeta { name: "Remedy".into(), artists: vec!["Lizzy Ashliegh".into(), "Jared Brady".into()], duration_secs: 156.0 }
    }

    #[test]
    fn picks_the_artist_upload_spotdl_rejected_and_never_a_different_song() {
        let results = [
            cand("Remedy (feat. Jared Brady)", "Lizzy Ashliegh", 157.0),
            cand("Jared Brady \"Midnight\"", "ISINA", 97.0),
            cand("Sweet N Low The Movie (Visual Album)", "Lizzy Ashliegh", 2007.0),
            cand("Remedy", "Some Other Artist", 156.0),
        ];
        assert_eq!(pick_candidate(&remedy(), &results), Some(&results[0]));
        assert_eq!(pick_candidate(&remedy(), &results[1..]), None);
    }

    #[test]
    fn rejects_wrong_length_and_prefers_the_closest_duration() {
        let meta = remedy();
        assert_eq!(pick_candidate(&meta, &[cand("Lizzy Ashliegh - Remedy (Live)", "x", 190.0)]), None);
        let results = [
            cand("Lizzy Ashliegh - Remedy (slowed)", "x", 158.5),
            cand("Lizzy Ashliegh - Remedy", "x", 156.4),
        ];
        assert_eq!(pick_candidate(&meta, &results), Some(&results[1]));
        assert_eq!(pick_candidate(&meta, &[Candidate { duration_secs: None, ..cand("Lizzy Ashliegh - Remedy", "x", 0.0) }]), None);
    }

    #[test]
    fn title_must_match_whole_words_and_decorations_are_ignored() {
        let meta = TrackMeta { name: "Fun (feat. Kid)".into(), artists: vec!["Mercury".into()], duration_secs: 103.0 };
        assert!(pick_candidate(&meta, &[cand("Mercury - Fun", "Mercury", 102.5)]).is_some());
        assert!(pick_candidate(&meta, &[cand("Mercury - Funeral", "Mercury", 102.5)]).is_none());
    }

    #[test]
    fn parses_flat_search_output() {
        assert_eq!(search_query(&remedy()), "Lizzy Ashliegh Jared Brady Remedy");

        let out = "{\"id\":\"eupdJBFBuu0\",\"title\":\"Remedy (feat. Jared Brady)\",\"channel\":\"Lizzy Ashliegh\",\"duration\":157,\"url\":\"https://www.youtube.com/watch?v=eupdJBFBuu0\"}\nnot json\n{\"id\":\"abc\",\"title\":\"t\",\"uploader\":\"u\"}\n";
        let found = parse_candidates(out);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].url, "https://www.youtube.com/watch?v=eupdJBFBuu0");
        assert_eq!((found[1].url.as_str(), found[1].channel.as_str(), found[1].duration_secs), ("https://www.youtube.com/watch?v=abc", "u", None));
    }
}
