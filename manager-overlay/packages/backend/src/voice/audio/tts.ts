import { spawn } from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import type { PrismaClient } from '../../../generated/prisma/index.js';

export const TTS_VOICES = { tr: 'tr_TR-dfki-medium', en: 'en_US-lessac-medium' } as const;
export type TtsSettings = { enabled: boolean; language: 'tr' | 'en'; maxCharacters: number; cooldownSeconds: number; volume: number };
export function parseTtsSettings(value: unknown): TtsSettings {
  const v = value as Partial<TtsSettings> | null;
  if (!v || typeof v.enabled !== 'boolean' || !['tr', 'en'].includes(v.language as string)
    || !Number.isInteger(v.maxCharacters) || v.maxCharacters! < 20 || v.maxCharacters! > 500
    || !Number.isInteger(v.cooldownSeconds) || v.cooldownSeconds! < 5 || v.cooldownSeconds! > 120
    || typeof v.volume !== 'number' || !Number.isFinite(v.volume) || v.volume < 0.1 || v.volume > 1.5) {
    throw new Error('Invalid TTS settings');
  }
  return { enabled: v.enabled, language: v.language!, maxCharacters: v.maxCharacters!, cooldownSeconds: v.cooldownSeconds!, volume: v.volume };
}
export async function ttsInstalled(): Promise<boolean> {
  return fs.access('/opt/teamhub-tts/bin/python').then(() => true, () => false);
}
export function botTtsSettingsKey(botId: number): string {
  if (!Number.isSafeInteger(botId) || botId <= 0) throw new Error('Invalid bot ID');
  return `tts_settings:bot:${botId}`;
}
export async function loadTtsSettings(prisma: PrismaClient, botId?: number): Promise<TtsSettings> {
  if (botId !== undefined) {
    const scoped = await prisma.appSetting.findUnique({ where: { key: botTtsSettingsKey(botId) } });
    if (scoped) return parseTtsSettings(JSON.parse(scoped.value));
  }
  // Read-only legacy defaults preserve existing behaviour until each bot is saved.
  const row = await prisma.appSetting.findUnique({ where: { key: 'tts_settings' } });
  if (row) return parseTtsSettings(JSON.parse(row.value));
  return { enabled: await ttsInstalled(), language: 'tr', maxCharacters: 250, cooldownSeconds: 10, volume: 1 };
}
export function parseSpeech(args: string, settings: TtsSettings): { language: 'tr' | 'en'; text: string } {
  const match = args.match(/^--(tr|en)\s+/i);
  const language = match ? match[1].toLowerCase() as 'tr' | 'en' : settings.language;
  const text = (match ? args.slice(match[0].length) : args).replace(/[\u0000-\u001f\u007f]/g, ' ').replace(/\[\[|\]\]/g, '').trim();
  if (!text || text.length > settings.maxCharacters) throw new Error(`!say [--tr|--en] <text> (1–${settings.maxCharacters} characters)`);
  return { language, text };
}
let active = 0;
const lastSpoken = new Map<number, number>();
export async function synthesizeSpeech(botId: number, args: string, settings: TtsSettings): Promise<{ file: string; cleanup: () => Promise<void> }> {
  if (!settings.enabled || !await ttsInstalled()) throw new Error('TTS is disabled or not installed. Enable the optional TTS package in the installer.');
  const speech = parseSpeech(args, settings);
  if (active >= 2 || Date.now() - (lastSpoken.get(botId) ?? 0) < settings.cooldownSeconds * 1000) throw new Error('TTS is busy. Please wait before speaking again.');
  lastSpoken.set(botId, Date.now());
  if (lastSpoken.size > 200) for (const [id, at] of lastSpoken) { if (Date.now() - at > 120000) lastSpoken.delete(id); }
  active++;
  let folder: string;
  try { folder = await fs.mkdtemp(path.join(os.tmpdir(), 'teamhub-tts-')); }
  catch (error) { active--; throw error; }
  const file = path.join(folder, 'speech.wav');
  const cleanup = () => fs.rm(folder, { recursive: true, force: true });
  try {
    await new Promise<void>((resolve, reject) => {
      const child = spawn('/opt/teamhub-tts/bin/python', ['-m', 'piper', '-m', `/opt/teamhub-voices/${TTS_VOICES[speech.language]}.onnx`, '-f', file, '--volume', String(settings.volume)], {
        stdio: ['pipe', 'ignore', 'pipe'], windowsHide: true, env: { ...process.env, OMP_NUM_THREADS: '1' },
      });
      const timeout = setTimeout(() => { child.kill('SIGKILL'); reject(new Error('TTS synthesis timed out')); }, 45000);
      child.stderr.on('data', () => {});
      child.stdin.on('error', () => {});
      child.on('error', () => { clearTimeout(timeout); reject(new Error('TTS runtime is unavailable')); });
      child.on('close', (code) => { clearTimeout(timeout); code === 0 ? resolve() : reject(new Error('TTS could not synthesize this text')); });
      child.stdin.end(speech.text);
    });
    const stat = await fs.stat(file);
    if (stat.size < 44 || stat.size > 8 * 1024 * 1024) throw new Error('TTS output exceeded the audio limit');
    const expiry = setTimeout(() => { void cleanup(); }, 300000);
    expiry.unref();
    return { file, cleanup };
  } catch (error) { await cleanup(); throw error; }
  finally { active--; }
}
