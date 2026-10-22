import { translate as tr, useTranslation as useUiTranslation } from '@/lib/i18n';
import { Copy, RefreshCw, Stethoscope } from 'lucide-react';
import { copyText } from '@/lib/clipboard';
import { toast } from 'sonner';
import type { MediaDiagnosticCheck, MediaDiagnosticStatus } from '@ts6/common';
import { PageHeader } from '@/components/shared/PageHeader';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useMediaAdminDiagnostics } from '@/hooks/use-media-admin-diagnostics';
import {
  formatMediaDiagnosticReport,
  mediaDiagnosticOverallWord,
  mediaDiagnosticStatusWord,
} from '@/lib/media-diagnostics-report';

function badgeVariant(status: MediaDiagnosticStatus): 'success' | 'warning' | 'destructive' | 'outline' {
  if (status === 'ok') return 'success';
  if (status === 'warning') return 'warning';
  if (status === 'fail') return 'destructive';
  return 'outline';
}

function CheckRow({ check }: { readonly check: MediaDiagnosticCheck }) {
  useUiTranslation();
  return (
    <li className="rounded-md border border-border px-3 py-2" data-status={check.status}>
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant={badgeVariant(check.status)}>{tr(mediaDiagnosticStatusWord(check.status))}</Badge>
        <span className="font-medium">{tr(check.label)}</span>
      </div>
      <p className="mt-1 text-sm text-muted-foreground">{tr(check.status === 'ok' ? 'Tool check passed' : check.status === 'fail' ? 'Tool check failed' : check.status === 'warning' ? 'Tool check needs attention' : 'Tool check was skipped')}</p>
      <details className="mt-1 text-xs text-muted-foreground"><summary>{tr('Technical output (original language)')}</summary>
      {check.version && <p>{check.version}</p>}
      <p>{check.message}</p>
      {check.path && <p className="mt-1 break-all text-xs text-muted-foreground"> {tr("Executable:")}{' '}{check.path}</p>}
      {check.remediation && (
        <p className="mt-1 text-sm text-amber-700 dark:text-amber-400">{check.remediation}</p>
      )}
      </details>
    </li>
  );
}

function TechnicalDetails({ children }: { children: React.ReactNode }) {
  return <details className="text-xs text-muted-foreground"><summary>{tr('Technical output (original language)')}</summary><div className="mt-2 space-y-2 break-words">{children}</div></details>;
}

