# Bot settings review

## Implemented

- Bot settings → Extensions and commands groups command switches, local TTS,
  the shared custom-command library and declarative extension packages.
- TTS settings are persisted per bot. Existing global settings remain read-only
  fallback values until a bot is configured; the old global PUT returns 410.
- Command switches are enforced by the chat dispatcher, including aliases and
  numeric search selections. They are not merely visual controls.
- Custom command definitions and package installation remain connection-scoped.
  The interface explicitly warns about this shared scope and fixes the library
  connection to the selected bot rather than the unrelated header selection.
- Player widget copy, bot console/source labels and selected media error copy
  use the active TR/EN language. User names, URLs, raw ffmpeg output and third-party
  content are not mechanically translated.
- Known yt-dlp failures return structured authentication/download errors without
  returning raw stderr, session data or signed URLs to the browser.
- Voice error diagnostics retain `failed_permid` when TeamSpeak supplies it.
  Error 2568 does not falsely disconnect an otherwise connected bot.
- The cookie screen reports file presence, not a claim that YouTube accepted
  the session. Cookie material must never be committed to this repository.

## Verification scope

Frontend/backend typechecks and production frontend build were run. Targeted
backend tests cover settings isolation, validation, admin restrictions, alias
enforcement, retained permission IDs and safe error presentation. Browser tests
cover per-bot saves, failed saves/retry, widget localization and existing Bot Hub
actions. These use deterministic fixtures; they do not prove real TTS audio,
YouTube playback or permission changes on a deployed TeamSpeak server.

## Still requires live verification

- YouTube can reject expired, guest-only or otherwise unusable cookie files.
  File presence alone is not authentication. This is not an ad-skipping problem.
- TeamSpeak 2568 needs the failed command/permission ID to determine the minimum
  permission change. Do not grant Server Admin or alter Guest globally as a
  generic workaround.
- Local source changes do not update an already running Manager container.
  Deployment, connection diagnostics and real audio must be checked separately.
- This is a scoped UX/architecture review, not a claim that every screen is fully
  translated or that the entire application has no vulnerabilities.
- Third-party manifests are declarative text-command packs, not arbitrary
  JavaScript/Python extensions. Full database/media backup is separate from a
  logical TeamSpeak configuration snapshot.
