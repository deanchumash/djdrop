use reqwest::Client;

pub async fn check_sc_for_gate(sc_url: &str) -> Option<String> {
    let client = build_client();
    let html = client
        .get(sc_url)
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let gate_url = extract_purchase_link(&html)?;
    resolve_gate_url(&client, &gate_url).await
}

// ── HTML parsing helpers ─────────────────────────────────────────────────────

/// Find the href on the purchaseLink button; returns it only if it's gate.sc or hypeddit.
fn extract_purchase_link(html: &str) -> Option<String> {
    let start = html.find("purchaseLink__container")?;
    // Scan forward up to 1500 chars for an <a href=
    let window = &html[start..html.len().min(start + 1500)];
    let href_pos = window.find("href=\"")?;
    let after = &window[href_pos + 6..];
    let end = after.find('"')?;
    let href = &after[..end];
    if href.contains("gate.sc") || href.contains("hypeddit.com") {
        Some(href.to_string())
    } else {
        None
    }
}

/// Extract value="" from a hidden input by name attribute.
fn extract_input_value(html: &str, name: &str) -> Option<String> {
    let needle = format!("name=\"{}\"", name);
    let pos = html.find(&needle)?;
    // Search the surrounding 400 chars for the input tag
    let lo = pos.saturating_sub(200);
    let hi = (pos + 400).min(html.len());
    let chunk = &html[lo..hi];
    // Find the <input tag boundary
    let tag_start = chunk.rfind('<').unwrap_or(0);
    let tag_end = chunk.find('>').unwrap_or(chunk.len());
    if tag_start >= tag_end { return None; }
    let tag = &chunk[tag_start..tag_end];
    for q in ['"', '\''] {
        let prefix = format!("value={}", q);
        if let Some(vs) = tag.find(&prefix) {
            let after = &tag[vs + prefix.len()..];
            if let Some(ve) = after.find(q) {
                return Some(after[..ve].to_string());
            }
        }
    }
    None
}

/// Extract content="" from a <meta name="X"> tag.
fn extract_meta_content(html: &str, meta_name: &str) -> Option<String> {
    let needle = format!("name=\"{}\"", meta_name);
    let pos = html.find(&needle)?;
    let window = &html[pos..(pos + 300).min(html.len())];
    let cs = window.find("content=\"")?;
    let after = &window[cs + 9..];
    let ce = after.find('"')?;
    Some(after[..ce].to_string())
}

/// True if the URL path ends with a known audio extension (before query string).
fn is_direct_audio_url(url: &str) -> bool {
    let path = url.split('?').next().unwrap_or(url).to_lowercase();
    [".mp3", ".wav", ".flac", ".aiff", ".aif", ".m4a", ".ogg", ".opus", ".aac"]
        .iter()
        .any(|ext| path.ends_with(ext))
}

/// Convert a Dropbox share URL to a direct-download URL.
fn convert_dropbox_url(url: &str) -> String {
    if url.contains("dl.dropboxusercontent.com") {
        return url.to_string();
    }
    // Change domain and ensure dl=1
    let url = url.replace("www.dropbox.com", "dl.dropboxusercontent.com");
    if url.contains("dl=0") {
        url.replace("dl=0", "dl=1")
    } else if url.contains('?') {
        format!("{}&dl=1", url)
    } else {
        format!("{}?dl=1", url)
    }
}

/// Extract Google Drive file ID and return a direct-download URL.
fn convert_gdrive_url(url: &str) -> Option<String> {
    // Handles /file/d/{id}/view and /open?id={id} patterns
    if let Some(s) = url.find("/file/d/") {
        let after = &url[s + 8..];
        let id_end = after.find('/').unwrap_or(after.len());
        let id = &after[..id_end];
        if !id.is_empty() {
            return Some(format!("https://drive.google.com/uc?export=download&id={}", id));
        }
    }
    if let Some(s) = url.find("id=") {
        let after = &url[s + 3..];
        let id_end = after.find('&').unwrap_or(after.len());
        let id = &after[..id_end];
        if !id.is_empty() {
            return Some(format!("https://drive.google.com/uc?export=download&id={}", id));
        }
    }
    None
}

// ── HTTP helpers ─────────────────────────────────────────────────────────────

fn build_client() -> Client {
    Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .cookie_store(true)
        .build()
        .unwrap_or_default()
}

