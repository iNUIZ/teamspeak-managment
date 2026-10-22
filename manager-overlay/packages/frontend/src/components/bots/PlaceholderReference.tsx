import { translate as uiText, useLanguageStore as useUiLanguageStore } from '@/lib/i18n';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle,
} from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Badge } from '@/components/ui/badge';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/* ------------------------------------------------------------------ */
/*  Tiny helper: one placeholder row                                  */
/* ------------------------------------------------------------------ */
function P({ code, desc, example }: { code: string; desc: string; example?: string }) {
  useUiLanguageStore(state => state.language);
  return (
    <div className="py-1.5 grid grid-cols-[1fr_1.4fr_1.6fr] gap-2 items-start text-xs border-b border-border/40 last:border-0">
      <code className="text-[11px] font-mono text-emerald-400 break-all">{code}</code>
      <span className="text-muted-foreground">{desc}</span>
      {example ? (
        <code className="text-[10px] font-mono text-muted-foreground/70 bg-muted/40 rounded px-1.5 py-0.5 break-all">{example}</code>
      ) : <span />}
    </div>
  );
}

function SectionHeader({ children }: { children: React.ReactNode }) {
  useUiLanguageStore(state => state.language);
  return <div className="mt-3 mb-1.5 first:mt-0"><Badge variant="outline" className="text-[10px]">{children}</Badge></div>;
}

