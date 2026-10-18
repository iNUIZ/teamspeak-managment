import { createWebQueryClient } from '../ts-client/webquery-client.js';

/** Called only by the local root installer; no unauthenticated HTTP endpoint. */
export async function serverAccessSummary(host: string, apiKey: string, uid?: string): Promise<string[]> {
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,252}$/.test(host) || !apiKey) throw new Error('Invalid server connection');
  if (uid && !/^[A-Za-z0-9+/=_-]{20,128}$/.test(uid)) throw new Error('Invalid TeamSpeak identity UID');
  const query = createWebQueryClient(host, 10080, apiKey, false);
  const list = (value: any) => Array.isArray(value) ? value : value ? [value] : [];
  try {
    const groups = list(await query.execute(1, 'servergrouplist'));
    const admin = groups.find(g => Number(g.type) === 1 && /^server[\s_-]*admin$/i.test(g.name));
    if (!admin) return ['Server Admin grubu otomatik tanınamadı; yetki anahtarlarını panelden yönetin.'];
    let tokens: any[];
    try { tokens = list(await query.execute(1, 'tokenlist')); }
    catch (error: any) { if (error.code === 1281) tokens = []; else throw error; }
    let token = tokens.find(t => Number(t.token_type) === 0 && Number(t.token_id1) === Number(admin.sgid))?.token;
    if (!token) {
      const created = list(await query.execute(1, 'tokenadd', { tokentype: 0, tokenid1: admin.sgid, tokenid2: 0, tokendescription: 'Teamspeak Managment installer' }));
      token = created[0]?.token;
    }
    const lines = [token ? `Server Admin privilege key: ${token}` : 'Privilege key alınamadı; panelde Yetki anahtarları bölümünü kontrol edin.'];
    if (uid) {
      try {
        const found = list(await query.execute(1, 'clientgetdbidfromuid', { cluid: uid }));
        const cldbid = Number(found[0]?.cldbid);
        if (!Number.isInteger(cldbid) || cldbid < 1) throw new Error('Unknown identity');
        const memberships = list(await query.execute(1, 'servergroupsbyclientid', { cldbid }));
        if (!memberships.some(g => Number(g.sgid) === Number(admin.sgid))) {
          await query.execute(1, 'servergroupaddclient', { sgid: admin.sgid, cldbid });
        }
        lines.push(`UID için Server Admin yetkisi hazır: ${uid}`);
      } catch { lines.push('UID sunucuda bulunamadı veya yetki verilemedi. Önce TS ile bağlanın ya da yukarıdaki privilege key kullanın.'); }
    }
    lines.push('TS kimliği ServerQuery oturumu değildir; Query erişimi serveradmin kimlik bilgileri/API anahtarı kullanır.');
    return lines;
  } finally { query.destroy(); }
}