/// Recursively resolves a gate URL to a direct-download URL, or None if unresolvable.
///
/// Must return a boxed future because the function is async and recursive.
fn resolve_gate_url<'a>(
    client: &'a Client,
    url: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send + 'a>> {
    Box::pin(async move {
        // gate.sc: decode the `url` query param and recurse
        if url.contains("gate.sc") {
            let parsed = reqwest::Url::parse(url).ok()?;
            let inner = parsed
                .query_pairs()
                .find(|(k, _)| k == "url")
                .map(|(_, v)| v.into_owned())?;
            return resolve_gate_url(client, &inner).await;
        }

        // hypeddit: try to bypass the gate
        if url.contains("hypeddit.com") {
            return bypass_hypeddit(client, url).await;
        }

        // Direct audio file
        if is_direct_audio_url(url) {
            return Some(url.to_string());
        }

        // Dropbox share link
        if url.contains("dropbox.com") {
            return Some(convert_dropbox_url(url));
        }

        // Google Drive
        if url.contains("drive.google.com") {
            return convert_gdrive_url(url);
        }

        // Unknown: try HEAD to see if server reports audio content-type
        if let Ok(resp) = client.head(url).send().await {
            let ct = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if ct.starts_with("audio/") || ct == "application/octet-stream" {
                return Some(url.to_string());
            }
        }

        None
    })
}

