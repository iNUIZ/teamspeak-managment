# Screenshot issue acceptance plan

1. Auto-DJ TR/EN; radio mode preset; save, disable, invalid/missing source and retry.
2. Ten radio defaults (5 TR/5 EN), no duplicates, user stations retained, deleted
   defaults not resurrected; arbitrary additional stations remain supported.
3. Widget selection uses actual server name/voice port; non-1 SID, connection
   switch, error, preview and failed create; correct public endpoint.
4. Privilege key create: server/channel group, channel requirements, error,
   copied full secret; string token_type=0 renders correctly; no token in audit.
5. Query SSH: missing credentials versus permission denial; no unusable mkdir
   action; setup link; installer preserves manually configured credentials.
6. Files/icons/banners/messages/logs/audit visible TR copy, including dialogs.
7. Logical server snapshot export: admin only, named target, clear exclusions.
   Full database/media backup must not be misrepresented as a Query snapshot.

Playwright: normal input + separate desktop/mobile visual pass. Off-happy-path:
invalid/missing server SID; failed SSH; double create; deleted radio source.
No live server stop, delete, permission grant or backup restore as a QA shortcut.

## Encoder screen: verified 2026-10-19

- Frontend typecheck, production build and all 231 frontend unit tests passed.
- `encoder-capabilities.spec.ts`: 2/2 passed on the final production build.
  Covers EN probe output, TR software-only availability, missing hardware,
  explicit retry request and switching back to English.
- Interactive desktop (1366x768) and mobile (390x844) inspection completed;
  the capability list wraps vertically on mobile without horizontal overflow.
  A fresh context with service workers blocked was also checked to avoid
  confusing an earlier cached build with the final production artifacts.
- Missing NVIDIA runtime and skipped platform/device prerequisites are not
  reported as generic encoding failures. Actual failed probes still are.
- ffmpeg commands and diagnostic output remain unmodified technical evidence.
- Browser probes used deterministic API fixtures. These results do NOT prove
  real hardware encoding, TeamSpeak playback or live server deployment.
- Items 1–7 above remain separate acceptance work; this focused pass does not
  sign them all off.
