# djdrop

A Windows Tauri desktop app that downloads audio from YouTube, SoundCloud, and Spotify links. Spotify links supply track metadata; spotDL searches YouTube, YouTube Music, and SoundCloud for public audio (not Spotify's audio stream). If the chosen source produces no file, djdrop retries Bandcamp in a separate job folder before reporting an error. Successful Spotify downloads move into the configured output directory. DRM-protected sources cannot be bypassed; a different publicly playable match is required.

## Build and run

Install Node.js, Rust with the Windows MSVC toolchain and Visual Studio C++ build tools, Python 3.10–3.14, and Git. From PowerShell:

```powershell
npm ci
.\scripts\fetch-binaries.ps1
npm run tauri -- dev
```

The fetch script refreshes yt-dlp, ffmpeg + ffprobe, and Deno. It builds the latest spotDL release with current yt-dlp because spotDL's published Windows executable can bundle an older, broken YouTube extractor. Re-run the script when a provider changes behavior and before packaging (`npm run tauri -- build`). The Windows installer bundles these binaries; users do not need to install Deno or ffmpeg separately.

In the queue, manually queued items retain a Start button. Starting and downloads without a reported size show activity without an invented percentage; yt-dlp and direct-file transfers show measured percentages. The native circle shows the same active/idle state while the queue is hidden. After the file arrives, the queue shows analysis separately. Finished items show the icon of the source the audio actually came from (for Spotify links, the YouTube/SoundCloud/Bandcamp URL spotDL recorded in the file's comment tag) and a file-type badge measured with ffprobe; MP3s also show their bitrate in kbps (e.g. `MP3 320`). Failed items show their download error directly below the track, without requiring a tooltip.

Spotify downloads try, in order: Soulseek (only when a Soulseek login is saved in Settings); spotDL's own YouTube / YouTube Music / SoundCloud match; a verified YouTube match; Bandcamp. While a source is being tried the queue shows "Checking Soulseek…", "Checking Bandcamp…" and so on, then switches to a percentage once bytes are arriving.

**Soulseek.** djdrop runs the bundled [slskd](https://github.com/slskd/slskd) headless, bound to `127.0.0.1` on random ports with its web UI off and its own authentication disabled (any program on your PC can reach that API while it runs), logs in with your Soulseek account, and shares nothing. Enter the login under *Settings → soulseek* (an `op://` reference, a literal value, or "Save to keychain"; Soulseek has no sign-up — the first login with an unused username creates the account). For a Spotify track it searches for `artist title`, keeps only FLAC or MP3 ≥256 kbps (320 CBR, or VBR ≥220) files whose path contains the title and lead artist, whose length is within 3 s of Spotify's, and whose file name isn't a remix/live/instrumental/etc. version the Spotify title doesn't claim; ranks FLAC > 320 > lower, then peers with a free upload slot and short queue; and tries the best three peers (each abandoned after 60 s queued or 60 s without progress). Files keep their native format (FLAC stays FLAC) and are renamed `Artists - Title.ext`. slskd stops after 10 idle minutes and when djdrop exits; its data lives in `%LOCALAPPDATA%\com.djdrop.app\slskd`. Peers may deprioritise or refuse users who share nothing, so some downloads will fall through to the other sources. Plain-text searches and non-track Spotify links (albums, playlists, artists) don't use Soulseek.

spotDL's match filter sometimes rejects the right upload (e.g. the artist's "Remedy (feat. …)" video), so for the YouTube step djdrop looks up the track's title, artists and duration, searches YouTube itself, and accepts a result only if its title contains the track title, its title or channel names the lead artist, and its length is within 3 s of Spotify's. Anything else is treated as a different recording and refused. If every source fails, the queue shows each stage's reason.

Run frontend checks with `npm test -- --run` and backend tests with `cd src-tauri; cargo test --lib`.
