# Changelog

This project follows [Semantic Versioning](https://semver.org/).

## [0.3.2] - 2026-10-17

### Security

- Fixed refresh-token reuse detection so replay revokes the complete session
  token family, including tokens issued after the stolen token.
- Added a hardened Manager image that runs as UID/GID 10001 with no Linux
  capabilities, `no-new-privileges`, and a read-only root filesystem.
- Dropped all Linux capabilities and enabled `no-new-privileges` for TS3 and TS6.
- Added base OS upgrades to the hardened Manager image and scan that exact image.
- Gated signed release publication on the complete security workflow.
- Encrypted backups with AES-256-CBC and PBKDF2 before writing them to disk.

### Fixed

- Backup now detects and stops only running TeamHub services, then restarts only
  the services that were running before the backup.

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
- Included `.env` and `compose.yml` in protected backup archives so credentials
  and deployment configuration can be restored with persistent data.
- Added Ed25519-signed checksum manifests and signature verification to the
  Windows bootstrapper before downloaded package hashes are trusted.
