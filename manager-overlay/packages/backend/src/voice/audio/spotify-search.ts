/**
 * Spotify catalogue search for `!ara`, using a Spotify developer app (client credentials).
 * Spotify only supplies the track name and artist; the audio is matched on YouTube when a
 * result is chosen, through the same path as a pasted Spotify link. Spotify audio is DRM
 * protected and is never downloaded.
 */

export interface SpotifyTrackResult {
  id: string;
  title: string;
  artist: string;
  /** Seconds. */
  duration: number;
  /** open.spotify.com/track/<id>, played through the Spotify link resolver. */
  url: string;
}

interface SpotifyCredentials { clientId: string; clientSecret: string }

type Fetch = typeof fetch;

const TOKEN_URL = 'https://accounts.spotify.com/api/token';
const SEARCH_URL = 'https://api.spotify.com/v1/search';
const TRACK_ID = /^[A-Za-z0-9]{22}$/;

let cachedToken: { value: string; expiresAt: number; clientId: string } | null = null;

/** SPOTIFY_CLIENT_ID and SPOTIFY_CLIENT_SECRET, or null when Spotify search is not set up. */
export function spotifyCredentials(env: NodeJS.ProcessEnv = process.env): SpotifyCredentials | null {
  const clientId = env.SPOTIFY_CLIENT_ID?.trim();
  const clientSecret = env.SPOTIFY_CLIENT_SECRET?.trim();
  return clientId && clientSecret ? { clientId, clientSecret } : null;
}

async function accessToken(credentials: SpotifyCredentials, fetchImpl: Fetch, signal?: AbortSignal): Promise<string> {
  if (cachedToken && cachedToken.clientId === credentials.clientId && cachedToken.expiresAt > Date.now()) {
    return cachedToken.value;
  }
  const basic = Buffer.from(`${credentials.clientId}:${credentials.clientSecret}`).toString('base64');
  const res = await fetchImpl(TOKEN_URL, {
    method: 'POST',
    headers: { Authorization: `Basic ${basic}`, 'Content-Type': 'application/x-www-form-urlencoded' },
    body: 'grant_type=client_credentials',
    signal,
  });
  // Never echo the response body: a failed login must not leak the app credentials.
  if (!res.ok) throw new Error(`Spotify sign-in failed (HTTP ${res.status}). Check SPOTIFY_CLIENT_ID and SPOTIFY_CLIENT_SECRET.`);
  const body = await res.json() as { access_token?: unknown; expires_in?: unknown };
  if (typeof body.access_token !== 'string' || !body.access_token) throw new Error('Spotify sign-in returned no token');
  const lifetime = typeof body.expires_in === 'number' ? body.expires_in : 3600;
  cachedToken = { value: body.access_token, clientId: credentials.clientId, expiresAt: Date.now() + Math.max(60, lifetime - 60) * 1000 };
  return cachedToken.value;
}

/** Tracks from the Spotify catalogue, or [] when Spotify search is not configured. */
export async function searchSpotifyTracks(
  query: string,
  limit = 6,
  signal?: AbortSignal,
  options: { env?: NodeJS.ProcessEnv; fetchImpl?: Fetch; market?: string } = {},
): Promise<SpotifyTrackResult[]> {
  const credentials = spotifyCredentials(options.env);
  if (!credentials) return [];
  const fetchImpl = options.fetchImpl ?? fetch;
  const token = await accessToken(credentials, fetchImpl, signal);
  const url = new URL(SEARCH_URL);
  url.searchParams.set('q', query);
  url.searchParams.set('type', 'track');
  url.searchParams.set('limit', String(Math.min(Math.max(limit, 1), 10)));
  url.searchParams.set('market', options.market ?? options.env?.SPOTIFY_MARKET ?? process.env.SPOTIFY_MARKET ?? 'TR');
  const res = await fetchImpl(url, { headers: { Authorization: `Bearer ${token}` }, signal });
  if (res.status === 401) cachedToken = null;
  if (!res.ok) throw new Error(`Spotify search failed (HTTP ${res.status})`);
  const body = await res.json() as { tracks?: { items?: unknown[] } };
  const results: SpotifyTrackResult[] = [];
  for (const raw of body.tracks?.items ?? []) {
    const track = raw as { id?: unknown; name?: unknown; duration_ms?: unknown; artists?: Array<{ name?: unknown }> };
    if (typeof track.id !== 'string' || !TRACK_ID.test(track.id) || typeof track.name !== 'string') continue;
    const artist = (track.artists ?? []).map((a) => (typeof a.name === 'string' ? a.name : '')).filter(Boolean).join(', ');
    results.push({
      id: track.id,
      title: track.name,
      artist: artist || 'Unknown',
      duration: typeof track.duration_ms === 'number' ? Math.round(track.duration_ms / 1000) : 0,
      url: `https://open.spotify.com/track/${track.id}`,
    });
  }
  return results;
}

export function resetSpotifyTokenForTests(): void { cachedToken = null; }
