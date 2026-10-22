# Panel review QA inventory

Scope: moderation, message delivery, report navigation, service settings and localized playback diagnostics. Local fixtures test UI behavior; they do not establish live TeamSpeak delivery or YouTube availability.

## Follow-up live QA inventory (21 October 2026)

- Key tracking: disabled → enabled → observed isolated redemption → persisted receipt → reconnect/reload. Inspect the real receipt in desktop and mobile UI. Verify raw token never enters the history value.
- Delivery: connect an isolated voice client; send private chat; observe the matching incoming private event, not just the Query response. Inspect the configured sender label in compose. Offline mail remains separate.
- Encoder fallback: obtain real capability probes, request unavailable GPU encoder, verify selected working software encoder and a synthetic FFmpeg encode.
- Deployment: encrypted consistent database/config snapshot; non-root Manager; TS6 container identity and start time unchanged; authenticated routes and public health reachable.
- Off-happy-path: recycled client ID rejected before sending; notification permission denial reports a capture gap rather than healthy status; YouTube verification remains a real external restriction, not a fabricated passing test.

| Claim/control | Functional check | Visual state/evidence |
| --- | --- | --- |
| Confirmed regex-ban deletion removes its row | Delete succeeds while list endpoint intentionally returns stale data | Before/after moderation table; failed delete keeps row |
| Messages distinguish live chat and Query mailbox | Named recipient sends chat payload; switch to offline mode requires UID and subject | Both compose states, accepted-not-read copy, failed request inline |
| Reports are grouped by type | Four tabs, legacy URLs redirect without dropping filters | Desktop and mobile report navigation, TR/EN round trip |
| Administrator and connection filters are named | Select named values and inspect request parameters | Audit filters with populated events, retained retired actor |
| Activity capture is honest opt-in | Disabled capture hides historical reconnect error; enable/disable cycle | Disabled empty state, capturing and connection-error states |
| Service info is under servers; templates under permissions | Legacy instance redirect, service tab, group-template tab | Group warning and service scope explanation |
| Existing privilege keys are not usage history | Active key renders with single-use and history limitation | Key list and copy controls |
| Media failures preserve provider cause | Auth warning outranks benign stdin message; response does not expose secrets | Turkish playback error and retriable/blocked state |
| Diagnostics are demand-driven readiness probes | Refresh and copy report; no promise of provider access | Technical output collapsed; status badges translated |
| Localization is real app copy | TR/EN toggle and reload; no translation of usernames or raw logs | Initial and dense views with spacing and small viewport |

Exploratory cases: switch connection while deletion/send is pending; refuse deletion/send and verify no false success; stale journal error while capture is off; old routes with query filters.

Excluded from fixture signoff: actual recipient-client display, hardware encoder availability, remote YouTube anti-bot restrictions, historical consumed-key users (not exposed by active-key listing). No credentials/cookies are test artifacts.

## Verification results

- Backend TypeScript check passed. Targeted message delivery, downloader failure classification and error-handler tests: 19 passed.
- Frontend TypeScript check and production build passed. Frontend unit tests: 232 passed.
- Playwright installed Chrome: 15 targeted tests passed across panel review, media diagnostics and localization suites. Covers confirmed/denied deletion, named chat/offline payloads, legacy redirects, named audit filters, journal toggle cycle, service placement, provider refusal, diagnostics details/copy/refresh, role gates and clipboard fallback/denial.
- Interactive visual inspection: populated audit report at 1440px, named-recipient and offline compose modes, disabled activity report at 390px. No document-level horizontal overflow or runtime errors in the inspected fresh context.
- Raw TeamSpeak log lines and expandable technical diagnostic output intentionally retain their original language. Names, identifiers and user-provided descriptions are not translated.

## Initial fixture-only boundary (superseded by the follow-up below)

The initial fixture pass did not create a release, push commits or update the running installation. The follow-up below records the subsequent live deployment and verification.

The downloader fix preserves the underlying authentication warning when metadata exits with code zero but contains no playable formats. It does not bypass provider restrictions. Real playback from the deployed network and actual display in a recipient's TeamSpeak client still require a live integration test.

The active privilege-key API is not a historical redemption ledger. No retrospective user attribution is inferred. The follow-up adds explicit future event capture rather than fabricating old usage.

## Follow-up outcome — 21 October 2026 (supersedes the earlier exclusions)

The follow-up implements durable, opt-in `notifytokenused` capture with an explicit `tokenused` subscription, SHA-256 key fingerprints, identity-safe cache enrichment, 200-event/30-day retention and reconnect restoration. Raw keys are not stored. A real isolated key redemption appeared in the running UI and survived a Manager restart.

A real temporary voice client received a private message through the Manager message endpoint, including recipient UID validation. The dialog identifies the real Query sender. The tested path is private chat, not a claim that every third-party client supports offline mail.

Actual host encoder capabilities selected software H.264 for an unavailable NVENC request; a synthetic FFmpeg encode passed. Query authentication was repaired from the existing installer credentials; only the Manager's private Docker IPv4 received a Query flood exemption. The TS6 process and container were not restarted.

The review build is installed on the running Manager, after encrypted consistent snapshots. It is not a GitHub release and no push was made. Follow-up checks: 26 backend tests, 11 browser tests, 232 frontend unit tests; private-IP allowlist tests ran on Windows and Linux. YouTube still returns sign-in verification with no audio formats, including for an independent public control video. A different outbound network path requires user direction; no unknown public proxy or paid service was used.