/// Try to get a direct download URL from a hypeddit gate page without social steps.
async fn bypass_hypeddit(client: &Client, url: &str) -> Option<String> {
    let resp = client.get(url).send().await.ok()?;
    let final_url = resp.url().to_string();
    let html = resp.text().await.ok()?;

    let csrf     = extract_meta_content(&html, "csrf-token")?;
    let gate_id  = extract_input_value(&html, "fan_gate_id")?;
    let wrndk    = extract_input_value(&html, "wrndk")?;
    let file_uid = extract_input_value(&html, "current_download_file_listner")?;
    let steps    = extract_input_value(&html, "nwSteps").unwrap_or_default();
    let gvt      = extract_input_value(&html, "gvt")
        .or_else(|| {
            // gvt is sometimes <input type="hidden" id="gvt" value="..."> not name=
            let pos = html.find("id=\"gvt\"")?;
            let chunk = &html[pos..(pos + 100).min(html.len())];
            let vs = chunk.find("value=\"")?;
            let after = &chunk[vs + 7..];
            let ve = after.find('"')?;
            Some(after[..ve].to_string())
        })?;

    let origin = {
        let parsed = reqwest::Url::parse(&final_url).ok()?;
        format!("{}://{}", parsed.scheme(), parsed.host_str()?)
    };

    // Announce our page visit (analytics tracking — needed to establish session state)
    let _ = client
        .post(format!("{}/gate/ge", origin))
        .header("X-CSRF-TOKEN", &csrf)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Referer", &final_url)
        .form(&[("vt", &gvt), ("uid", &file_uid), ("_token", &csrf)])
        .send()
        .await;

    // Build POST params: include skip_gate_steps[] for every step
    let mut params: Vec<(String, String)> = vec![
        ("file".into(),             file_uid.clone()),
        ("download_visit".into(),   "true".into()),
        ("profile_downloads".into(),"true".into()),
        ("page".into(),             "nonsingle".into()),
        ("download_action".into(),  "DOWNLOAD".into()),
        ("is_skippable".into(),     "1".into()),
        ("steps".into(),            steps.clone()),
        ("email".into(),            String::new()),
        ("is_mobile".into(),        String::new()),
        ("fan_gate_id".into(),      gate_id),
        ("wrndk".into(),            wrndk),
        ("gvf".into(),              "0".into()),
        ("_token".into(),           csrf.clone()),
    ];
    for step in steps.split(',').filter(|s| !s.is_empty()) {
        params.push(("skip_gate_steps[]".into(), step.into()));
    }

    let resp = client
        .post(format!("{}/gate/download/ul", origin))
        .header("X-CSRF-TOKEN", &csrf)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Referer", &final_url)
        .form(&params)
        .send()
        .await
        .ok()?;

    let json: serde_json::Value = resp.json().await.ok()?;

    if json["download_status"].as_bool().unwrap_or(false) {
        let dl_url = json["URL"].as_str().unwrap_or("").to_string();
        if !dl_url.is_empty() {
            // The URL is a hypeddit redirect that chains to S3 — resolve further
            return resolve_gate_url(client, &dl_url).await;
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_purchase_link_gate_sc() {
        let html = r#"<div class="purchaseLink__container"><a href="https://gate.sc?url=https%3A%2F%2Fhypeddit.com%2Ftest&amp;token=abc" class="sc-button">FREE DL</a></div>"#;
        let result = extract_purchase_link(html);
        assert!(result.is_some());
        assert!(result.unwrap().contains("gate.sc"));
    }

    #[test]
    fn extract_purchase_link_hypeddit_direct() {
        let html = r#"<div class="purchaseLink__container"><a href="https://hypeddit.com/artist/track">DL</a></div>"#;
        let result = extract_purchase_link(html);
        assert_eq!(result, Some("https://hypeddit.com/artist/track".to_string()));
    }

    #[test]
    fn extract_purchase_link_none_for_buy_link() {
        let html = r#"<div class="purchaseLink__container"><a href="https://beatport.com/track/x">Buy</a></div>"#;
        assert!(extract_purchase_link(html).is_none());
    }

    #[test]
    fn extract_purchase_link_none_when_absent() {
        assert!(extract_purchase_link("<html><body>no links</body></html>").is_none());
    }

    #[test]
    fn extract_input_value_double_quote() {
        let html = r#"<input type="hidden" name="fan_gate_id" id="fan_gate_id" value="2205464" />"#;
        assert_eq!(extract_input_value(html, "fan_gate_id"), Some("2205464".to_string()));
    }

    #[test]
    fn extract_input_value_closing_tag_before_input_returns_none() {
        // Real-world HTML: closing tags in the 200-char lookback window cause tag_start > tag_end.
        // Must return None, not panic.
        // Closing tags before <input invert tag_start/tag_end — guard returns None, no panic.
        let html = r#"</div></label><input type="hidden" name="fan_gate_id" value="999" />"#;
        assert!(extract_input_value(html, "fan_gate_id").is_none());
        // Pathological: name appears inside a closing tag context where no opening < precedes >
        let html2 = r#"</span> name="fan_gate_id" value="bad">"#;
        assert!(extract_input_value(html2, "fan_gate_id").is_none());
    }

    #[test]
    fn extract_input_value_single_quote() {
        let html = r#"<input type="hidden" name="fan_gate_id" value='99999' />"#;
        assert_eq!(extract_input_value(html, "fan_gate_id"), Some("99999".to_string()));
    }

    #[test]
    fn extract_meta_content_csrf() {
        let html = r#"<meta name="csrf-token" content="abc123XYZ">"#;
        assert_eq!(extract_meta_content(html, "csrf-token"), Some("abc123XYZ".to_string()));
    }

    #[test]
    fn is_direct_audio_url_mp3() {
        assert!(is_direct_audio_url("https://example.com/song.mp3"));
        assert!(is_direct_audio_url("https://example.com/song.MP3?v=1"));
        assert!(!is_direct_audio_url("https://hypeddit.com/artist/track"));
    }

    #[test]
    fn convert_dropbox_dl0_to_dl1() {
        let result = convert_dropbox_url("https://www.dropbox.com/s/abc/song.mp3?dl=0");
        assert!(result.contains("dl.dropboxusercontent.com"));
        assert!(result.contains("dl=1"));
        assert!(!result.contains("dl=0"));
    }

    #[test]
    fn convert_dropbox_already_direct() {
        let url = "https://dl.dropboxusercontent.com/s/abc/song.mp3";
        assert_eq!(convert_dropbox_url(url), url);
    }

    #[test]
    fn convert_gdrive_file_view() {
        let url = "https://drive.google.com/file/d/1aBcDeFgHiJ/view";
        let result = convert_gdrive_url(url);
        assert_eq!(result, Some("https://drive.google.com/uc?export=download&id=1aBcDeFgHiJ".to_string()));
    }

    #[test]
    fn convert_gdrive_open_id() {
        let url = "https://drive.google.com/open?id=1aBcDeFgHiJ";
        let result = convert_gdrive_url(url);
        assert_eq!(result, Some("https://drive.google.com/uc?export=download&id=1aBcDeFgHiJ".to_string()));
    }
}
