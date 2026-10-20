import type { PrismaClient } from '../../generated/prisma/index.js';
import type { RadioPreset } from '@ts6/common';

/** Publisher endpoints checked 2026-10-19; availability/region rules can change. */
export const DEFAULT_RADIO_STATIONS: RadioPreset[] = [
  { name: 'Kral FM', url: 'https://dygedge2.radyotvonline.net/kralfm/playlist.m3u8', genre: 'TR · Arabesk' },
  { name: 'Kral Pop', url: 'https://dygedge.radyotvonline.net/kralpop/playlist.m3u8', genre: 'TR · Pop' },
  { name: 'PowerTürk', url: 'https://listen.powerapp.com.tr/powerturk/mpeg/icecast.audio', genre: 'TR · Pop' },
  { name: 'SlowTürk', url: 'https://radyo.duhnet.tv/slowturk', genre: 'TR · Slow' },
  { name: 'Radyo D', url: 'https://radyo.duhnet.tv/radyod', genre: 'TR · Pop' },
  { name: 'Heart UK', url: 'https://media-ice.musicradio.com/HeartUKMP3', genre: 'EN · Pop' },
  { name: 'Capital UK', url: 'https://media-ice.musicradio.com/CapitalUKMP3', genre: 'EN · Hits' },
  { name: 'Smooth UK', url: 'https://media-ice.musicradio.com/SmoothUKMP3', genre: 'EN · Easy listening' },
  { name: 'Classic FM', url: 'https://media-ice.musicradio.com/ClassicFMMP3', genre: 'EN · Classical' },
  { name: 'LBC UK', url: 'https://media-ice.musicradio.com/LBCUKMP3', genre: 'EN · Talk' },
];

/** One-time per connection: preserve custom stations and deliberate deletions. */
export async function seedRadioDefaults(prisma: PrismaClient, serverConfigId: number): Promise<void> {
  await prisma.$transaction(async tx => {
    const key = `radio-defaults-v1:${serverConfigId}`;
    if (await tx.appSetting.findUnique({ where: { key } })) return;
    for (const station of DEFAULT_RADIO_STATIONS) {
      if (!await tx.radioStation.findFirst({ where: { serverConfigId, url: station.url } })) {
        await tx.radioStation.create({ data: { ...station, serverConfigId } });
      }
    }
    await tx.appSetting.create({ data: { key, value: 'seeded' } });
  });
}
