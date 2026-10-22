import fs from 'node:fs';

// Loaded at startup or after an administrator uploads/removes a cookie file.
// Legacy authentication-mode settings deliberately have no effect.
let cookiePath: string | null = null;

export function setYtCookieFile(filePath: string | null): void { cookiePath = filePath; }

export function getYtCookieFile(): string | null {
  if (!cookiePath) return null;
  try {
    const stat = fs.statSync(cookiePath);
    fs.accessSync(cookiePath, fs.constants.R_OK);
    return stat.isFile() && stat.size > 0 ? cookiePath : null;
  } catch { return null; }
}

/** Do not silently fall back to anonymous playback or an external provider. */
export function requireYouTubeCookies(target: string, args: readonly string[]): void {
  let youtube = /^ytsearch\d*:/.test(target);
  try {
    const host = new URL(target).hostname.toLowerCase();
    youtube ||= host === 'youtu.be' || host === 'youtube.com' || host.endsWith('.youtube.com');
  } catch { /* Search expressions are not URLs. */ }
  if (!youtube) return;
  const file = getYtCookieFile();
  const at = args.indexOf('--cookies');
  if (!file || at < 0 || args[at + 1] !== file) {
    throw new Error('YouTube cookies required. Upload a cookies.txt file in Settings → YouTube before searching or playing YouTube media.');
  }
}
