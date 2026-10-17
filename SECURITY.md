# Security Policy

## Supported versions

Security fixes are provided for the latest release only.

## Reporting a vulnerability

Do not open a public issue containing credentials, private server addresses,
API keys, logs with tokens, or exploit details. Use GitHub's private security
advisory feature under **Security > Advisories > Report a vulnerability**.

Include the affected version, platform, reproduction steps, and impact. Remove
all real credentials and personal data before attaching logs.

## Deployment guidance

- Keep the generated `.env` private and never add it to Git.
- Put the Manager behind TLS and an authenticated reverse proxy when exposed
  to the internet.
- Restrict Query and Manager ports with the host and cloud firewalls.
- Pin tested container versions for production deployments.
- Back up named volumes and deployment configuration before upgrades. Backup
  archives contain `.env` secrets, so store them encrypted and off-server.

## Release verification

Release checksum manifests are signed with Ed25519. The Windows bootstrapper
contains the trusted public key and verifies the manifest signature before it
trusts any SHA-256 value. Manual downloads can be checked with:

```bash
openssl pkeyutl -verify -pubin \
  -inkey security/release-signing-public.pem -rawin \
  -in teamspeak-managment-vX.Y.Z-SHA256SUMS.txt \
  -sigfile teamspeak-managment-vX.Y.Z-SHA256SUMS.txt.sig
```
