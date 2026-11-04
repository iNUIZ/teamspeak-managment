import { Link } from 'react-router-dom';
import {
  Ban, Bot, FolderOpen, KeyRound, Lock, Mail, MessageSquareWarning, Plug, ScrollText, Server, Shield, ShieldCheck, Users,
  type LucideIcon,
} from 'lucide-react';
import { useTranslation } from '@/lib/i18n';

interface Entry { to: string; icon: LucideIcon; title: string; hint: string }

/**
 * Everything that configures something, in one place. The TeamSpeak server picked at the
 * top of the page applies to all of it, so no page asks for a server again.
 */
const GROUPS: Array<{ title: string; hint: string; entries: Entry[] }> = [
  {
    title: 'People and access',
    hint: 'Who may do what on the selected TeamSpeak server.',
    entries: [
      { to: '/server-groups', icon: Shield, title: 'Server Groups', hint: 'Ranks such as Admin or Guest' },
      { to: '/channel-groups', icon: ShieldCheck, title: 'Channel Groups', hint: 'Roles inside a channel' },
      { to: '/permissions', icon: Lock, title: 'Permissions', hint: 'Fine-grained rights' },
      { to: '/bans', icon: Ban, title: 'Bans', hint: 'Banned people and addresses' },
      { to: '/tokens', icon: KeyRound, title: 'Tokens', hint: 'Keys that grant a group' },
      { to: '/clients', icon: Users, title: 'Clients', hint: 'Everyone who has ever joined' },
    ],
  },
  {
    title: 'Server content',
    hint: 'Files and messages stored on the selected TeamSpeak server.',
    entries: [
      { to: '/files', icon: FolderOpen, title: 'Files', hint: 'Channel file browser' },
      { to: '/messages', icon: Mail, title: 'Messages', hint: 'Offline messages' },
      { to: '/complaints', icon: MessageSquareWarning, title: 'Complaints', hint: 'Reports about users' },
    ],
  },
  {
    title: 'Panel',
    hint: 'How this panel reaches TeamSpeak, and what it records.',
    entries: [
      { to: '/settings?tab=connections', icon: Plug, title: 'Connections', hint: 'Add or fix a TeamSpeak connection' },
      { to: '/servers', icon: Server, title: 'Virtual Servers', hint: 'Start, stop and create servers' },
      { to: '/bots', icon: Bot, title: 'Bot Flows', hint: 'Automatic actions on events' },
      { to: '/reports', icon: ScrollText, title: 'Reports and diagnostics', hint: 'Logs, audit and health' },
    ],
  },
];

export function SettingsOverview() {
  const { t } = useTranslation();
  return (
    <div className="space-y-6">
      {GROUPS.map((group) => (
        <section key={group.title} aria-label={t(group.title)}>
          <h2 className="text-sm font-semibold">{t(group.title)}</h2>
          <p className="mb-2 text-xs text-muted-foreground">{t(group.hint)}</p>
          <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
            {group.entries.map((entry) => (
              <Link
                key={entry.to} to={entry.to}
                className="flex items-center gap-3 rounded-md border p-3 transition-colors hover:border-primary/60 hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
                  <entry.icon className="h-4 w-4" aria-hidden="true" />
                </span>
                <span className="min-w-0">
                  <span className="block truncate text-sm font-medium">{t(entry.title)}</span>
                  <span className="block truncate text-xs text-muted-foreground">{t(entry.hint)}</span>
                </span>
              </Link>
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
