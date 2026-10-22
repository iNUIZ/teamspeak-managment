# Cookie-only YouTube regression coverage

- No cookie: reject YouTube search/download before spawning a process or acquiring egress.
- Uploaded file: immediately active, no mode switch; missing, empty or unreadable files fail closed.
- Existing legacy mode: ignored; removed mode endpoint returns 410 and requires admin access.
- Existing cookies and unrelated Manager/TeamSpeak data are preserved across updates.
- No provider/plugin stage, supervisor process or provider environment in the new image recipe.
- yt-dlp ignores host configuration and plugin discovery; existing media-egress protection remains.
- Radio and non-YouTube media do not require account cookies.
- UI in TR/EN: upload, failed upload, deletion, automatic activation, no provider controls, mobile fit.

These regression tests do not prove that YouTube accepts any particular cookie/account/IP.
Actual audible playback must be tested with an authorized, current cookie file.

Visual inventory: review the YouTube cookie card in TR and EN at 1440×1000 and
390×844, before upload, after immediate activation, after failed replacement,
after removal and during paste/cancel. Check text wrapping, readable status and
accessible upload/remove controls. Exploratory cases: blank paste remains disabled;
failed replacement preserves the active cookie state. Never display real cookie contents.

## Verified — 2026-10-21

Backend/frontend TypeScript checks and production builds passed. 26 targeted backend
tests, 232 frontend unit tests and 13 targeted browser tests passed. Bash syntax,
automatic connection registration, PowerShell parsing and three allowlist tests passed.
Windows skips the shell-script subprocess fixture suite; this is not a native Linux
fixture-run claim. Live Docker verification confirmed the provider directory, compiled
module and autostart entry are absent, and anonymous YouTube requests are blocked.
Live TR/EN, paste/cancel and disabled empty save were inspected interactively, with
desktop/mobile screenshots and no horizontal overflow at 390px. The current live
installation has no cookie file, so no successful YouTube playback is claimed.
