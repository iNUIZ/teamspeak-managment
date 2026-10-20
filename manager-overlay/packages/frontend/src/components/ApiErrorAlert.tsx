import { apiErrorPresentation } from '@/lib/api-error';
import { useTranslation } from '@/lib/i18n';

/** Title + explanation for a structured API error (not a raw Axios sentence). */
export function ApiErrorAlert({ error, fallback }: { error: unknown; fallback: string }) {
  const { t } = useTranslation();
  const p = apiErrorPresentation(error, fallback);
  return (
    <div role="alert" className="space-y-0.5 text-sm text-destructive">
      <p className="font-medium">{t(p.title)}</p>
      {p.message && p.message !== p.title && <p className="text-xs">{t(p.message)}</p>}
      {p.action && p.action !== p.message && <p className="text-xs text-muted-foreground">{t(p.action)}</p>}
    </div>
  );
}
