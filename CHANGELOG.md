# Changelog

This project follows [Semantic Versioning](https://semver.org/).

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
