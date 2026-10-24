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

Spotify downloads try, in order: spotDL's own YouTube / YouTube Music / SoundCloud match; a verified YouTube match; Bandcamp. spotDL's match filter sometimes rejects the right upload (e.g. the artist's "Remedy (feat. …)" video), so djdrop looks up the track's title, artists and duration, searches YouTube itself, and accepts a result only if its title contains the track title, its title or channel names the lead artist, and its length is within 3 s of Spotify's. Anything else is treated as a different recording and refused. If all three fail, the queue shows each stage's reason.

Run frontend checks with `npm test -- --run` and backend tests with `cd src-tauri; cargo test --lib`.