/* ------------------------------------------------------------------ */
/*  Main component                                                    */
/* ------------------------------------------------------------------ */
export function PlaceholderReference({ open, onOpenChange }: Props) {
  useUiLanguageStore(state => state.language);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl !grid-rows-none !block p-0 overflow-hidden">
        <DialogHeader className="px-5 pt-5 pb-2">
          <DialogTitle className="text-base">{uiText("Placeholder Reference")}</DialogTitle>
          <p className="text-xs text-muted-foreground">
            {uiText("Use ")}<code className="text-emerald-400">{'{{placeholder}}'}</code> {uiText(" in any text field. Values are resolved at runtime.")}</p>
        </DialogHeader>

        <Tabs defaultValue="event" className="px-5 pb-5">
          <TabsList className="h-8 mb-3">
            <TabsTrigger value="event" className="text-xs px-2.5 h-6">{uiText("Event")}</TabsTrigger>
            <TabsTrigger value="time" className="text-xs px-2.5 h-6">{uiText("Time")}</TabsTrigger>
            <TabsTrigger value="var" className="text-xs px-2.5 h-6">{uiText("Variables")}</TabsTrigger>
            <TabsTrigger value="temp" className="text-xs px-2.5 h-6">{uiText("Temp")}</TabsTrigger>
            <TabsTrigger value="exec" className="text-xs px-2.5 h-6">{uiText("Exec")}</TabsTrigger>
            <TabsTrigger value="filter" className="text-xs px-2.5 h-6">{uiText("Filter")}</TabsTrigger>
            <TabsTrigger value="functions" className="text-xs px-2.5 h-6">{uiText("Functions")}</TabsTrigger>
          </TabsList>

          <ScrollArea className="h-[58vh]">
            {/* ========== EVENT ========== */}
            <TabsContent value="event" className="mt-0 pr-3">
              <p className="text-xs text-muted-foreground mb-2">
                {uiText("Fields from the triggering TS3 event. Available fields depend on the event type.")}</p>

              <SectionHeader>{uiText("Client Connected — notifycliententerview")}</SectionHeader>
              <P code="{{event.clid}}" desc="Client ID" example='Move client #{{event.clid}} to channel' />
              <P code="{{event.client_nickname}}" desc="Nickname" example='Welcome {{event.client_nickname}}!' />
              <P code="{{event.client_database_id}}" desc="Database ID" example='DB-ID: {{event.client_database_id}}' />
              <P code="{{event.client_unique_identifier}}" desc="Unique ID (UID)" example='UID: {{event.client_unique_identifier}}' />
              <P code="{{event.client_type}}" desc="Type (0=User, 1=Query)" example='Condition: event.client_type == 0' />
              <P code="{{event.client_servergroups}}" desc="Server group IDs (comma-separated)" example="contains(event.client_servergroups, '6')" />
              <P code="{{event.connection_client_ip}}" desc="IP address" example='VPN check: https://api.vpn.io/{{event.connection_client_ip}}' />
              <P code="{{event.cid}}" desc="Channel the client joined" example='Condition: event.cid == 42' />

              <SectionHeader>{uiText("Client Disconnected — notifyclientleftview")}</SectionHeader>
              <P code="{{event.clid}}" desc="Client ID" />
              <P code="{{event.cfid}}" desc="Channel the client was in" />
              <P code="{{event.reasonid}}" desc="Reason (3=lost, 5=kick, 6=ban, 8=leave)" example='Condition: event.reasonid == 5' />
              <P code="{{event.reasonmsg}}" desc="Reason message" example='Kicked: {{event.reasonmsg}}' />
              <P code="{{event.client_nickname}}" desc="Nickname (enriched from enter cache when available)" example='{{event.client_nickname}} left TeamSpeak' />
              <P code="{{event.client_type}}" desc="Type (0=User, 1=Query) when known from enter" example='Condition: event.client_type == 0' />
              <P code="{{event.client_database_id}}" desc="Database ID when known from enter" />
              <P code="{{event.client_unique_identifier}}" desc="Unique ID when known from enter" />
              <p className="text-[11px] text-muted-foreground mt-1 mb-2">
                {uiText("Leave notifications from TeamSpeak often omit identity fields. EventBridge merges cached enter metadata when present; missing cache still emits the native leave payload.")}</p>

              <SectionHeader>{uiText("Client Moved — notifyclientmoved")}</SectionHeader>
              <P code="{{event.clid}}" desc="Client ID" />
              <P code="{{event.ctid}}" desc="Target channel ID" example='Condition: event.ctid == 10' />
              <P code="{{event.cfid}}" desc="Source channel ID" />
              <P code="{{event.reasonid}}" desc="Reason (0=self, 1=moved by admin)" />

              <SectionHeader>{uiText("Text Message — notifytextmessage")}</SectionHeader>
              <P code="{{event.clid}}" desc="Sender client ID" />
              <P code="{{event.client_nickname}}" desc="Sender nickname" />
              <P code="{{event.msg}}" desc="Message text" example='User wrote: {{event.msg}}' />
              <P code="{{event.targetmode}}" desc="Target (1=private, 2=channel, 3=server)" />

              <SectionHeader>{uiText("Chat Command — trigger_command")}</SectionHeader>
              <P code="{{event.command_name}}" desc="Command name (without prefix)" example='Command: {{event.command_name}}' />
              <P code="{{event.command_args}}" desc="Arguments after command" example='Args: {{event.command_args}}' />
              <P code="{{event.clid}}" desc="Sender client ID" />
              <P code="{{event.client_nickname}}" desc="Sender nickname" />

              <SectionHeader>{uiText("Channel Events — created / edited / deleted")}</SectionHeader>
              <P code="{{event.cid}}" desc="Channel ID" />
              <P code="{{event.invokerid}}" desc="Client ID of who made the change" />
              <P code="{{event.invokername}}" desc="Name of who made the change" />

              <SectionHeader>{uiText("Webhook — trigger_webhook")}</SectionHeader>
              <P code="{{event.webhook_path}}" desc="Request path" example='/my-hook' />
              <P code="{{event.webhook_method}}" desc="HTTP method" example='GET or POST' />
              <P code="{{event.webhook_body}}" desc="Request body (raw JSON)" />
              <P code="{{event.webhook_body.field}}" desc="Nested field from JSON body" example='Status: {{event.webhook_body.status}}' />
              <P code="{{event.webhook_query}}" desc="Query parameters (raw JSON)" />
            </TabsContent>

            {/* ========== TIME ========== */}
            <TabsContent value="time" className="mt-0 pr-3">
              <p className="text-xs text-muted-foreground mb-2">
                {uiText("Current date and time. Respects the timezone configured on the Cron trigger (falls back to UTC).")}</p>
              <P code="{{time.time}}" desc="Formatted time HH:MM" example='Channel name: Es ist {{time.time}} Uhr' />
              <P code="{{time.date}}" desc="Formatted date DD.MM.YYYY" example='Datum: {{time.date}}' />
              <P code="{{time.hours}}" desc="Hour (00-23)" example='Condition: time.hours >= 22' />
              <P code="{{time.minutes}}" desc="Minute (00-59)" />
              <P code="{{time.seconds}}" desc="Second (00-59)" />
              <P code="{{time.day}}" desc="Day of month (01-31)" />
              <P code="{{time.month}}" desc="Month (01-12)" example='Condition: time.month == 12' />
              <P code="{{time.year}}" desc="Year (4-digit)" example='2026' />
              <P code="{{time.dayOfWeek}}" desc="Day of week (0=Sun, 1=Mon, ..., 6=Sat)" example='Condition: time.dayOfWeek == 0' />
              <P code="{{time.timestamp}}" desc="Unix timestamp (seconds)" example='1739462400' />
            </TabsContent>

            {/* ========== VARIABLES ========== */}
            <TabsContent value="var" className="mt-0 pr-3">
              <p className="text-xs text-muted-foreground mb-2">
                {uiText("Persistent variables stored in the database. Survive restarts. Scoped per flow. Set via the ")}<strong>{uiText("Set Variable")}</strong> {uiText(" action node.")}</p>
              <P code={'{{var.name}}'} desc="Read variable by name" example='Counter: {{var.visit_count}}' />

              <SectionHeader>{uiText("Actions for Variables")}</SectionHeader>
              <div className="text-xs text-muted-foreground space-y-1 mt-1">
                <p><strong>{uiText("Set")}</strong> {uiText(" — Set a variable to a fixed value or template")}</p>
                <p><strong>{uiText("Increment")}</strong> {uiText(" — Add a number to the current value")}</p>
                <p><strong>{uiText("Append")}</strong> {uiText(" — Append text to the current value")}</p>
              </div>

              <SectionHeader>{uiText("Use Case Examples")}</SectionHeader>
              <div className="text-xs text-muted-foreground space-y-1 mt-1">
                <p>{uiText("Visit counter: Increment ")}<code className="text-emerald-400">visit_count</code> {uiText(" by 1 on each join")}</p>
                <p>{uiText("Last seen: Set ")}<code className="text-emerald-400">{'lastseen_{{event.client_database_id}}'}</code> {uiText(" to ")}<code className="text-emerald-400">{'{{time.timestamp}}'}</code></p>
                <p>{uiText("Online time tracking: Increment ")}<code className="text-emerald-400">{'onlinetime_{{event.clid}}'}</code></p>
              </div>
            </TabsContent>

            {/* ========== TEMP ========== */}
            <TabsContent value="temp" className="mt-0 pr-3">
              <p className="text-xs text-muted-foreground mb-2">
                {uiText("Temporary variables that only exist during a single flow execution. Automatically set by certain action nodes. Supports nested access via dot notation.")}</p>

              <SectionHeader>{uiText("Auto-set by Actions")}</SectionHeader>
              <P code="{{temp.lastCreatedChannelId}}" desc="CID of channel created by Create Channel" example='Move client to {{temp.lastCreatedChannelId}}' />
              <P code="{{temp.lastResult}}" desc="Raw JSON result from WebQuery action" />
              <P code="{{temp.afkMovedCount}}" desc="Number of clients moved by AFK Mover" example='Moved {{temp.afkMovedCount}} AFK users' />
              <P code="{{temp.idleKickedCount}}" desc="Number of clients kicked by Idle Kicker" />
              <P code="{{temp.pokedCount}}" desc="Number of clients poked by Poke Group" />
              <P code="{{temp.rankPromotedCount}}" desc="Number of clients promoted by Rank Check" />
              <P code="{{temp.tempChannelsDeleted}}" desc="Number of channels deleted by Temp Cleanup" />

              <SectionHeader>{uiText("Custom via \"Store As\"")}</SectionHeader>
              <p className="text-xs text-muted-foreground mb-1">
                {uiText("WebQuery and HTTP Request actions have a \"Store As\" field. The result is saved as a temp variable.")}</p>
              <P code={'{{temp.server.virtualserver_clientsonline}}'} desc='Online users (after serverinfo stored as "server")' example='Online: {{temp.server.virtualserver_clientsonline}}/{{temp.server.virtualserver_maxclients}}' />
              <P code={'{{temp.server.virtualserver_uptime}}'} desc='Server uptime in seconds' example='Uptime: {{temp.server.virtualserver_uptime|uptime}}' />
              <P code={'{{temp.client.client_nickname}}'} desc='Client name (after clientinfo stored as "client")' />
            </TabsContent>

            {/* ========== EXEC ========== */}
            <TabsContent value="exec" className="mt-0 pr-3">
              <p className="text-xs text-muted-foreground mb-2">
                {uiText("Read-only metadata about the current flow execution.")}</p>
              <P code="{{exec.flowId}}" desc="ID of the flow being executed" />
              <P code="{{exec.executionId}}" desc="Unique execution instance ID" />
              <P code="{{exec.configId}}" desc="Server config ID" />
              <P code="{{exec.sid}}" desc="Virtual server ID" />
              <P code="{{exec.triggerType}}" desc="Trigger type (event, cron, webhook, command)" example='Condition: exec.triggerType == "cron"' />
            </TabsContent>

            {/* ========== FILTER ========== */}
            <TabsContent value="filter" className="mt-0 pr-3">
              <p className="text-xs text-muted-foreground mb-2">
                {uiText("Filters transform values. Apply with pipe syntax: ")}<code className="text-emerald-400">{'{{value|filter}}'}</code>
              </p>
              <P code="uptime" desc="Converts seconds to readable uptime" example='{{temp.server.virtualserver_uptime|uptime}} → "5d 3h 42m"' />
              <P code="round" desc="Rounds to nearest integer" example='{{temp.value|round}} → "42"' />
              <P code="floor" desc="Rounds down to integer" example='{{temp.value|floor}} → "41"' />
              <P code="dropFirst" desc="Text after the first word" example='{{event.command_args|dropFirst}}' />
            </TabsContent>

            {/* ========== FUNCTIONS ========== */}
            <TabsContent value="functions" className="mt-0 pr-3">
              <p className="text-xs text-muted-foreground mb-2">
                {uiText("Functions for use in ")}<strong>{uiText("Condition")}</strong> {uiText(" node expressions. Return 0 or 1 (false/true).")}</p>
              <P code="contains(str, sub)" desc="String contains substring?" example="contains(event.client_servergroups, '6')" />
              <P code="startsWith(str, prefix)" desc="String starts with prefix?" example="startsWith(event.client_nickname, 'Admin')" />
              <P code="endsWith(str, suffix)" desc="String ends with suffix?" example="endsWith(event.client_nickname, 'Bot')" />
              <P code="lower(str)" desc="Convert to lowercase" example="contains(lower(event.client_nickname), 'bot')" />
              <P code="upper(str)" desc="Convert to uppercase" />
              <P code="length(str)" desc="String length" example="length(event.msg) > 100" />
              <P code="split(str, sep, idx)" desc="Split string and get element at index" example="split(event.command_args, ' ', 0)" />
              <P code="dropFirst(str)" desc="Text after the first word" example="length(dropFirst(event.command_args)) > 0" />

              <SectionHeader>{uiText("Condition Examples")}</SectionHeader>
              <div className="text-xs text-muted-foreground space-y-1 mt-1">
                <p><code className="text-emerald-400">event.client_type == 0</code> {uiText(" — Only real users (no query clients)")}</p>
                <p><code className="text-emerald-400">{"contains(event.client_servergroups, '7')"}</code> {uiText(" — Client is in server group 7")}</p>
                <p><code className="text-emerald-400">time.hours {'>='} 22 or time.hours {'<'} 6</code> {uiText(" — Nighttime only")}</p>
                <p><code className="text-emerald-400">event.ctid == 42</code> {uiText(" — Client moved to specific channel")}</p>
                <p><code className="text-emerald-400">temp.vpn.security.vpn == 1</code> {uiText(" — VPN detected (after HTTP request)")}</p>
              </div>
            </TabsContent>
          </ScrollArea>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