export default function MediaDiagnostics() {
  useUiTranslation();
  const { report, isFetching, isError, invalidated, notChecked, offlineUnchecked, refresh } = useMediaAdminDiagnostics();

  let statusMessage = '';
  if (isFetching) statusMessage = tr('Checking media tools…');
  else if (report) statusMessage = `${tr('Last checked:')} ${new Date(report.checkedAt).toLocaleString()}`;

  const copyReport = async () => {
    if (!report) return;
    try {
      await copyText(formatMediaDiagnosticReport(report));
      toast.success(tr("Diagnostic report copied"));
    } catch {
      toast.error(tr("Could not copy the diagnostic report"));
    }
  };

  return (
    <div className="space-y-4" data-testid="media-diagnostics">
      <PageHeader
        title={tr("Media diagnostics")}
        icon={Stethoscope}
        description={tr("Check media tools, audio encoding, and sidecar health.")}
        badge={report && (
          <Badge variant={badgeVariant(report.overall === 'partial' ? 'warning' : report.overall)}>
            {tr(mediaDiagnosticOverallWord(report.overall))}
          </Badge>
        )}
        actions={(
          <>
            <Button type="button" variant="outline" size="sm" onClick={() => void copyReport()} disabled={!report || isFetching}>
              <Copy className="mr-1.5 h-3.5 w-3.5" /> {tr("Copy diagnostic report")}{' '}</Button>
            <Button type="button" variant="outline" size="sm" onClick={() => refresh()} disabled={isFetching}>
              <RefreshCw className={`mr-1.5 h-3.5 w-3.5 ${isFetching ? 'animate-spin' : ''}`} /> {tr("Refresh")}{' '}</Button>
          </>
        )}
      />

      <output className="block text-sm text-muted-foreground">{statusMessage}</output>
      <p className="text-sm text-muted-foreground">{tr('Diagnostics run on demand, not as a historical log. Available tools do not guarantee YouTube playback; provider authentication and network restrictions are checked during playback.')}</p>

      {(notChecked || offlineUnchecked) && !report && (
        <p className="text-sm text-muted-foreground"> {tr("Not checked. Use Refresh when this browser is online.")}{' '}</p>
      )}
      {isError && !isFetching && (
        <p className="text-sm text-destructive"> {tr("Could not run media diagnostics. Use Refresh to retry.")}{' '}</p>
      )}
      {invalidated && report && (
        <p className="text-sm text-amber-700 dark:text-amber-400"> {tr("This report is stale. Use Refresh to probe again.")}{' '}</p>
      )}

      {report && (
        <>
          <Card>
            <CardHeader>
              <CardTitle> {tr("Tools and smoke tests")}{' '}</CardTitle>
            </CardHeader>
            <CardContent>
              <ul className="space-y-2">
                {report.checks.map((check) => <CheckRow key={check.id} check={check} />)}
              </ul>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle> {tr("Sidecar")}{' '}</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2 text-sm">
              <Badge variant={badgeVariant(report.sidecar.status)}>{tr(mediaDiagnosticStatusWord(report.sidecar.status))}</Badge>
              <TechnicalDetails><p>{report.sidecar.message}</p></TechnicalDetails>
              <dl className="grid gap-1 text-muted-foreground sm:grid-cols-2">
                <div> {tr("Mode:")}{' '}{report.sidecar.mode}</div>
                <div> {tr("Version:")}{' '}{report.sidecar.version ?? tr('not reported by GET /health')}</div>
                <div> {tr("Health:")}{' '}{tr(report.sidecar.healthStatus ?? 'not reported')}</div>
                <div> {tr("Ports:")}{' '}{report.sidecar.videoPort ?? '—'} {tr("video /")}{' '}{report.sidecar.audioPort ?? '—'} {tr("audio")}{' '}</div>
              </dl>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle> {tr("Encoders")}{' '}</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              <Badge variant={badgeVariant(report.encoders.status)}>{tr(mediaDiagnosticStatusWord(report.encoders.status))}</Badge>
              <TechnicalDetails>{report.encoders.message}</TechnicalDetails>
              {report.encoders.items.length > 0 && (
                <ul className="space-y-1 text-sm">
                  {report.encoders.items.map((item) => {
                    let status: MediaDiagnosticStatus = 'fail';
                    if (item.available) status = 'ok';
                    else if (item.hardware) status = 'warning';
                    return (
                      <li key={item.id} className="flex flex-wrap items-baseline gap-2">
                        <Badge variant={badgeVariant(status)}>
                          {tr(mediaDiagnosticStatusWord(status))}
                        </Badge>
                        <span className="font-medium">{item.id}</span>
                        <div className="text-muted-foreground">
                          {item.codec} · {item.hardware ? tr("hardware") : tr("software")}
                          {item.detail && <TechnicalDetails>{item.detail}</TechnicalDetails>}
                        </div>
                      </li>
                    );
                  })}
                </ul>
              )}
              {report.encoders.av1Decode && (
                <div className="space-y-1 pt-2 text-sm" role="status" aria-live="polite" aria-label={tr("AV1 decoding")}>
                  <p className="font-medium"> {tr("AV1 decoding")}{' '}</p>
                  <TechnicalDetails>{report.encoders.av1Decode.message}</TechnicalDetails>
                  {report.encoders.av1Decode.items.length > 0 && (
                    <ul className="space-y-1">
                      {report.encoders.av1Decode.items.map((item) => (
                        <li key={item.hwaccel} className="flex flex-wrap items-baseline gap-2">
                          <Badge variant={badgeVariant(item.available ? 'ok' : 'skipped')}>
                            {item.available ? tr("Decodes") : tr("No")}
                          </Badge>
                          <span className="font-medium">{item.hwaccel}</span>
                          {item.detail && <TechnicalDetails>{item.detail}</TechnicalDetails>}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle> {tr("Warnings and failures")}{' '}</CardTitle>
            </CardHeader>
            <CardContent>
              {report.warnings.length === 0 ? (
                <p className="text-sm text-muted-foreground"> {tr("None.")}{' '}</p>
              ) : (
                <ul className="list-disc space-y-1 pl-5 text-sm">
                  {report.warnings.map((warning) => <li key={warning}><TechnicalDetails>{warning}</TechnicalDetails></li>)}
                </ul>
              )}
            </CardContent>
          </Card>
        </>
      )}
    </div>
  );
}
