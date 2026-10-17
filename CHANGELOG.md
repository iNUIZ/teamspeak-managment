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
- Included `.env` and `compose.yml` in protected backup archives so credentials
  and deployment configuration can be restored with persistent data.
- Added Ed25519-signed checksum manifests and signature verification to the
  Windows bootstrapper before downloaded package hashes are trusted.
