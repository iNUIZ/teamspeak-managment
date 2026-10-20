# Bot extension settings acceptance

- No global TTS settings tab; existing bot-level values take precedence over
  read-only legacy defaults. No unintentional reset of existing configurations.
- Each bot menu opens Extensions and commands: commands, TTS, custom library,
  packages. Library scope is explicitly shared; bot switches are isolated.
- Switch full cycle (on/off/on), aliases, no effect on another bot; runtime
  command dispatch must enforce saved switches, not just display them.
- TTS default voice, volume, length and cooldown; validation, loading/error/retry,
  missing engine; save to the selected bot only. No automatic root installer.
- Widget dialog heading/body/labels/error/copy feedback in TR and EN.
- Console and downloader error messages localized without rewriting raw logs,
  bot names, URLs or user-defined command content.
- Desktop/mobile visual inspection of dialog tabs, search, TTS and widget.
- Off-happy-path: failed save keeps previous state; invalid TTS values cannot
  save; failed initial fetch can retry; stale global TTS PUT cannot change bots.
- Live playback and permission fixes require live evidence, not fixture tests.

## Results (2026-10-19)

- Frontend and backend typecheck: passed. Production UI build: passed (existing
  main-bundle size warning remains).
- Frontend unit tests: 231 passed.
- Focused backend/API tests: 59 passed, including role checks and scoped writes.
- Command regression suite: 149 passed in one run. This includes real SQLite
  IPTV history tests after fixing their Windows file-URI initialization.
- Browser regression: 17 passed; final extension/widget TR+EN scenarios rerun
  after the final layout adjustment: 3 passed.
- Desktop 1366x768 and mobile 390x844 inspected separately. Fixed the mobile
  dialog title/close-button collision. Command tabs wrap, the long TTS form
  scrolls to Save, and there is no horizontal page overflow.
- Manual test fixtures verified saving/reopening a bot voice setting; backend
  tests separately prove that another bot retains its own settings.
- No live deployment, real TTS synthesis, valid YouTube authentication or
  resolution of the deployed TeamSpeak permission refusal is signed off.
