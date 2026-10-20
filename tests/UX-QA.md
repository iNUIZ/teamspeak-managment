# Panel UX acceptance inventory

Scope: preserve the existing visual theme; simplify Turkish terminology and navigation;
opt-in now-playing avatar and description; search-selection failure feedback.

| Claim/control | Functional check | Visual check |
| --- | --- | --- |
| TeamSpeak terminology | TR/EN language cycle, server list and selector | Desktop labels and mobile menu |
| Menu search | Turkish/English search, no match, clear button, Escape | Filtered and empty menu, 390px |
| Server controls | Start, stop confirmation, cancel, failed request | Pending/error state; do not stop production servers |
| Appearance preserved | Theme/accent switch and restore | Desktop/mobile screenshots |
| Track profile | Opt-in cover mode, track change, idle reset, missing art, cancellation | Avatar mode control and error |
| Music failures | Failed numeric selection replies; no false 'playing' claim | Chat requires a real voice server |

Exploratory cases: long Turkish labels at 390px; empty menu search; rejected
clipboard; rapid track replacement while artwork is loading.

Do not treat mocked browser tests as proof of TeamSpeak audio, file-transfer
permissions, YouTube availability, or a successful live deployment. Record exact
executed tests and exclusions in the final report.

## Completed, 2026-10-19

- Production build and frontend/backend typechecks passed.
- Frontend unit tests: 230 passed. Targeted backend tests: 90 + 18 passed.
- Broad browser inventory: 306 scenarios; one documentation screenshot scenario
  was skipped by its existing condition. Failed cases were investigated and
  rerun after fixes. The entire inventory was not green in one uninterrupted run.
- Final latest-build closeout: 37/37 passed (Bot Console, responsive header,
  sidebar, TR/EN navigation search). Separate reruns covered PWA, wide layout,
  badges, clipboard, avatar mode and localization.
- Desktop 1366x768 and mobile 375x844 screenshots reviewed separately; final
  menu headings fit, selectors are legible, no horizontal document overflow.
- Linux-only downloader subprocess fixture failed to launch on Windows;
  live audio, TS avatar transfer and optional Linux TTS were not signed off.
- Full findings and limitations: ../docs/panel-ux-review.md.
