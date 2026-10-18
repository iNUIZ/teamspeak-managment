# Alpha UI and installation verification

| Area | Controls and states | Evidence |
|---|---|---|
| Localization | TR → EN → TR on widgets, permissions and bot hub; reload persistence | Headings, form labels and screenshots |
| Clipboard | Privilege key and widget embed copy on HTTP; API rejection; modal focus preserved | Paste into an input and compare full value |
| Bot connection | Create, connect, stop, restart; auto voice port; manual NAT override | Client/channel IDs and normal-client list entry |
| Bot failures | Unreachable port and refused handshake | Error message rather than a false Idle state |
| TTS | Disabled engine, TR/EN, limits, cooldown and permissions | Unit tests and Linux voice smoke tests |
| Installation | Server/Manager order, second stack, admin preservation, access summary | Bash and SQLite tests |
| Branding | Original mark/favicon, accent and narrow layout | Desktop and mobile screenshots |

Exploration includes copy in a focus-trapped dialog on insecure HTTP, clipboard denial,
non-default TS6 voice ports, and empty/oversized/shell-like TTS messages.

UI fixtures do not prove real TS connectivity. Record the live versions and architectures
actually exercised; never describe an untested target as verified.

## Executed locally (2026-10-18)

- 30 Playwright tests: widget/permissions locale, clipboard fallback and denial,
  bot hub controls and permission-editor regressions passed.
- 13 backend tests: SQLite bootstrap, idempotent sync, port discovery, TTS input
  validation and socket cleanup/reconnect passed.
- Frontend/backend typechecks and production frontend build passed.
- Desktop 1440×900 and mobile 390×844 reviewed: no horizontal viewport overflow;
  widget dialog, original brand and settings controls remain legible.
- Interactive TTS UI fixture: enable, change voice, save, disable and restore;
  TR/EN labels checked. This does not prove real synthesized audio.
- Live existing TS6 bot verified connected on its actual voice port after correction.
- Optional engine WAV generation and container scans run separately in CI on
  amd64 and ARM64. Check the final commit's CI results before release.
