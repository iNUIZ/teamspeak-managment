# PO Token integration QA

Coverage inventory before UI signoff:

- Default PO Token mode; saved cookies not used without opt-in.
- Upload stores only; explicit cookie mode then return to anonymous mode.
- Cookie deletion resets the persistent mode; file is deletable while inactive.
- Provider ready, unavailable, unconfigured; refresh and unavailable-service copy.
- Invalid mode, non-admin access and persistence failure must not switch runtime mode.
- Trusted plugin directory only; loopback provider only; no runtime remote components.
- Local native Windows download through the real per-job media egress proxy.
- TR/EN labels, desktop/mobile fit, failed update and empty-cookie option disabled.

Intentional exclusions: production TeamSpeak audio, production datacenter IP,
Linux/ARM/macOS container runtime, Docker image build and distribution.
No Docker daemon is available on the development PC. This is not an all-platform
runtime certification and does not resolve separate TeamSpeak permission errors.

## Observed results (2026-10-19)

- Common/backend/frontend TypeScript checks passed; frontend production build passed
  (the existing large-bundle warning remains).
- 29 targeted backend unit/API/egress checks passed. The shell-executable subprocess
  fixture suite is not run on Windows; the real Windows downloader checks below
  exercise the actual executable instead.
- Two Playwright TR/EN lifecycle tests passed: upload stores without activating,
  explicit mode change, return to anonymous mode, failed update reverts, provider
  unavailable/unconfigured, deletion and narrow-viewport fit. HTTP responses are
  deterministic mocks, not the production API or a production cookie login.
- Persistent browser visual review at 1440×1000 and 390×844: no horizontal overflow
  or clipped playback controls. The settings page intentionally scrolls vertically.
  Turkish limits/removal text discovered during review was localized as well.
- Native Windows real download via `spawnYtDlp`: provider ready, matching plugin
  loaded, GVS PO Token generated, no account cookies, complete Opus audio fetched
  through the real per-job egress proxy and fully decoded by FFmpeg.
- Real `searchYouTube('semicenk herkes gibisin', 3)` returned three results.
  The selected `GfTBpCuu6yI` then passed the production metadata → download →
  cache pipeline, producing 3,159,306 bytes. Full FFmpeg audio decode passed.
  This isolated test injects an FFmpeg-based cache probe because ffprobe is not
  installed on this PC; packaged production keeps its normal ffprobe verifier.
- PowerShell parser, Bash syntax and automatic-registration regression checks passed.
- npm package dry run includes the trusted service wrapper and excludes account
  cookies, local test media, venvs and local environment files.
- The newly introduced provider's production npm dependency audit reported no
  known advisories at test time. This is not a full application/container audit
  or a security guarantee.

CI now defines native amd64/arm64 provider build/readiness checks with read-only,
non-root containers and no published ports. These jobs have not been run here.
The live Manager has not been deployed or tested for audible TeamSpeak playback.
No credentials, tokens or media are included in this document.
