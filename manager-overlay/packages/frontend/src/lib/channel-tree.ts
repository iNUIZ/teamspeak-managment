export interface TreeChannel { cid: number; pid: number; name: string; depth: number }

/** Channels in display order with their depth, children right after their parent. */
export function flattenChannels(raw: Array<{ cid: number; pid: number; name: string }>): TreeChannel[] {
  const byParent = new Map<number, typeof raw>();
  for (const c of raw) byParent.set(c.pid, [...(byParent.get(c.pid) ?? []), c]);
  const out: TreeChannel[] = [];
  const known = new Set(raw.map((c) => c.cid));
  const walk = (pid: number, depth: number) => {
    for (const c of byParent.get(pid) ?? []) { out.push({ ...c, depth }); walk(c.cid, depth + 1); }
  };
  walk(0, 0);
  // A channel whose parent is missing from the list still shows, at the top level.
  for (const c of raw) if (c.pid !== 0 && !known.has(c.pid) && !out.some((o) => o.cid === c.cid)) out.push({ ...c, depth: 0 });
  return out;
}
