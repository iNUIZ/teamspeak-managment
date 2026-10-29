import { isIP } from 'node:net';

/** A bot that joins another TeamSpeak server as a plain voice client. */
export interface ExternalServerAddress {
  host: string;
  /** Explicit port, or null to use the nickname lookup or the TeamSpeak default. */
  port: number | null;
}

const DEFAULT_VOICE_PORT = 9987;
const NICKNAME_LOOKUP_URL = 'https://named.myteamspeak.com/lookup?name=';
const HOST_LABEL = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/i;

function parsePort(text: string): number {
  const port = Number(text);
  if (!/^\d{1,5}$/.test(text) || port < 1 || port > 65535) throw new Error('Invalid external server port');
  return port;
}

/** "host", "host:port", "[v6]:port" or a myTeamSpeak nickname such as "25krs". Empty means none. */
export function parseExternalHost(input: unknown): ExternalServerAddress | null {
  if (input == null) return null;
  if (typeof input !== 'string') throw new Error('Invalid external server address');
  const text = input.trim();
  if (!text) return null;
  if (text.length > 260) throw new Error('Invalid external server address');

  const bracketed = /^\[([^\]]+)\](?::(\d+))?$/.exec(text);
  if (bracketed) {
    if (isIP(bracketed[1]) !== 6) throw new Error('Invalid external server address');
    return { host: bracketed[1], port: bracketed[2] ? parsePort(bracketed[2]) : null };
  }
  if (isIP(text) === 6) return { host: text, port: null };

  const at = text.lastIndexOf(':');
  const host = at >= 0 ? text.slice(0, at) : text;
  const port = at >= 0 ? parsePort(text.slice(at + 1)) : null;
  if (isIP(host) !== 4 && !host.split('.').every((label) => HOST_LABEL.test(label))) {
    throw new Error('Invalid external server address');
  }
  return { host: host.toLowerCase(), port };
}

/** The value stored for a bot, so "25KRS " and "25krs" save the same way. */
export function normalizeExternalHost(input: unknown): string | null {
  const address = parseExternalHost(input);
  if (!address) return null;
  const host = isIP(address.host) === 6 ? `[${address.host}]` : address.host;
  return address.port ? `${host}:${address.port}` : host;
}

/** A dotless, non-IP name is a myTeamSpeak server nickname, as typed into the TeamSpeak client. */
function isServerNickname(address: ExternalServerAddress): boolean {
  return address.port === null && isIP(address.host) === 0 && !address.host.includes('.');
}

export type FetchText = (url: string) => Promise<string>;

const fetchText: FetchText = async (url) => {
  const res = await fetch(url, { signal: AbortSignal.timeout(10_000) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.text();
};

/** Where the voice client connects for a saved external address. */
export async function resolveExternalServer(
  stored: string,
  lookup: FetchText = fetchText,
): Promise<{ host: string; port: number }> {
  const address = parseExternalHost(stored);
  if (!address) throw new Error('Invalid external server address');
  if (!isServerNickname(address)) return { host: address.host, port: address.port ?? DEFAULT_VOICE_PORT };

  // A dotless name can also be a local host (a Docker service name), so an unknown
  // or unreachable nickname falls back to connecting to the name itself.
  let resolved: ExternalServerAddress | null = null;
  try {
    const answer = (await lookup(NICKNAME_LOOKUP_URL + encodeURIComponent(address.host))).trim();
    resolved = answer ? parseExternalHost(answer.split(/[\s,]+/)[0]) : null;
  } catch { /* fall back below */ }
  if (!resolved || isServerNickname(resolved)) return { host: address.host, port: DEFAULT_VOICE_PORT };
  return { host: resolved.host, port: resolved.port ?? DEFAULT_VOICE_PORT };
}

/**
 * Command routing keys bots by connection and virtual server. An external bot gets a scope
 * no ServerQuery event can carry, so it never answers or moves for the connection's own server.
 */
export function externalCommandScope(botId: number): number {
  return -Math.abs(botId);
}
