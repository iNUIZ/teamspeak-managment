# TR panel regression inventory

Scope: authored interface copy, not user-entered names, media titles, identifiers, code samples or raw technical logs.

| Claim / control | Functional check | Visual state / evidence |
| --- | --- | --- |
| TR persists, EN remains available | Switch TR → EN → TR, reload | Header, sidebar, open dialog; desktop screenshots |
| Navigation / table / empty states translated | Visit dashboard, connections, clients, channels, files, bans, permissions, keys, messages, bots, library, reports, settings, widgets | Loaded and empty states; route text inventory |
| Dialogs and authored metadata translated | Connection wizard/help; widget creation; bot automation/extensions; flow palette/templates | Open dialog, menu, selector and template gallery |
| Dynamic messages are Turkish | Media lifecycle labels, counts, interpolated toasts, API refusal | Unit assertions + browser fixture failures |
| User data preserved | English server/song/client names and interpolated values | Exact text assertions, no mutation of stored values |
| Smaller viewport remains usable | 390×844 TR navigation and dialogs | Screenshot and horizontal-overflow check |
| Regression prevention | Static catalog coverage, TS compilation, frontend units + relevant browser tests | Automated exit status |

Exploratory: unknown English service error must produce a Turkish actionable fallback; switch language and reopen a metadata-driven dialog; reload with persisted TR; long user-entered English text must remain unchanged.

Live tests are read-only for this localization pass. Never confirm destructive actions or disclose active privilege keys / credentials in screenshots.

## Verification — 2026-10-21

- Frontend TypeScript check and production build passed. The existing bundle-size advisory remains a warning, not a compilation failure.
- 240 frontend unit tests passed, including static coverage, interpolation, data preservation, native permission labels and runtime-error presentation.
- Static check: 3,521 authored-copy observations; zero uncovered interface translations and zero unwrapped interface text under the checked policy. Protocol identifiers, code examples, original diagnostic output and authored Turkish copy are explicitly outside English-source coverage.
- The broad 324-case browser inventory exposed outdated assertions (renamed complaint column, consolidated reports navigation and improved SSH guidance) and a UI-only runner that blocked service workers. Those expectations/configuration were corrected and the affected/remaining suites were rerun. This is not represented as one uninterrupted green full-suite run.
- The final focused TR suite passed all four cases: twenty routes, metadata-driven dialogs and wizard step/scroll behavior, small viewport/data preservation, and native bot error display with an original-details expand/collapse and EN/TR round trip. The preceding bot/encoder/TR run passed 18 cases.
- Live read-only checks: channels, clients, permissions, files, messages, all three report views, appearance, connection wizard, dashboard and bot hub. No browser page errors were observed; TR–EN–TR and persisted TR were verified. Desktop 1440px and touch-mobile 390px were inspected; no document overflow or short bot-name clipping remained.
- Final Manager image: `teamspeak-managment/manager-hardened:review-20261021-21bf8fa4376d`; healthy, rootless user `10001:10001`. Encrypted pre-update backup: `/opt/teamhub/backups/manager-before-review-20261021T205448Z.tar.gz.gpg`.
- TeamSpeak container identity and start time remained unchanged: `ba90cc5d04159a7f80c62739b90e420e7042d23659e5b9828896487daeb01128`, `2026-10-18T21:59:54.309278604Z`.

The live bot still reports native error 1538 from a bot operation. This change localizes its visible explanation, preserves the error code and keeps original evidence available on demand; it does not claim to repair the underlying operation or prove audible playback. User-entered names and media titles remain unchanged.
