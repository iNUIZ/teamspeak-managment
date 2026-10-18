# Alpha UI and installation verification

| Area | Controls and states | Evidence |
|---|---|---|
| Localization | TR → EN → TR on widgets, permissions and bot hub; reload persistence | Headings, form labels and screenshots |
| Clipboard | Privilege key and widget embed copy on HTTP; API rejection; modal focus preserved | Paste into an input and compare full value |
| Bot connection | Create, connect, stop, restart; auto voice port; manual NAT override | Client/channel IDs and normal-client list entry |
| Bot failures | Unreachable port and refused handshake | Error message rather than a false Idle state |
| TTS | Disabled engine, TR/EN, limits, cooldown and permissions | Unit tests and Linux voice smoke tests |
| Installation | Server/Manager order, second stack, admin preservation, access summary | Bash and SQLite tests |
| Branding | Original mark/favicon, accent and narrow layout | Desktop and mobile screenshots |

Exploration includes copy in a focus-trapped dialog on insecure HTTP, clipboard denial,
non-default TS6 voice ports, and empty/oversized/shell-like TTS messages.

UI fixtures do not prove real TS connectivity. Record the live versions and architectures
actually exercised; never describe an untested target as verified.
