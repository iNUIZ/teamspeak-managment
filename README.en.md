# TeamHub Universal Voice Stack

English · [Türkçe](README.md)

![TeamHub Universal Voice Stack menu](docs/menu.svg)

An interactive, clean-room installer for TeamSpeak 3, TeamSpeak 6, TS6 Manager,
and its music bot module. It does not contain copied databases, identities,
passwords, API keys, SSH keys, or configuration from an existing server.

### TeamHub Universal Voice Stack

Version

**0.2.0**

Developer

**Berat Akar**

Source

[GitHub @iNUIZ](https://github.com/iNUIZ)

## Supported platforms

- Debian and Ubuntu on x86_64 or ARM64
- Windows 10/11 with Docker Desktop and Git Bash or WSL

The installer uses Docker Compose and stores all application data in named
volumes. Secrets are generated locally during setup and kept out of Git.

## Quick start

Linux:

```bash
chmod +x install.sh
sudo ./install.sh
```

Windows PowerShell, with Docker Desktop running:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\install.ps1
```

## Components

| Component | Image | Persistent storage |
|---|---|---|
| TeamSpeak 3 | `teamspeak:3.13.8` | `teamhub_ts3-data` |
| TeamSpeak 6 | `teamspeaksystems/teamspeak6-server:6.0.0-beta13.1` | `teamhub_ts6-data` |
| TS6 Manager and music bot | `ghcr.io/uniskela/ts6-manager/all-in-one:1.11.0` | Manager data and music volumes |

The TS3 option enables HTTP WebQuery and automatically creates a manage-scope
API key for the Manager. TS6 receives a generated WebQuery API key. Both are
stored in the private installation environment file.

The official TS3 image is amd64-only. On ARM64, the installer verifies that
Docker's amd64 emulation works before continuing. For Manager, it first tries a
native published image; if none is available, it builds the upstream all-in-one
image locally for the host architecture.

TS6 worker and query thread counts are derived from the detected CPU count
within conservative server-supported limits. Containers use bounded log
rotation, an init process, and graceful shutdown periods for predictable
long-running operation.

Query ports bind to `127.0.0.1` by default while the Manager continues to reach
the servers over the private Docker network. The Manager UI is also local-only
by default and requires an explicit installer choice before being exposed to
other devices.

## Network architecture and IP selection

When TeamSpeak, Manager, and the music bot run on the same VPS, they normally
share one public IP. The bot does not need a second public IP. Each component
still has an isolated container address and uses private service discovery:

```text
Internet ── voice/file ports ──> TS3 or TS6
User ── HTTPS/domain ──────────> reverse proxy ──> Manager (127.0.0.1:3000)
Manager + music bot ── private Docker network ──> ts3:10080 or ts6:10080
```

- TeamSpeak clients use the VPS public IP or a domain pointing to it.
- Manager uses `ts3` or `ts6` as the host for an in-stack connection, not the
  VPS public IP.
- Query ports stay off the public internet; Manager reaches them through the
  private `teamhub` network.
- The panel binds to `127.0.0.1:3000` by default. A domain and HTTPS reverse
  proxy are recommended for remote access.
- A bot on a different physical host may have another public IP; prefer a VPN
  or private network for Query access in that topology.

## Data locations

- Linux: `/opt/teamhub`
- Windows through Git Bash: `~/.teamhub`
- WSL follows the Linux location

Never publish the generated `.env`, databases, cookie files, backups, or keys.

Container versions are pinned to immutable SHA-256 manifest digests so a reused
tag cannot silently change the installed artifact.

## First Manager login

1. Open the `/setup` URL printed by the installer.
2. Create the first administrator account. There are no default credentials.
3. Add the TeamSpeak connection under `Settings > Connections`.
4. For an in-stack TS6 server use host `ts6`, port `10080`, and `TS6_API_KEY`.
5. For an in-stack TS3 server use host `ts3`, port `10080`, and `TS3_API_KEY`.
6. Create a bot under `Music Bots > Add Bot` and select its channel.

## Project documentation

- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)
- [Changelog](CHANGELOG.md)
- [MIT license](LICENSE)

Third-party TeamSpeak software and container images retain their own licenses
and terms. This repository licenses only the installer code and documentation.

## Upstream dependencies

`TeamHub Universal Voice Stack` is the name of this repository. References to `teamspeak`,
`teamspeaksystems`, and `uniskela/ts6-manager` are the real image and source
names of the installed upstream components, not stale project branding or
misdirected links. Those projects retain their own names, licenses, and terms.
