import { translate as uiText, useLanguageStore as useUiLanguageStore } from '@/lib/i18n';
import { useEffect, useRef, useState } from 'react';
import { Link, NavLink, useLocation } from 'react-router-dom';
import {
  LayoutDashboard, Server, Hash, Users, Shield, ShieldCheck,
  Lock, Ban, KeyRound, FolderOpen, MessageSquareWarning, Mail,
  ScrollText, Settings, Bot, ChevronDown, ChevronLeft, ChevronRight, Library, Github, BookOpen, Menu,
  Music2,

  type LucideIcon,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { useUiStore } from '@/stores/ui.store';
import { useAuthStore } from '@/stores/auth.store';
import { useScrollEdges } from '@/hooks/use-scroll-edges';
import { Tooltip, TooltipContent, TooltipTrigger, TooltipProvider } from '@/components/ui/tooltip';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import { Sheet, SheetContent, SheetDescription, SheetTitle, SheetTrigger } from '@/components/ui/sheet';
import { BrandMark } from '@/components/shared/BrandMark';
import { APP_DOCUMENTATION_URL, APP_REPOSITORY_URL, APP_VERSION, APP_VERSION_LABEL } from '@/lib/app-version';
import { useTranslation } from '@/lib/i18n';
import { Input } from '@/components/ui/input';

export interface NavItem {
  to: string;
  icon: LucideIcon;
  label: string;
  adminOnly?: boolean;
  /** Extra words the menu search matches, in both languages. */
  keywords?: string[];
}

/** The whole menu: what people come to do. Everything that configures something lives under Settings. */
const mainNav: NavItem[] = [
  { to: '/control', icon: Music2, label: 'Music bots', adminOnly: true, keywords: ['bot', 'müzik', 'music', 'radio', 'radyo'] },
  { to: '/server-control', icon: Users, label: 'Server', adminOnly: true, keywords: ['kick', 'ban', 'poke', 'move', 'taşı', 'dürt', 'people', 'kişiler'] },
  { to: '/channels', icon: Hash, label: 'Channels', keywords: ['kanal'] },
  { to: '/media-bots', icon: Library, label: 'Library', adminOnly: true, keywords: ['playlist', 'liste', 'iptv', 'tv', 'youtube', 'cookie', 'çerez', 'radio', 'radyo'] },
  { to: '/dashboard', icon: LayoutDashboard, label: 'Status', keywords: ['dashboard', 'pano', 'kontrol paneli'] },
];

/**
 * Pages opened from the Settings overview. They are not in the menu, but the menu search
 * finds them and Settings stays highlighted while one is open.
 */
export const settingsPages: NavItem[] = [
  { to: '/server-groups', icon: Shield, label: 'Server Groups', adminOnly: true },
  { to: '/channel-groups', icon: ShieldCheck, label: 'Channel Groups', adminOnly: true },
  { to: '/permissions', icon: Lock, label: 'Permissions', adminOnly: true },
  { to: '/bans', icon: Ban, label: 'Bans', adminOnly: true },
  { to: '/tokens', icon: KeyRound, label: 'Tokens', adminOnly: true },
  { to: '/clients', icon: Users, label: 'Clients' },
  { to: '/files', icon: FolderOpen, label: 'Files', adminOnly: true },
  { to: '/messages', icon: Mail, label: 'Messages', adminOnly: true },
  { to: '/complaints', icon: MessageSquareWarning, label: 'Complaints', adminOnly: true },
  { to: '/servers', icon: Server, label: 'Virtual Servers', adminOnly: true, keywords: ['teamspeak', 'connection', 'bağlantı'] },
  { to: '/reports', icon: ScrollText, label: 'Reports and diagnostics', adminOnly: true, keywords: ['logs', 'audit', 'activity', 'diagnostics', 'kayıt', 'günlük', 'etkinlik', 'tanılama'] },
  { to: '/bots', icon: Bot, label: 'Bot Flows', adminOnly: true, keywords: ['flow', 'akış', 'otomasyon', 'automation'] },
];

const isAt = (pathname: string, to: string) => pathname === to || pathname.startsWith(to + '/');

/** The Settings page a settings sub-page belongs to, for its back link. */
export function settingsPageFor(pathname: string): NavItem | undefined {
  return settingsPages.find((item) => isAt(pathname, item.to));
}

interface NavigationContentProps {
  collapsed?: boolean;
  mobile?: boolean;
  onNavigate?: () => void;
}

function NavigationContent({ collapsed = false, mobile = false, onNavigate }: NavigationContentProps) {
  useUiLanguageStore(state => state.language);
  const { t } = useTranslation();
  const [menuSearch, setMenuSearch] = useState('');
  const query = collapsed ? '' : menuSearch.trim().toLocaleLowerCase('tr-TR');
  const matches = (item: NavItem) => !query || t(item.label).toLocaleLowerCase('tr-TR').includes(query)
    || item.label.toLocaleLowerCase('en-US').includes(query)
    || !!item.keywords?.some((word) => word.includes(query));
  const isAdmin = useAuthStore((s) => s.isAdmin());
  const location = useLocation();
  const settingsActive = location.pathname.startsWith('/settings') || !!settingsPageFor(location.pathname);
  const scrollRootRef = useRef<HTMLDivElement>(null);
  const scrollEdges = useScrollEdges(scrollRootRef);
  const settingsLink = (
    // A plain Link: NavLink would drop aria-current on the pages that live under Settings.
    <Link
      to="/settings"
      onClick={onNavigate}
      aria-label={collapsed ? t('Settings') : undefined}
      aria-current={settingsActive ? 'page' : undefined}
      className={cn(
        'flex items-center gap-2.5 rounded-md text-sm text-sidebar-foreground hover:bg-sidebar-accent/50 hover:text-sidebar-accent-foreground transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
        'min-h-10 px-3 py-2',
        collapsed && 'justify-center px-0',
        settingsActive && 'bg-sidebar-accent font-medium text-sidebar-accent-foreground',
      )}
    >
      <Settings className={cn('h-[18px] w-[18px]', settingsActive && 'text-primary')} />
      {!collapsed && <span>{t('Settings')}</span>}
    </Link>
  );

  return (
    <>
      <div className="relative flex min-h-0 flex-1 flex-col" data-nav-scroll-container>
        {!collapsed && <div className="px-2 pt-2 relative">
          <Input value={menuSearch} onChange={(event) => setMenuSearch(event.target.value)}
            aria-label={t('Search menu')} placeholder={t('Search pages…')} className="h-11 pr-12 sm:h-9 sm:pr-9"
            onKeyDown={(event) => { if (event.key === 'Escape') setMenuSearch(''); }} />
          {menuSearch && <button type="button" aria-label={t('Clear search')} onClick={() => setMenuSearch('')}
            className="absolute right-2 top-2 h-11 w-11 text-muted-foreground hover:text-foreground sm:h-9 sm:w-9">×</button>}
        </div>}
        <ScrollArea ref={scrollRootRef} type="auto" className="h-full py-2">
          <nav className="space-y-1 px-2" aria-label={mobile ? uiText("Mobile navigation") : uiText("Primary navigation")}>
            {(() => {
              const allowed = (item: NavItem) => !item.adminOnly || isAdmin;
              const items = mainNav.filter((item) => allowed(item) && matches(item));
              // Searching also finds the pages that live under Settings.
              const more = query ? settingsPages.filter((item) => allowed(item) && matches(item)) : [];
              const renderItem = (item: NavItem) => {
                const isActive = isAt(location.pathname, item.to);
                const link = (
                  <NavLink
                    key={item.to}
                    to={item.to}
                    onClick={onNavigate}
                    aria-label={collapsed ? t(item.label) : undefined}
                    aria-current={isActive ? 'page' : undefined}
                    className={cn(
                      'flex items-center gap-3 rounded-md text-sm transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
                      'min-h-10 px-3 py-2',
                      collapsed && 'justify-center px-0',
                      isActive
                        ? 'bg-sidebar-accent font-medium text-sidebar-accent-foreground'
                        : 'text-sidebar-foreground hover:bg-sidebar-accent/50 hover:text-sidebar-accent-foreground',
                    )}
                  >
                    <item.icon className={cn('h-[18px] w-[18px] shrink-0', isActive && 'text-primary')} />
                    {!collapsed && <span>{t(item.label)}</span>}
                  </NavLink>
                );
                if (!collapsed) return link;
                return (
                  <Tooltip key={item.to}>
                    <TooltipTrigger asChild>{link}</TooltipTrigger>
                    <TooltipContent side="right" className="font-medium">{t(item.label)}</TooltipContent>
                  </Tooltip>
                );
              };
              if (query && items.length === 0 && more.length === 0) {
                return <p role="status" className="px-2 py-3 text-sm text-muted-foreground">{t('No matching pages')}</p>;
              }
              return (
                <>
                  {items.map(renderItem)}
                  {more.length > 0 && (
                    <>
                      <p className="px-3 pb-1 pt-3 text-[10px] font-semibold uppercase tracking-widest text-sidebar-foreground/55">{t('Settings')}</p>
                      {more.map(renderItem)}
                    </>
                  )}
                </>
              );
            })()}
          </nav>
        </ScrollArea>
        {scrollEdges.top && (
          <div aria-hidden="true" data-nav-scroll-edge="top" className="pointer-events-none absolute inset-x-0 top-0 h-6 bg-gradient-to-b from-sidebar to-transparent" />
        )}
        {scrollEdges.bottom && (
          <div aria-hidden="true" data-nav-scroll-edge="bottom" className="pointer-events-none absolute inset-x-0 bottom-0 flex h-12 items-end justify-center bg-gradient-to-t from-sidebar via-sidebar/85 to-transparent pb-1">
            <ChevronDown className="h-4 w-4 text-sidebar-foreground/60" />
          </div>
        )}
      </div>

      <div className="border-t border-sidebar-border p-2 space-y-1">
        {collapsed ? (
          <Tooltip>
            <TooltipTrigger asChild>{settingsLink}</TooltipTrigger>
            <TooltipContent side="right" className="font-medium">{uiText("Settings")}</TooltipContent>
          </Tooltip>
        ) : settingsLink}

        {!mobile && <DesktopSidebarFooter collapsed={collapsed} />}
        {mobile && <VersionLinks collapsed={false} />}
      </div>
    </>
  );
}

function VersionLinks({ collapsed }: { collapsed: boolean }) {
  useUiLanguageStore(state => state.language);
  if (collapsed) {
    return (
      <div className="flex flex-col items-center gap-1 pt-1">
        <Tooltip>
          <TooltipTrigger asChild>
            <p className="px-1 text-center text-[9px] font-mono-data text-sidebar-foreground/35 truncate cursor-default">v{APP_VERSION}</p>
          </TooltipTrigger>
          <TooltipContent side="right" className="font-mono-data text-xs">{APP_VERSION_LABEL}</TooltipContent>
        </Tooltip>
        <div className="flex items-center gap-1">
          <ExternalNavLinks compact />
        </div>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2 px-2.5 pt-1 pb-0.5">
      <p className="min-w-0 flex-1 truncate text-[10px] font-mono-data text-sidebar-foreground/40" title={APP_VERSION_LABEL}>{APP_VERSION_LABEL}</p>
      <ExternalNavLinks />
    </div>
  );
}

function ExternalNavLinks({ compact = false }: { compact?: boolean }) {
  useUiLanguageStore(state => state.language);
  const linkClass = cn(
    'shrink-0 rounded text-sidebar-foreground/40 transition-colors hover:bg-sidebar-accent/50 hover:text-sidebar-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
    compact ? 'p-1' : 'p-2 md:p-1',
  );
  return (
    <>
      <a href={APP_REPOSITORY_URL} target="_blank" rel="noopener noreferrer" aria-label={uiText("Open TS6 Manager repository on GitHub")} title={uiText("View TS6 Manager on GitHub")} className={linkClass}>
        <Github className="h-3.5 w-3.5" />
      </a>
      <a href={APP_DOCUMENTATION_URL} target="_blank" rel="noopener noreferrer" aria-label={uiText("Open TS6 Manager documentation")} title={uiText("Open TS6 Manager documentation")} className={linkClass}>
        <BookOpen className="h-3.5 w-3.5" />
      </a>
    </>
  );
}

function DesktopSidebarFooter({ collapsed }: { collapsed: boolean }) {
  useUiLanguageStore(state => state.language);
  const { t } = useTranslation();
  const { toggleSidebar } = useUiStore();
  return (
    <>
      <button
        onClick={toggleSidebar}
        aria-label={collapsed ? uiText("Expand sidebar") : uiText("Collapse sidebar")}
        className={cn(
          'flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm text-sidebar-foreground hover:bg-sidebar-accent/50 transition-colors w-full focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
          collapsed && 'justify-center px-0 py-2',
        )}
      >
        {collapsed ? <ChevronRight className="h-4 w-4" /> : <ChevronLeft className="h-4 w-4" />}
        {!collapsed && <span>{t('Collapse')}</span>}
      </button>
      <VersionLinks collapsed={collapsed} />
    </>
  );
}

function Logo({ compact = false, onNavigate }: { compact?: boolean; onNavigate?: () => void }) {
  useUiLanguageStore(state => state.language);
  return (
    <NavLink
      to="/dashboard"
      onClick={onNavigate}
      aria-label={uiText("Teamspeak Managment dashboard")}
      title={compact ? uiText("Teamspeak Managment dashboard") : undefined}
      className={cn(
        'flex items-center gap-2.5 rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
        compact && 'justify-center',
      )}
    >
      <BrandMark className="h-7 w-7 text-primary" />
      {!compact && (
        <div>
          <span className="text-sm font-semibold text-sidebar-accent-foreground">{uiText("Teamspeak")}</span>
          <span className="text-sm text-sidebar-foreground ml-1">{uiText("Managment")}</span>
        </div>
      )}
    </NavLink>
  );
}

export function Sidebar() {
  useUiLanguageStore(state => state.language);
  const sidebarCollapsed = useUiStore((s) => s.sidebarCollapsed);

  return (
    <TooltipProvider delayDuration={0}>
      <aside
        className={cn(
          'relative hidden pb-[env(safe-area-inset-bottom)] flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground transition-all duration-300 ease-in-out lg:flex',
          sidebarCollapsed ? 'w-16' : 'w-56',
        )}
      >
        <div className={cn('flex items-center h-14 px-4 border-b border-sidebar-border', sidebarCollapsed && 'justify-center px-0')}>
          <Logo compact={sidebarCollapsed} />
        </div>
        <NavigationContent collapsed={sidebarCollapsed} />
      </aside>
    </TooltipProvider>
  );
}

export function MobileNavigation() {
  useUiLanguageStore(state => state.language);
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const location = useLocation();

  useEffect(() => {
    setOpen(false);
  }, [location.pathname]);

  return (
    <Sheet open={open} onOpenChange={setOpen}>
      <SheetTrigger asChild>
        <Button variant="ghost" size="icon" className="h-10 w-10 shrink-0 lg:hidden" aria-label={t('Open navigation menu')} title={t('Open navigation menu')}>
          <Menu className="h-5 w-5" />
        </Button>
      </SheetTrigger>
      <SheetContent>
        <SheetTitle className="sr-only">{uiText("TS6 Manager navigation")}</SheetTitle>
        <SheetDescription className="sr-only">{uiText("Choose a page to navigate to.")}</SheetDescription>
        <div className="flex min-h-14 items-center border-b border-sidebar-border px-4 pr-14 pt-[env(safe-area-inset-top)]">
          <Logo onNavigate={() => setOpen(false)} />
        </div>
        <TooltipProvider delayDuration={0}>
          <NavigationContent mobile onNavigate={() => setOpen(false)} />
        </TooltipProvider>
      </SheetContent>
    </Sheet>
  );
}
