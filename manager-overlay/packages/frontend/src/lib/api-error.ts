import { translate as tr, translateUiError } from './i18n';
const GENERIC_AXIOS = /^Request failed with status code \d+$/i;

export type ApiErrorPresentation = {
  title: string;
  message: string;
  retryable: boolean;
  action?: string;
};

function axiosData(error: unknown): { status?: number; data?: any; message?: string } {
  const err = error as any;
  return {
    status: err?.response?.status,
    data: err?.response?.data,
    message: err?.message,
  };
}

const REASON_DEFAULTS: Record<string, Partial<ApiErrorPresentation>> = {
  media_auth_required: { title: 'The media provider requires verification', retryable: false, action: 'Refresh your YouTube cookies in Settings → YouTube and check outbound connectivity. Cookies do not guarantee access.' },
  media_download_failed: { title: 'The media download failed', retryable: true },
  stream_already_running: { title: 'A video is already streaming', retryable: false },
  stream_not_running: { title: 'No active stream', retryable: false, action: 'Start a video first.' },
  stream_starting: { title: 'The stream is still starting', retryable: true },
  stream_stopping: { title: 'The stream is stopping', retryable: true },
  sidecar_unavailable: { title: 'Media sidecar unavailable', retryable: true, action: 'Check that the sidecar is running, then retry.' },
  sidecar_timeout: { title: 'Media sidecar timed out', retryable: true },
  request_timeout: { title: 'The request timed out', retryable: true },
  source_invalid: { title: 'Invalid source', retryable: false },
  source_not_found: { title: 'File not found', retryable: false },
  source_refused: { title: 'This source cannot be used', retryable: false },
  source_timeout: { title: 'The source timed out', retryable: true },
  source_unavailable: { title: 'The source could not be played', retryable: true },
  bot_not_connected: { title: 'Bot is not connected', retryable: true, action: 'Start the bot, then try again.' },
  bot_already_started: { title: 'This bot is already started', retryable: false },
  unexpected_error: { title: 'Something went wrong', retryable: true, action: 'Try again. If it keeps happening, check the server logs.' },
  ts_permission_denied: { title: 'Insufficient TeamSpeak permissions', message: 'The Query account needs permission for this operation. Successful connection tests do not grant write access.', retryable:false },
  ts_query_flood: { title:'TeamSpeak Query is cooling down', message:'Wait for the Query request limit to reset, then retry.', retryable: true },
  ts_query_starting: { title:'TeamSpeak is still starting', retryable: true },
  ts_ssh_disconnected: { title:'SSH reconnecting', retryable: true },
};

export function apiErrorPresentation(error: unknown, fallback: string): ApiErrorPresentation {
  const { status, data, message } = axiosData(error);
  const reason = typeof data?.reason === 'string' ? data.reason : undefined;
  const defaults = (reason && REASON_DEFAULTS[reason]) || {};
  const rawTitle = typeof data?.error === 'string' ? data.error : '';
  const title = (rawTitle && !GENERIC_AXIOS.test(rawTitle) ? rawTitle : defaults.title) || fallback;
  const details = typeof data?.details === 'string' ? data.details : '';
  const axiosMsg = typeof message === 'string' && !GENERIC_AXIOS.test(message) ? message : '';
  const bodyMessage = details || (axiosMsg && axiosMsg !== title ? axiosMsg : '') || defaults.message || '';
  const retryable = typeof data?.retryable === 'boolean'
    ? data.retryable
    : defaults.retryable === true || status === 429 || status === 503;
  const action = defaults.action;
  return {
    title: translateUiError(title, defaults.title || fallback),
    message: translateUiError(bodyMessage, defaults.message || 'Check the technical logs for details.'),
    retryable,
    action: action ? tr(action) : undefined,
  };
}

export function apiErrorMessage(error: unknown, fallback: string): string {
  const p = apiErrorPresentation(error, fallback);
  if (p.message && p.message !== p.title) return `${p.title}. ${p.message}`;
  if ((error as any)?.response?.data?.reason === 'media_auth_required' && p.action) return `${p.title}. ${p.action}`;
  return p.title || fallback;
}

