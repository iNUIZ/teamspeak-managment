import { translate as uiText, useLanguageStore as useUiLanguageStore } from '@/lib/i18n';
import { Navigate, useSearchParams } from 'react-router-dom';
import { PageHeader } from '@/components/shared/PageHeader';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import {
  Library,
  ListMusic,
  FileAudio,
  Radio,
  Clock,
  Video,
  Tv,
  Youtube,
} from 'lucide-react';
import Iptv from '@/pages/Iptv';
import { YouTubeTab } from '@/pages/Settings';
import { RequestsTab } from '@/components/media/RequestsTab';
import { ActiveBotsMenu } from '@/components/media/ActiveBotsMenu';
import { isMediaLibraryTab, mediaLibraryRedirect, type MediaLibraryTab } from '@/lib/media-library-redirect';
import { LibraryTab } from './LibraryTab';
import { PlaylistsTab } from './PlaylistsTab';
import { RadioTab } from './RadioTab';
import { StreamingDefaultsTab } from './StreamingDefaultsTab';


// ─── Main Page ───────────────────────────────────────────────────────────────

/**
 * Media Library (`/media-bots`): media shared by every bot on a server.
 * Bots live in the Bot Hub; old Bots, Queue, Video and Commands links redirect.
 */
export default function MusicBots() {
  useUiLanguageStore(state => state.language);
  const [searchParams, setSearchParams] = useSearchParams();
  const redirect = mediaLibraryRedirect(searchParams);
  const tabParam = searchParams.get('tab');
  const activeTab: MediaLibraryTab = isMediaLibraryTab(tabParam) ? tabParam : 'library';
  const setActiveTab = (tab: string) => {
    const next = new URLSearchParams(searchParams);
    if (tab === 'library') next.delete('tab'); else next.set('tab', tab);
    next.delete('bot');
    setSearchParams(next, { replace: true });
  };

  if (redirect) return <Navigate to={redirect} replace />;

  return (
    <div className="space-y-5">
      <PageHeader
        title={uiText("Library")}
        icon={Library}
        description={uiText("Songs, playlists, radio, TV and YouTube access for the bots of the selected server.")}
        actions={<ActiveBotsMenu />}
      />

      <Tabs value={activeTab} onValueChange={setActiveTab} className="space-y-4">
        <TabsList className="max-w-full justify-start overflow-x-auto">
          <TabsTrigger value="library"><FileAudio className="h-3.5 w-3.5 mr-1.5" /> {uiText("Songs")}</TabsTrigger>
          <TabsTrigger value="playlists"><ListMusic className="h-3.5 w-3.5 mr-1.5" /> {uiText(" Playlists")}</TabsTrigger>
          <TabsTrigger value="radio"><Radio className="h-3.5 w-3.5 mr-1.5" /> {uiText(" Radio stations")}</TabsTrigger>
          <TabsTrigger value="tv"><Tv className="h-3.5 w-3.5 mr-1.5" /> {uiText("TV (IPTV)")}</TabsTrigger>
          <TabsTrigger value="requests"><Clock className="h-3.5 w-3.5 mr-1.5" /> {uiText(" Requests")}</TabsTrigger>
          <TabsTrigger value="youtube"><Youtube className="h-3.5 w-3.5 mr-1.5" /> {uiText("YouTube access")}</TabsTrigger>
          <TabsTrigger value="streaming"><Video className="h-3.5 w-3.5 mr-1.5" /> {uiText(" Streaming defaults")}</TabsTrigger>
        </TabsList>

        <TabsContent value="library"><LibraryTab /></TabsContent>
        <TabsContent value="playlists"><PlaylistsTab /></TabsContent>
        <TabsContent value="radio"><RadioTab /></TabsContent>
        <TabsContent value="tv"><Iptv embedded /></TabsContent>
        <TabsContent value="requests"><RequestsTab /></TabsContent>
        <TabsContent value="youtube"><YouTubeTab /></TabsContent>
        <TabsContent value="streaming"><StreamingDefaultsTab /></TabsContent>
      </Tabs>
    </div>
  );
}
