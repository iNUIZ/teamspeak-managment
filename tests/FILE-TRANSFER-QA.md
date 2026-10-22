# Channel file transfer QA inventory

Verify protocol types (0 directory / 1 file) and seconds/milliseconds dates.
Create a Turkish-named folder on a Unicode-refusing server: use a disclosed,
non-overwriting ASCII retry and verify the directory on the server before success.
Navigate into that directory, upload a known binary file, verify the stored size,
download and compare its SHA-256/bytes, delete file and then only the owned test folder.
Repeat a second time without a page reload. Existing files/folders must remain intact.

Controls: channel selection, parent/root breadcrumbs, upload file picker/progress,
mkdir open/submit/cancel, download, delete open/confirm/cancel, summary refresh,
TR/EN switch. Inspect initial/empty/loaded/busy/error views at desktop and 390px.
Off-happy paths: duplicate file never overwrites, missing/oversize/invalid paths,
unauthorized viewer, rejected server path, transfer failure, context switch while busy.
Check all visible Files strings, tooltips, errors, confirmations and toast messages in TR.
Do not expose credentials, transfer keys or unrelated server data in test artifacts.

## Verified — 2026-10-21

- Backend/common/frontend TypeScript and production builds passed; 32 targeted backend tests, 232 frontend unit tests and 14 targeted browser tests passed. The Files lifecycle was rerun after the mobile control adjustment.
- Actual ARM64 server: Turkish mkdir rejected by TS6 with 2054 and upload rejected with 2052; conditional ASCII retry now succeeds and discloses the stored name. Other I/O errors are not treated as Unicode automatically.
- Two live browser upload/download/delete rounds completed without reloading between rounds. Downloaded binary content matched all original bytes (4,129 bytes and 20 bytes respectively).
- Attempted duplicate upload returned 409. Downloading again confirmed the original bytes were unchanged.
- File/folder protocol types and millisecond timestamps were verified against the real server, not only a fixture. Manual summary scanning and live TR/EN switching were exercised.
- Deployment preserves data volumes, takes encrypted backups, keeps Manager non-root, and does not restart TeamSpeak. The review image synchronizes the shared package at the actual packaged runtime path.
- Only owned test files and the owned temporary folder were removed. Operator files/channels were not deleted.
- A separate 390px touch-enabled browser context also uploaded, downloaded, byte-compared and deleted a real file on the final image. Both action buttons remained visible and 32px wide, without horizontal overflow; live TR→EN→TR switching passed. No renderer exceptions were observed in the desktop test session.

Not a universal filesystem/OS certification: automatic name compatibility is used only after the connected server rejects that Unicode path. Names remain unchanged on servers that accept them. Uploads never request overwrite. Current upload/download limit is 25 MB per file.
