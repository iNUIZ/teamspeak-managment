import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { bansApi } from '../api/bans.api';
import { useServerStore } from '../stores/server.store';

export function useBans() {
  const { selectedConfigId: c, selectedSid: s } = useServerStore();
  return useQuery({
    queryKey: ['bans', c, s],
    queryFn: () => bansApi.list(c!, s!),
    enabled: !!c && !!s,
  });
}

export function useAddBan() {
  const qc = useQueryClient();
  const { selectedConfigId: c, selectedSid: s } = useServerStore();
  return useMutation({
    mutationFn: (data: any) => bansApi.add(c!, s!, data),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['bans'] }),
  });
}

export function useDeleteBan() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (target: { configId: number; sid: number; banid: number | string }) =>
      bansApi.delete(target.configId, target.sid, Number(target.banid)),
    onSuccess: async (_result, target) => {
      const queryKey = ['bans', target.configId, target.sid];
      await qc.cancelQueries({ queryKey, exact: true });
      // A confirmed deletion must not wait for (or be undone by) an old list request.
      qc.setQueryData(queryKey, (rows: any) => Array.isArray(rows)
        ? rows.filter(row => String(row.banid) !== String(target.banid)) : rows);
      await qc.invalidateQueries({ queryKey, exact: true, refetchType: 'none' });
    },
  });
}
