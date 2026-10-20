# YouTube playback: PO Token by default

Manager uses an anonymous PO Token provider before asking for account cookies.
This applies to text commands (`!ara` and result selection), library downloads
and YouTube-backed Spotify matches. Spotify is a search/metadata source, not a
direct Spotify audio downloader. Public content only; account-only access can
still require authentication and the content owner's authorization.

## Packaged installation

The hardened Manager image includes bgutil provider **2.0.2** and its matching
yt-dlp plugin. The provider runs automatically as UID 10001, supervised alongside
the backend. It listens only on **127.0.0.1:4416 inside the Manager container**.
Do not publish port 4416, add a reverse proxy to it, or open it in a firewall.

- Linux amd64/arm64: use the normal installer; the provider is built for the
  image's target architecture, not through amd64 emulation.
- Windows: `npm run setup` / `install.ps1` uses Docker Desktop + WSL2. The same
  Manager image includes the service; no Windows Python setup or manual token
  copying is needed. This is not a native Windows TeamSpeak server installer.
- macOS: use the supported Docker installation path. Native macOS execution of
  this integration has not been runtime-verified here.

Existing Manager images must be **rebuilt and replaced while preserving their
data volumes**. Pulling source alone does not update a running container. Do not
delete `.env`, the database, music volumes or server volumes to apply this change.

## Panel controls

Open **Settings → YouTube** (TR: **Ayarlar → YouTube**). The default is
**PO Token — cookieless**. The status checks the local provider's `/ping` and
expected version; it does not prove that YouTube accepts the server's IP.

Existing cookie files are retained, but are not used by default. Uploading a
cookie file only stores it; explicitly select **Optional account cookies** to
enable account access. Switching back disables account-cookie use without
deleting the file. The chosen mode persists across restarts. Removing cookies
returns playback to PO Token mode. Cookie changes apply to new playback jobs,
not already-running downloads. Cookies are private credentials: never commit
them, paste them into an issue or share backups containing them unencrypted.

Saved panel uploads take precedence over `YT_COOKIE_FILE`, including after a
restart. The panel removes only its own uploaded file, not an operator-managed
environment file; remove that environment setting/file separately if required.

PO Tokens and cookies do **not** guarantee removal of IP, regional, age,
account or rate restrictions. A network failure is not automatically a cookie
problem. Check provider readiness and outbound connectivity before enabling
cookies. YouTube anti-abuse restrictions can remain even when tokens work.

## Native backend development (Windows/Linux/macOS)

The media integration does not depend on Oracle or Linux paths. A native
deployment needs the matching trusted provider/plugin, Node >=22 for bgutil,
Python/yt-dlp with the pinned EJS component, plus the app's usual media tools.
Use the immutable source and lockfile referenced in
`docker/manager-hardened.Dockerfile`; do not install arbitrary third-party
yt-dlp plugins. Start `server/build/main.js --host 127.0.0.1 --port 4416`
as a private service with stdout/stderr disabled (upstream diagnostics may
contain tokens). Supervise and stop it with the backend; native process/service
installation remains operator-managed, unlike the automatic Docker package.

Set these deployment environment variables, not request parameters:

| Variable | Value |
| --- | --- |
| `YT_DLP_PATH` | Absolute path to the trusted executable, e.g. a venv's `Scripts/yt-dlp.exe` on Windows |
| `YT_DLP_NODE_PATH` | Optional absolute Node executable for yt-dlp's JS runtime; packaged images use Deno |
| `YOUTUBE_POT_PLUGIN_DIR` | Absolute trusted plugin parent directory; its child must contain `yt_dlp_plugins` |
| `YOUTUBE_POT_URL` | `http://127.0.0.1:4416` (only IPv4 loopback origins accepted) |

Host configuration and automatic plugin discovery stay disabled. Only the
explicit trusted plugin is permitted. Media jobs still use the existing
per-job egress proxy; the provider receives the same job proxy for token
generation. Do not turn off media egress protection to make this work.

## Dependency provenance and licensing

Provider/plugin source commit:
`26475e9d3665b972c4aeb83b4dbaafcf64b88438`.
The build checks the source archive's SHA-256 before extraction, uses its npm
lockfile, builds Canvas from source, removes dev dependencies, and retains the
complete corresponding source archive and GPL-3.0 license in
`/opt/teamhub-pot/`. This separately packaged component is **GPL-3.0-only**, not
MIT; the repository's MIT license does not replace upstream or transitive
dependency licenses. Preserve these files when distributing the image.
Provider/token diagnostics are not copied into container logs or API responses.
No static token, account cookie, browser profile or test media is shipped.

Primary references:
[bgutil provider](https://github.com/Brainicism/bgutil-ytdlp-pot-provider),
[yt-dlp PO Token guide](https://github.com/yt-dlp/yt-dlp/wiki/PO-Token-Guide).
