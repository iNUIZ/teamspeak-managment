import https from 'node:https';
import { lookup } from 'node:dns';
import { mediaEgressRefusal } from './egress/egress-policy.js';
import { parseYouTubeUrl } from './audio/youtube.js';
import type { QueueItem } from './playlist/queue.js';

export function trackDescription(item: QueueItem | null): string {
  if (!item) return '';
  const clean = (value: string) => value.replace(/[\[\]\x00-\x1f\x7f]/g, ' ').replace(/\s+/g, ' ').trim();
  const title = clean(item.title).slice(0, 300);
  const artist = clean(item.artist || '').slice(0, 120);
  const duration = Number.isFinite(item.duration) && item.duration! > 0
    ? `${Math.floor(item.duration! / 60)}:${String(Math.floor(item.duration! % 60)).padStart(2, '0')}` : '';
  const description = [title, artist, duration].filter(Boolean).join(' • ');
  let bounded = '';
  for (const character of description) {
    if (Buffer.byteLength(bounded + character, 'utf8') > 200) break;
    bounded += character;
  }
  return bounded;
}

/** Never fetch arbitrary metadata URLs: only a fixed YouTube CDN path. */
export function trackCoverUrl(item: QueueItem | null): string | null {
  if (!item?.sourceUrl || item.source !== 'youtube') return null;
  const id = parseYouTubeUrl(item.sourceUrl).videoId;
  return id && /^[\w-]{11}$/.test(id) ? `https://i.ytimg.com/vi/${id}/mqdefault.jpg` : null;
}

export function fetchTrackCover(url: string, signal: AbortSignal): Promise<Buffer> {
  if (!/^https:\/\/i\.ytimg\.com\/vi\/[\w-]{11}\/mqdefault\.jpg$/.test(url)) {
    return Promise.reject(new Error('Unsupported track cover'));
  }
  return new Promise((resolve, reject) => {
    const request = https.get(url, {
      signal: AbortSignal.any([signal, AbortSignal.timeout(8_000)]),
      // Validate every resolved address, then connect to the vetted address.
      lookup(host, options, callback) {
        lookup(host, { all: true }, (error, addresses) => {
          if (error) return callback(error, '', 4);
          if (!addresses.length || addresses.some((address) => mediaEgressRefusal(address.address))) {
            return callback(new Error('Track cover address refused'), '', 4);
          }
          if (options.all) callback(null, addresses);
          else callback(null, addresses[0].address, addresses[0].family);
        });
      },
    }, response => {
      if (response.statusCode !== 200 || !/^image\/jpeg(?:;|$)/i.test(String(response.headers['content-type']))) {
        response.resume(); reject(new Error('Track cover unavailable')); return;
      }
      let size = 0; const chunks: Buffer[] = [];
      response.on('data', (chunk: Buffer) => {
        size += chunk.length;
        if (size > 200 * 1024) request.destroy(new Error('Track cover too large'));
        else chunks.push(chunk);
      });
      response.on('end', () => resolve(Buffer.concat(chunks)));
      response.on('error', reject);
    });
    request.setTimeout(8_000, () => request.destroy(new Error('Track cover timed out')));
    request.on('error', reject);
  });
}
