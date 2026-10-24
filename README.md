# djdrop

A Windows Tauri desktop app that downloads audio from YouTube, SoundCloud, and Spotify links. Spotify links supply track metadata; spotDL matches and downloads audio from public sources (not Spotify's audio stream). DRM-protected and unavailable tracks cannot be downloaded.

## Build and run

Install Node.js, Rust with the Windows MSVC toolchain and Visual Studio C++ build tools, Python 3.10–3.14, and Git. From PowerShell:

```powershell
npm ci
.\scripts\fetch-binaries.ps1
npm run tauri -- dev
```

The fetch script refreshes yt-dlp, ffmpeg + ffprobe, and Deno. It builds the latest spotDL release with current yt-dlp because spotDL's published Windows executable can bundle an older, broken YouTube extractor. Re-run the script when a provider changes behavior and before packaging (`npm run tauri -- build`). The Windows installer bundles these binaries; users do not need to install Deno or ffmpeg separately.

In the queue, manually queued items retain a Start button. Starting and downloads without a reported size show activity without an invented percentage; yt-dlp and direct-file transfers show measured percentages. The native circle shows the same active/idle state while the queue is hidden. After the file arrives, the queue shows analysis separately.

Run frontend checks with `npm test -- --run` and backend tests with `cd src-tauri; cargo test --lib`.