/** TeamSpeak `logview` could not open/read its logfile (error 2052 / file I/O). */
export function isTeamSpeakLogviewIo(error: unknown): boolean {
  const err = error as any;
  const data = err?.response?.data;
  if (data?.reason === 'ts_logview_io') return true;
  if (Number(data?.code) === 2052) return true;

  const text = `${data?.error || ''} ${data?.details || ''} ${err?.message || ''}`.toLowerCase();
  return text.includes('file input/output error') || text.includes('log file unavailable');
}

/** TeamSpeak Query still booting / resetting sockets (gated 503, or transport hang-ups). */
export function isTeamSpeakStarting(error: unknown): boolean {
  if (isTeamSpeakLogviewIo(error)) return false;
  if (isTeamSpeakSshDisconnected(error)) return false;

  const err = error as any;
  const status = err?.response?.status;
  const reason = err?.response?.data?.reason;
  if (status === 503 && reason === 'ts_query_starting') return true;

  const text = `${err?.response?.data?.error || ''} ${err?.response?.data?.details || ''} ${err?.message || ''}`.toLowerCase();

  // Explicit warming copy from TeamSpeakUnavailableError — not every generic 503.
  if (text.includes('query is still starting') || text.includes('still coming up')) return true;

  // Unmapped hang-ups / resets while Query boots (often still HTTP 502).
  if (status === 502 || status === 503) {
    return (
      text.includes('socket hang up')
      || text.includes('econnreset')
      || text.includes('econnrefused')
      || text.includes('etimedout')
      || text.includes('connection lost before handshake')
    );
  }
  return false;
}

/** EventBridge SSH session temporarily down (often Query flood cooldown after redeploy). */
export function isTeamSpeakSshDisconnected(error: unknown): boolean {
  const err = error as any;
  const reason = err?.response?.data?.reason;
  if (reason === 'ts_ssh_disconnected') return true;

  const status = err?.response?.status;
  const text = `${err?.response?.data?.error || ''} ${err?.response?.data?.details || ''} ${err?.message || ''}`.toLowerCase();
  if (status === 503 || status === 502) {
    return text.includes('ssh is not connected') || text.includes('ssh not connected');
  }
  return false;
}

export function teamSpeakConnectionTitle(error: unknown, failedTitle = 'Connection failed'): string {
  if (isTeamSpeakLogviewIo(error)) return tr('TeamSpeak log file unavailable');
  if (isTeamSpeakSshDisconnected(error)) return tr('SSH reconnecting');
  return tr(isTeamSpeakStarting(error) ? 'TeamSpeak is still starting' : failedTitle);
}

export function teamSpeakRefreshTone(error: unknown | null | undefined): 'live' | 'degraded' | 'starting' {
  if (!error) return 'live';
  if (isTeamSpeakStarting(error) || isTeamSpeakSshDisconnected(error)) return 'starting';
  return 'degraded';
}

/**
 * Retry Query-backed pages through flood cooldowns and cold-start transport failures.
 * Flood (429): do not stampede. Starting / SSH reconnect only: keep trying a bit longer.
 * Logview I/O (2052): never auto-retry — hammering TeamSpeak can worsen file locks.
 * Ordinary 503 (e.g. refresh-failure fixtures) use the short retry budget.
 */
export function teamSpeakQueryRetry(failureCount: number, error: unknown): boolean {
  if (isTeamSpeakLogviewIo(error)) return false;
  const status = (error as any)?.response?.status;
  if (status === 429) return false;
  if (isTeamSpeakStarting(error) || isTeamSpeakSshDisconnected(error)) return failureCount < 10;
  return failureCount < 3;
}

export function teamSpeakQueryRetryDelay(attempt: number, error: unknown): number {
  if (isTeamSpeakLogviewIo(error)) return 0;

  const retryAfterHeader = Number((error as any)?.response?.headers?.['retry-after']);
  const retryAfterBody = Number((error as any)?.response?.data?.retryAfterSeconds);
  const retryAfter = Number.isFinite(retryAfterHeader) && retryAfterHeader > 0
    ? retryAfterHeader
    : (Number.isFinite(retryAfterBody) && retryAfterBody > 0 ? retryAfterBody : 0);
  if (retryAfter > 0) return Math.min(60_000, retryAfter * 1000);

  if (isTeamSpeakStarting(error) || isTeamSpeakSshDisconnected(error)) {
    return Math.min(8_000, 1_000 * 2 ** attempt);
  }
  return Math.min(1_500, 300 * 2 ** attempt);
}
