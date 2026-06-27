#[derive(Debug, Clone, PartialEq)]
pub enum Pool { BpmSupreme, ClubKillers, LiveDjService }

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadSource { YtDlp, Spotdl, QobuzDlp, Pool(Pool), Search(String) }

pub fn route(input: &str) -> DownloadSource {
    if let Ok(u) = url::Url::parse(input) {
        let host = u.host_str().unwrap_or("").to_lowercase();
        if host.contains("youtube.com") || host.contains("youtu.be") { return DownloadSource::YtDlp; }
        if host.contains("soundcloud.com") { return DownloadSource::YtDlp; }
        if host.contains("spotify.com") { return DownloadSource::Spotdl; }
        if host.contains("qobuz.com") { return DownloadSource::QobuzDlp; }
        if host.contains("bpmsupreme.com") { return DownloadSource::Pool(Pool::BpmSupreme); }
        if host.contains("clubkillers.com") { return DownloadSource::Pool(Pool::ClubKillers); }
        if host.contains("livedjservice.com") { return DownloadSource::Pool(Pool::LiveDjService); }
    }
    DownloadSource::Search(input.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn routes_youtube() { assert_eq!(route("https://www.youtube.com/watch?v=abc"), DownloadSource::YtDlp); }
    #[test] fn routes_youtu_be() { assert_eq!(route("https://youtu.be/abc"), DownloadSource::YtDlp); }
    #[test] fn routes_soundcloud() { assert_eq!(route("https://soundcloud.com/artist/track"), DownloadSource::YtDlp); }
    #[test] fn routes_spotify() { assert_eq!(route("https://open.spotify.com/track/abc"), DownloadSource::Spotdl); }
    #[test] fn routes_qobuz() { assert_eq!(route("https://www.qobuz.com/album/abc"), DownloadSource::QobuzDlp); }
    #[test] fn routes_bpmsupreme() { assert_eq!(route("https://app.bpmsupreme.com/track/123"), DownloadSource::Pool(Pool::BpmSupreme)); }
    #[test] fn routes_clubkillers() { assert_eq!(route("https://www.clubkillers.com/track/abc"), DownloadSource::Pool(Pool::ClubKillers)); }
    #[test] fn routes_livedjservice() { assert_eq!(route("https://www.livedjservice.com/track/abc"), DownloadSource::Pool(Pool::LiveDjService)); }
    #[test] fn routes_plain_text_to_search() { assert_eq!(route("Kendrick Lamar HUMBLE"), DownloadSource::Search("Kendrick Lamar HUMBLE".into())); }
    #[test] fn routes_unknown_url_to_search() { assert_eq!(route("https://example.com/audio"), DownloadSource::Search("https://example.com/audio".into())); }
}
