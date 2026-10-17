# Teamspeak Managment

English · [Türkçe](README.md)

![Teamspeak Managment menu](docs/menu.en.svg)

An interactive Docker Compose deployment and management tool for TeamSpeak 3,
TeamSpeak 6, TS6 Manager, and the Manager music bot.

| Metadata | Value |
|---|---|
| Version | **0.3.1** |
| Developer | **Berat Akar** |
| Source | [GitHub @iNUIZ](https://github.com/iNUIZ) |

## Downloads

- [GitHub Releases](https://github.com/iNUIZ/teamspeak-managment/releases): versioned ZIP, TAR.GZ, and SHA-256 files
- [GitHub Packages](https://github.com/users/iNUIZ/packages?repo_name=teamspeak-managment): npm package

## Components

- TeamSpeak 3 Server with HTTP WebQuery
- TeamSpeak 6 Server with WebQuery
- TS6 Manager web panel
- Manager music bot support
- Service control, logs, backup, and data-preserving updates

Every deployment starts with clean Docker volumes. The repository contains no
passwords, API keys, SSH keys, bot identities, databases, or existing server data.

## Requirements

| Platform | Requirements |
|---|---|
| Ubuntu/Debian x86_64 | Root/sudo access and an internet connection |
| Ubuntu/Debian ARM64 | Root/sudo and internet; amd64 emulation for TS3 |
| Windows 10/11 | Docker Desktop, WSL2, Node.js 18+, and npm 9+ |

The Linux installer adds Docker's official APT repository when Docker is not
installed. Docker Desktop WSL Integration must be enabled on Windows.

## Installation

### Ubuntu or Debian

```bash
git clone https://github.com/iNUIZ/teamspeak-managment.git
cd teamspeak-managment
chmod +x install.sh
sudo ./install.sh
```

### ARM64 Linux

Use the same Linux commands. If a native Manager image is unavailable, the
installer builds an ARM64 image from a pinned upstream commit. The official TS3
image is amd64-only, so the TS3 option verifies Docker binfmt/QEMU emulation.

### Windows 10/11 — PowerShell and npm

1. Install Docker Desktop and enable your distribution under
   **Settings > Resources > WSL Integration**.
2. Install Node.js LTS and npm.
3. Run these commands in the project directory from PowerShell:

```powershell
npm install
npm run setup
```

`npm run setup` starts the PowerShell launcher and executes the shared installer
engine through WSL2. Git Bash is not required.

One-line alternative that downloads the latest Release and verifies its SHA-256:

```powershell
irm https://raw.githubusercontent.com/iNUIZ/teamspeak-managment/v0.3.1/bootstrap.ps1 | iex
```

The command is pinned to the immutable `v0.3.1` release tag. Review
`bootstrap.ps1` before running it in a production environment.

### WSL2 terminal

```bash
chmod +x install.sh
sudo ./install.sh
```

## Network model

TeamSpeak, Manager, and the bot may share one public IP on the same VPS. Internal
traffic uses the private `teamhub` Docker network:

```text
Internet ── voice/file ports ──> TS3 or TS6
HTTPS/domain ── reverse proxy ──> Manager (default 127.0.0.1:3000)
Manager + bot ── private Docker network ──> ts3:10080 or ts6:10080
```

Query and Manager ports bind to localhost by default. Use a TLS reverse proxy
and firewall rules when exposing the panel remotely.

## Data locations

- Linux and WSL2: `/opt/teamhub`
- Windows launcher deployment data: `/opt/teamhub` inside WSL2
- Persistent volumes: `teamhub_ts3-data`, `teamhub_ts6-data`,
  `teamhub_manager-data`, and `teamhub_manager-music`

The generated `.env` contains secrets and must never be committed.

## Initial Manager connection

1. Create the first administrator at the `/setup` URL printed by the installer.
2. For an in-stack TS6 server, use host `ts6` and port `10080`.
3. For TS3, use host `ts3` and port `10080`.
4. Read the corresponding API key from `/opt/teamhub/.env`.

## License and dependencies

Installer code is released under the [MIT license](LICENSE). TeamSpeak software
and `uniskela/ts6-manager` remain subject to their own licenses and terms.
