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
- Back up named volumes before upgrades.
