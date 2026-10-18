import { translate as tr, useTranslation as useUiTranslation } from '@/lib/i18n';
import { useMemo } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { tokensApi } from '@/api/bans.api';
import { useServerStore } from '@/stores/server.store';
import { DataTable } from '@/components/shared/DataTable';
import { Button } from '@/components/ui/button';
import { PageLoader } from '@/components/shared/LoadingSpinner';
import { NoServerSelectedState } from '@/components/shared/NoServerSelectedState';
import { KeyRound, Trash2, Copy } from 'lucide-react';
import { type ColumnDef } from '@tanstack/react-table';
import { toast } from 'sonner';
import { copyText } from '@/lib/clipboard';
import { useTranslation } from '@/lib/i18n';

export default function Tokens() {
  useUiTranslation();
  const { t, language } = useTranslation();
  const { selectedConfigId: c, selectedSid: s } = useServerStore();
  const { data, isLoading } = useQuery({ queryKey: ['tokens', c, s], queryFn: () => tokensApi.list(c!, s!), enabled: !!c && !!s });
  const qc = useQueryClient();
  const deleteToken = useMutation({ mutationFn: (token: string) => tokensApi.delete(c!, s!, token), onSuccess: () => qc.invalidateQueries({ queryKey: ['tokens'] }) });

  const tokens = useMemo(() => (Array.isArray(data) ? data : []), [data]);

  const columns: ColumnDef<any>[] = useMemo(() => [
    { accessorKey: 'token', header: t('Token'), cell: ({ getValue }) => (
      <div className="flex items-center gap-1">
        <span className="font-mono-data text-xs truncate max-w-[200px]">{getValue() as string}</span>
        <Button
          variant="ghost"
          size="icon"
          aria-label={t('Copy privilege key')}
          onClick={() => { void copyText(getValue() as string).then(() => toast.success(t('Copied')), () => toast.error(t('Could not copy. Select the text and copy it by hand.'))); }}
        >
          <Copy className="h-3 w-3 text-muted-foreground" />
        </Button>
      </div>
    )},
    { accessorKey: 'token_type', header: t('Type'), cell: ({ getValue }) => <span className="text-xs">{(getValue() as number) === 0 ? tr("Server Group") : tr("Channel Group")}</span> },
    { accessorKey: 'token_id1', header: t('Group ID'), cell: ({ getValue }) => <span className="font-mono-data text-xs">{getValue() as number}</span> },
    { accessorKey: 'token_description', header: t('Description'), cell: ({ getValue }) => <span className="text-xs">{(getValue() as string) || '-'}</span> },
    { id: 'actions', header: () => <span className="sr-only"> {tr("Actions")}</span>, cell: ({ row }) => (
      <Button variant="ghost" size="icon" className="text-destructive hover:text-destructive" aria-label={tr("Delete privilege key")} onClick={() => deleteToken.mutate(row.original.token, { onSuccess: () => toast.success('Token deleted') })}>
        <Trash2 className="h-3.5 w-3.5" />
      </Button>
    )},
  ], [deleteToken.mutate, language]);

  if (!c || !s) return <NoServerSelectedState pageTitle={tr("Privilege Keys")} icon={KeyRound} />;
  if (isLoading) return <PageLoader />;

  return (
    <div className="space-y-5">
      <h1 className="text-xl font-semibold"> {tr("Privilege Keys")}</h1>
      <DataTable
        columns={columns}
        data={tokens}
        searchEnabled
        searchLabel={tr("Search privilege keys")}
        searchPlaceholder={tr("Search tokens...")}
        tableLabel={tr("Privilege keys table")}
        density="compact"
        stickyHeader
        emptyText={tr("No privilege keys found")}
        filteredEmptyText={tr("No privilege keys match your search")}
      />
    </div>
  );
}
