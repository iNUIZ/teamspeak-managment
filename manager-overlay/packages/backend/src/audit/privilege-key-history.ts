import { createHash, randomUUID } from 'node:crypto';
import type { PrismaClient } from '../../generated/prisma/index.js';
import type { EventBridge } from '../bot-engine/event-bridge.js';

const PREFIX = 'privilege_key_history:';
const MAX_ROWS = 200;
const MAX_AGE = 30 * 86400_000;
export const keyFingerprint = (token: string) => createHash('sha256').update(token).digest('hex');
export interface KeyUseReceipt { id: string; observedAt: string; fingerprint: string; clientId: number; databaseId: number | null; uniqueId: string | null; nickname: string | null }
interface State { enabled: boolean; since: string | null; items: KeyUseReceipt[] }
const empty = (): State => ({ enabled: false, since: null, items: [] });
const positive = (value: string | undefined) => { const n = Number(value); return Number.isSafeInteger(n) && n > 0 ? n : null; };

/** Observed redemption events only. Raw tokens, IPs and event payloads never enter storage. */
export class PrivilegeKeyHistory {
  private pending = Promise.resolve();
  private enabled = new Set<string>();
  private registered = new Set<string>();
  private failures = new Set<string>();
  private queued = 0;
  private retentionTimer: ReturnType<typeof setInterval> | null = null;
  constructor(private prisma: PrismaClient, private bridge: EventBridge) {}
  private connected = (c: number, s: number) => { if (this.enabled.has(`${c}:${s}`)) void this.registerNotifications(c, s); };
  private disconnected = (c: number, s: number) => { this.registered.delete(`${c}:${s}`); };
  private async registerNotifications(c: number, s: number) {
    const pair = `${c}:${s}`;
    try {
      await this.bridge.executeCommand(c, s, 'servernotifyregister event=tokenused');
      if (this.enabled.has(pair)) this.registered.add(pair);
    } catch (error) {
      if (/TS error 516:/.test(String((error as Error).message))) { if (this.enabled.has(pair)) this.registered.add(pair); }
      else this.failures.add(pair);
    }
  }
  private event = (configId: number, sid: number, name: string, data: Record<string, string>) => {
    const pair = `${configId}:${sid}`;
    if (name !== 'notifytokenused' || !this.enabled.has(pair)) return;
    const clientId = positive(data.clid);
    if (!clientId || typeof data.token !== 'string' || data.token.length < 8 || data.token.length > 512) return;
    if (this.queued >= 500) { this.failures.add(pair); return; }
    const cached = this.bridge.clientCache?.lookup(configId, sid, String(clientId));
    const identity = data.cluid && cached?.client_unique_identifier && data.cluid !== cached.client_unique_identifier ? undefined : cached;
    const item: KeyUseReceipt = { id: randomUUID(), observedAt: new Date().toISOString(), fingerprint: keyFingerprint(data.token), clientId,
      databaseId: positive(data.cldbid || identity?.client_database_id), uniqueId: (data.cluid || identity?.client_unique_identifier || '').slice(0, 128) || null,
      nickname: (data.client_nickname || identity?.client_nickname || '').slice(0, 128) || null };
    this.queued++;
    this.pending = this.pending.then(async () => {
      const state = await this.read(configId, sid);
      if (!state.enabled) return;
      const duplicate = state.items.some(row => row.fingerprint === item.fingerprint && row.uniqueId === item.uniqueId && row.databaseId === item.databaseId);
      if (!duplicate) state.items.unshift(item);
      await this.write(configId, sid, state);
    }).catch(() => { this.failures.add(pair); }).finally(() => { this.queued--; });
  };
  async start() {
    this.bridge.on('tsEvent', this.event);
    this.bridge.on('sshConnected', this.connected);
    this.bridge.on('sshDisconnected', this.disconnected);
    const rows = await this.prisma.appSetting.findMany({ where: { key: { startsWith: PREFIX } }, take: 1000 });
    for (const row of rows) {
      const match = /^privilege_key_history:(\d+):(\d+)$/.exec(row.key);
      if (!match) continue;
      const previous = JSON.parse(row.value) as State;
      if (previous.items.some(item => Date.parse(item.observedAt) < Date.now() - MAX_AGE) || previous.items.length > MAX_ROWS) await this.write(Number(match[1]), Number(match[2]), previous);
      try { if (JSON.parse(row.value).enabled) { const c = Number(match[1]); const s = Number(match[2]); this.enabled.add(`${c}:${s}`); await this.bridge.retainSession('key-history', c, s); if (this.bridge.isRegistered(c, s)) await this.registerNotifications(c, s); } } catch { this.failures.add(`${match[1]}:${match[2]}`); }
    }
    this.retentionTimer = setInterval(() => {
      this.pending = this.pending.then(async () => {
        const saved = await this.prisma.appSetting.findMany({ where: { key: { startsWith: PREFIX } }, take: 1000 });
        for (const row of saved) {
          const match = /^privilege_key_history:(\d+):(\d+)$/.exec(row.key); if (!match) continue;
          const state = JSON.parse(row.value) as State;
          if (state.items.some(item => Date.parse(item.observedAt) < Date.now() - MAX_AGE) || state.items.length > MAX_ROWS) await this.write(Number(match[1]), Number(match[2]), state);
        }
      }).catch(() => { for (const pair of this.enabled) this.failures.add(pair); });
    }, 3600_000);
    this.retentionTimer.unref();
  }
  private async read(c: number, s: number): Promise<State> {
    const row = await this.prisma.appSetting.findUnique({ where: { key: `${PREFIX}${c}:${s}` } });
    if (!row) return empty();
    const state = JSON.parse(row.value) as State;
    state.items = state.items.filter(item => Date.parse(item.observedAt) >= Date.now() - MAX_AGE).slice(0, MAX_ROWS);
    return state;
  }
  private async write(c: number, s: number, state: State) {
    state.items = state.items.filter(item => Date.parse(item.observedAt) >= Date.now() - MAX_AGE).slice(0, MAX_ROWS);
    const key = `${PREFIX}${c}:${s}`; const value = JSON.stringify(state);
    await this.prisma.appSetting.upsert({ where: { key }, create: { key, value }, update: { value } });
  }
  async list(c: number, s: number) {
    await this.pending;
    const state = await this.read(c, s); const pair = `${c}:${s}`;
    return { ...state, status: !state.enabled ? 'disabled' : this.failures.has(pair) ? 'gap' : this.registered.has(pair) && this.bridge.isRegistered(c, s) ? 'capturing' : 'connecting', retentionDays: 30, maxRows: MAX_ROWS };
  }
  async setEnabled(c: number, s: number, enabled: boolean) {
    const change = this.pending.then(async () => {
      const state = await this.read(c, s); state.enabled = enabled;
      if (enabled && !state.since) state.since = new Date().toISOString();
      await this.write(c, s, state);
      const pair = `${c}:${s}`;
      if (enabled) { this.enabled.add(pair); const configured = await this.bridge.retainSession('key-history', c, s); if (!configured) this.failures.add(pair); else if (this.bridge.isRegistered(c, s)) await this.registerNotifications(c, s); }
      else { this.enabled.delete(pair); this.registered.delete(pair); await this.bridge.releaseSession('key-history', c, s); }
    });
    this.pending = change.catch(() => {});
    await change;
    return this.list(c, s);
  }
  async stop() { if (this.retentionTimer) clearInterval(this.retentionTimer); this.retentionTimer = null; this.bridge.off('tsEvent', this.event); this.bridge.off('sshConnected', this.connected); this.bridge.off('sshDisconnected', this.disconnected); await this.pending; for (const pair of this.enabled) { const [c, s] = pair.split(':').map(Number); await this.bridge.releaseSession('key-history', c, s); } this.enabled.clear(); this.registered.clear(); }
}
