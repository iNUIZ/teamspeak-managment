# Changelog

This project follows [Semantic Versioning](https://semver.org/).

## [0.3.1] - 2026-10-17

### Security

- Forced Manager to bind to localhost; remote access now requires a TLS reverse proxy or SSH tunnel.
- Pinned the one-line installer documentation to the protected release tag.
- Added weekly Dependabot checks and Trivy source, secret, configuration, and container scanning.
- Uploaded container findings to GitHub code scanning in SARIF format.
- Fixed PowerShell CI parsing so errors from both scripts are preserved.
- Updated TS6 Manager to 1.11.1 with immutable image and source checksums.
- Added locally built hardened TS3 and TS6 images with current base OS security updates.
- Made fixed HIGH/CRITICAL container findings fail the Security workflow.

## [0.3.0] - 2026-10-17

### Changed

- Renamed the project to Teamspeak Managment.
- Reworked both README files as concise technical documentation.
- Added a separate English menu illustration.
- Added an npm-driven Windows setup command using PowerShell and WSL2.
- Removed the Git Bash requirement from the Windows installation path.
- Added a checksum-verifying one-line PowerShell bootstrap installer.

## [0.2.0] - 2026-10-17

### Added

- Data-preserving TeamSpeak Manager update workflow with an automatic backup.
- CPU-aware conservative TS6 worker and query tuning.
- Container log rotation, init handling, health checks, and graceful shutdowns.
- Project ownership metadata for Berat Akar (`@iNUIZ`).
- Documented the recommended one-public-IP/private-container-network topology.

### Security

- Pinned TeamSpeak, Manager, and helper images to immutable manifest digests.
- Pinned ARM source fallback to an immutable upstream commit.
- Added SHA-256 verification before extracting the ARM source fallback.
- Pinned GitHub Actions to an audited commit.
- Stopped printing generated API keys in the terminal guide.
- Added strict public-host input validation and backup failure recovery.
- Changed Query and Manager host bindings to local-only secure defaults.

## [0.1.0] - 2026-10-17

### Added

- Interactive TeamSpeak 3 and TeamSpeak 6 installation.
- TS3 HTTP WebQuery activation and automatic API-key generation.
- TS6 Manager and music bot setup guidance.
- Separate reboot restart-policy selection for each component.
- Service status, logs, lifecycle controls, backup, and uninstall menus.
- Windows, Linux x86_64, and Linux ARM64 environment handling.
- ARM64 fallback build for TS6 Manager and amd64 emulation check for TS3.
- Secret-safe defaults and repository ignore rules.
