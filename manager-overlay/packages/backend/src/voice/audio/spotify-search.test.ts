import { beforeEach, describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { resetSpotifyTokenForTests, searchSpotifyTracks, spotifyCredentials } from './spotify-search.js';

const env = { SPOTIFY_CLIENT_ID: 'client', SPOTIFY_CLIENT_SECRET: 'secret' } as NodeJS.ProcessEnv;
const track = (id: string, name: string, artists: string[], ms = 201_000) => ({ id, name, duration_ms: ms, artists: artists.map((n) => ({ name: n })) });

function fakeFetch(tracks: unknown[], tokenStatus = 200) {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const impl = (async (input: string | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, init });
    if (url.startsWith('https://accounts.spotify.com/')) {
      return new Response(tokenStatus === 200 ? JSON.stringify({ access_token: 'token', expires_in: 3600 }) : 'nope', { status: tokenStatus });
    }
    return new Response(JSON.stringify({ tracks: { items: tracks } }), { status: 200 });
  }) as typeof fetch;
  return { impl, calls };
}

describe('Spotify catalogue search', () => {
  beforeEach(() => resetSpotifyTokenForTests());

  it('is off without both credentials', async () => {
    assert.equal(spotifyCredentials({ SPOTIFY_CLIENT_ID: 'x' } as NodeJS.ProcessEnv), null);
    const { impl, calls } = fakeFetch([]);
    assert.deepEqual(await searchSpotifyTracks('song', 5, undefined, { env: {} as NodeJS.ProcessEnv, fetchImpl: impl }), []);
    assert.equal(calls.length, 0);
  });

  it('maps tracks to playable Spotify links and reuses the app token', async () => {
    const { impl, calls } = fakeFetch([
      track('4PTG3Z6ehGkBFwjybzWkR8', 'Kuzu Kuzu', ['Tarkan']),
      track('bad id', 'Skipped', ['Nobody']),
      track('1PTG3Z6ehGkBFwjybzWkR8', 'Duet', ['A', 'B'], 90_500),
    ]);
    const first = await searchSpotifyTracks('tarkan', 5, undefined, { env, fetchImpl: impl });
    assert.deepEqual(first, [
      { id: '4PTG3Z6ehGkBFwjybzWkR8', title: 'Kuzu Kuzu', artist: 'Tarkan', duration: 201, url: 'https://open.spotify.com/track/4PTG3Z6ehGkBFwjybzWkR8' },
      { id: '1PTG3Z6ehGkBFwjybzWkR8', title: 'Duet', artist: 'A, B', duration: 91, url: 'https://open.spotify.com/track/1PTG3Z6ehGkBFwjybzWkR8' },
    ]);
    await searchSpotifyTracks('tarkan', 5, undefined, { env, fetchImpl: impl });
    assert.equal(calls.filter((c) => c.url.startsWith('https://accounts.spotify.com/')).length, 1);
    const search = new URL(calls[1].url);
    assert.equal(search.searchParams.get('type'), 'track');
    assert.equal(search.searchParams.get('market'), 'TR');
  });

  it('reports a failed sign-in without echoing the credentials', async () => {
    const { impl } = fakeFetch([], 400);
    await assert.rejects(
      searchSpotifyTracks('song', 5, undefined, { env, fetchImpl: impl }),
      (error: Error) => /HTTP 400/.test(error.message) && !error.message.includes('secret'),
    );
  });
});
