import path from 'node:path';
import { existsSync } from 'node:fs';

export const YOUTUBE_AUTH_MODE_KEY = 'youtube_auth_mode';
export type YouTubeAuthMode = 'po-token' | 'cookies';

/** Only an administrator's deployment environment can choose the trusted plugin directory.
 * Never accept plugin paths, provider URLs or tokens from a playback request. */
export function poTokenArgs(env: NodeJS.ProcessEnv = process.env): string[] {
  const pluginDir = env.YOUTUBE_POT_PLUGIN_DIR;
  if (!pluginDir) return [];
  if (!path.isAbsolute(pluginDir) || !existsSync(pluginDir)) {
    throw new Error('YOUTUBE_POT_PLUGIN_DIR must be an existing absolute trusted plugin directory');
  }
  return ['--plugin-dirs', pluginDir, '--extractor-args',
    `youtubepot-bgutilhttp:base_url=${poTokenBaseUrl(env)}`];
}

export function poTokenBaseUrl(env: NodeJS.ProcessEnv = process.env): string {
  const raw = env.YOUTUBE_POT_URL || 'http://127.0.0.1:4416';
  const url = new URL(raw);
  if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || url.username || url.password ||
      url.pathname !== '/' || url.search || url.hash) {
    throw new Error('YOUTUBE_POT_URL must be an http://127.0.0.1:port loopback origin');
  }
  return url.origin;
}

/** Bounded local readiness probe. Does not generate, return or persist a token. */
export async function poTokenStatus(env: NodeJS.ProcessEnv = process.env): Promise<{
  configured: boolean; ready: boolean; version: string | null;
}> {
  try {
    if (!poTokenArgs(env).length) return { configured: false, ready: false, version: null };
    const response = await fetch(`${poTokenBaseUrl(env)}/ping`, {
      signal: AbortSignal.timeout(1500), redirect: 'error',
    });
    if (!response.ok) throw new Error('provider unavailable');
    const body = await response.json() as { version?: unknown };
    const version = typeof body.version === 'string' && /^\d+\.\d+\.\d+$/.test(body.version)
      ? body.version : null;
    return { configured: true, ready: version === '2.0.2', version };
  } catch {
    return { configured: !!env.YOUTUBE_POT_PLUGIN_DIR, ready: false, version: null };
  }
}
