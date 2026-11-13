-- CreateTable
CREATE TABLE "User" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "username" TEXT NOT NULL,
    "passwordHash" TEXT NOT NULL,
    "displayName" TEXT NOT NULL,
    "role" TEXT NOT NULL DEFAULT 'viewer',
    "enabled" BOOLEAN NOT NULL DEFAULT true,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL,
    "lastLoginAt" DATETIME
);

-- CreateTable
CREATE TABLE "RefreshToken" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "token" TEXT NOT NULL,
    "userId" INTEGER NOT NULL,
    "expiresAt" DATETIME NOT NULL,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "family" TEXT,
    "replacedBy" TEXT,
    CONSTRAINT "RefreshToken_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "UserServerAccess" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "userId" INTEGER NOT NULL,
    "serverConfigId" INTEGER NOT NULL,
    CONSTRAINT "UserServerAccess_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User" ("id") ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT "UserServerAccess_serverConfigId_fkey" FOREIGN KEY ("serverConfigId") REFERENCES "TsServerConfig" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "TsServerConfig" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "host" TEXT NOT NULL,
    "webqueryPort" INTEGER NOT NULL DEFAULT 10080,
    "apiKey" TEXT NOT NULL,
    "useHttps" BOOLEAN NOT NULL DEFAULT false,
    "sshPort" INTEGER NOT NULL DEFAULT 10022,
    "sshUsername" TEXT,
    "sshPassword" TEXT,
    "sshHostKeyFingerprint" TEXT,
    "enabled" BOOLEAN NOT NULL DEFAULT true,
    "isDemo" BOOLEAN NOT NULL DEFAULT false,
    "metricsEnabled" BOOLEAN NOT NULL DEFAULT false,
    "metricsPort" INTEGER NOT NULL DEFAULT 9187,
    "metricsHost" TEXT,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL
);

-- CreateTable
CREATE TABLE "BotFlow" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "description" TEXT,
    "flowData" TEXT NOT NULL DEFAULT '{"nodes":[],"edges":[]}',
    "serverConfigId" INTEGER NOT NULL,
    "virtualServerId" INTEGER NOT NULL DEFAULT 1,
    "enabled" BOOLEAN NOT NULL DEFAULT false,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL,
    CONSTRAINT "BotFlow_serverConfigId_fkey" FOREIGN KEY ("serverConfigId") REFERENCES "TsServerConfig" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "BotVariable" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "flowId" INTEGER NOT NULL,
    "name" TEXT NOT NULL,
    "value" TEXT NOT NULL DEFAULT '',
    "scope" TEXT NOT NULL DEFAULT 'flow',
    CONSTRAINT "BotVariable_flowId_fkey" FOREIGN KEY ("flowId") REFERENCES "BotFlow" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "BotExecution" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "flowId" INTEGER NOT NULL,
    "triggeredBy" TEXT NOT NULL,
    "triggerData" TEXT,
    "status" TEXT NOT NULL DEFAULT 'running',
    "startedAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "endedAt" DATETIME,
    "error" TEXT,
    CONSTRAINT "BotExecution_flowId_fkey" FOREIGN KEY ("flowId") REFERENCES "BotFlow" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "BotExecutionLog" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "executionId" INTEGER,
    "serverConfigId" INTEGER NOT NULL,
    "flowId" INTEGER,
    "nodeId" TEXT,
    "nodeName" TEXT,
    "level" TEXT NOT NULL DEFAULT 'info',
    "message" TEXT NOT NULL,
    "data" TEXT,
    "timestamp" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "BotExecutionLog_executionId_fkey" FOREIGN KEY ("executionId") REFERENCES "BotExecution" ("id") ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT "BotExecutionLog_serverConfigId_fkey" FOREIGN KEY ("serverConfigId") REFERENCES "TsServerConfig" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "AppSetting" (
    "key" TEXT NOT NULL PRIMARY KEY,
    "value" TEXT NOT NULL,
    "updatedAt" DATETIME NOT NULL
);

-- CreateTable
CREATE TABLE "MusicBot" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "serverConfigId" INTEGER NOT NULL,
    "nickname" TEXT NOT NULL DEFAULT 'MusicBot',
    "serverPassword" TEXT,
    "externalHost" TEXT,
    "defaultChannel" TEXT,
    "channelPassword" TEXT,
    "commandChannelIds" TEXT,
    "virtualServerId" INTEGER NOT NULL DEFAULT 1,
    "voicePort" INTEGER NOT NULL DEFAULT 9987,
    "volume" INTEGER NOT NULL DEFAULT 50,
    "identityData" TEXT,
    "avatarMode" TEXT NOT NULL DEFAULT 'none',
    "avatarFile" TEXT,
    "avatarMd5" TEXT,
    "autoStart" BOOLEAN NOT NULL DEFAULT false,
    "automationSettings" TEXT,
    "automationState" TEXT,
    "streamPreset" TEXT NOT NULL DEFAULT '720p',
    "sidecarPort" INTEGER NOT NULL DEFAULT 9800,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL,
    CONSTRAINT "MusicBot_serverConfigId_fkey" FOREIGN KEY ("serverConfigId") REFERENCES "TsServerConfig" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "Song" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "title" TEXT NOT NULL,
    "artist" TEXT,
    "duration" REAL,
    "filePath" TEXT NOT NULL,
    "source" TEXT NOT NULL DEFAULT 'local',
    "sourceUrl" TEXT,
    "fileSize" INTEGER,
    "serverConfigId" INTEGER NOT NULL,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "Song_serverConfigId_fkey" FOREIGN KEY ("serverConfigId") REFERENCES "TsServerConfig" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "Playlist" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "mode" TEXT NOT NULL DEFAULT 'local',
    "musicBotId" INTEGER,
    "youtubePlaylistId" TEXT,
    "serverConfigId" INTEGER,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "Playlist_musicBotId_fkey" FOREIGN KEY ("musicBotId") REFERENCES "MusicBot" ("id") ON DELETE SET NULL ON UPDATE CASCADE,
    CONSTRAINT "Playlist_serverConfigId_fkey" FOREIGN KEY ("serverConfigId") REFERENCES "TsServerConfig" ("id") ON DELETE SET NULL ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "PlaylistSong" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "playlistId" INTEGER NOT NULL,
    "songId" INTEGER NOT NULL,
    "position" INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT "PlaylistSong_playlistId_fkey" FOREIGN KEY ("playlistId") REFERENCES "Playlist" ("id") ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT "PlaylistSong_songId_fkey" FOREIGN KEY ("songId") REFERENCES "Song" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "RadioStation" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "url" TEXT NOT NULL,
    "genre" TEXT,
    "imageUrl" TEXT,
    "serverConfigId" INTEGER NOT NULL,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "RadioStation_serverConfigId_fkey" FOREIGN KEY ("serverConfigId") REFERENCES "TsServerConfig" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "IptvPlaylist" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "sourceType" TEXT NOT NULL DEFAULT 'url',
    "url" TEXT,
    "sourcePath" TEXT,
    "originalFilename" TEXT,
    "serverConfigId" INTEGER NOT NULL,
    "autoRefreshMinutes" INTEGER NOT NULL DEFAULT 0,
    "lastRefreshedAt" DATETIME,
    "lastError" TEXT,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL,
    CONSTRAINT "IptvPlaylist_serverConfigId_fkey" FOREIGN KEY ("serverConfigId") REFERENCES "TsServerConfig" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "VideoQueueEntry" (
    "id" TEXT NOT NULL PRIMARY KEY,
    "musicBotId" INTEGER NOT NULL,
    "position" INTEGER NOT NULL,
    "source" TEXT NOT NULL,
    "title" TEXT NOT NULL,
    "durationSec" INTEGER,
    "sourceMode" TEXT NOT NULL DEFAULT 'auto',
    "addedBy" TEXT,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- CreateTable
CREATE TABLE "IptvChannelPick" (
    "serverConfigId" INTEGER NOT NULL,
    "playlistId" INTEGER NOT NULL,
    "channelKey" TEXT NOT NULL,
    "name" TEXT NOT NULL,
    "favourite" BOOLEAN NOT NULL DEFAULT false,
    "lastStreamedAt" DATETIME,

    PRIMARY KEY ("serverConfigId", "playlistId", "channelKey"),
    CONSTRAINT "IptvChannelPick_serverConfigId_fkey" FOREIGN KEY ("serverConfigId") REFERENCES "TsServerConfig" ("id") ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT "IptvChannelPick_playlistId_fkey" FOREIGN KEY ("playlistId") REFERENCES "IptvPlaylist" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "IptvChannel" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "playlistId" INTEGER NOT NULL,
    "name" TEXT NOT NULL,
    "url" TEXT NOT NULL,
    "logo" TEXT,
    "groupTitle" TEXT,
    "tvgId" TEXT,
    "tvgCountry" TEXT,
    "tvgLanguage" TEXT,
    "position" INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT "IptvChannel_playlistId_fkey" FOREIGN KEY ("playlistId") REFERENCES "IptvPlaylist" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "ChatCommand" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "serverConfigId" INTEGER NOT NULL,
    "name" TEXT NOT NULL,
    "response" TEXT NOT NULL,
    "description" TEXT,
    "enabled" BOOLEAN NOT NULL DEFAULT true,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL,
    CONSTRAINT "ChatCommand_serverConfigId_fkey" FOREIGN KEY ("serverConfigId") REFERENCES "TsServerConfig" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "Widget" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "token" TEXT NOT NULL,
    "serverConfigId" INTEGER NOT NULL,
    "virtualServerId" INTEGER NOT NULL DEFAULT 1,
    "theme" TEXT NOT NULL DEFAULT 'dark',
    "showChannelTree" BOOLEAN NOT NULL DEFAULT true,
    "showClients" BOOLEAN NOT NULL DEFAULT true,
    "hideEmptyChannels" BOOLEAN NOT NULL DEFAULT false,
    "maxChannelDepth" INTEGER NOT NULL DEFAULT 5,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL,
    CONSTRAINT "Widget_serverConfigId_fkey" FOREIGN KEY ("serverConfigId") REFERENCES "TsServerConfig" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "MusicRequest" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "title" TEXT NOT NULL,
    "artist" TEXT,
    "duration" REAL,
    "url" TEXT NOT NULL,
    "source" TEXT NOT NULL DEFAULT 'url',
    "status" TEXT NOT NULL DEFAULT 'available',
    "serverConfigId" INTEGER NOT NULL,
    "musicBotId" INTEGER NOT NULL DEFAULT 0,
    "requestedAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "MusicRequest_serverConfigId_fkey" FOREIGN KEY ("serverConfigId") REFERENCES "TsServerConfig" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "StreamSession" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "musicBotId" INTEGER NOT NULL,
    "source" TEXT NOT NULL,
    "preset" TEXT NOT NULL DEFAULT '720p',
    "startedAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "endedAt" DATETIME,
    "peakViewers" INTEGER NOT NULL DEFAULT 0
);

-- CreateTable
CREATE TABLE "AdminAuditEvent" (
    "id" TEXT NOT NULL PRIMARY KEY,
    "operationId" TEXT NOT NULL,
    "actorUserId" INTEGER NOT NULL,
    "actorUsername" TEXT,
    "action" TEXT NOT NULL,
    "connectionId" INTEGER,
    "virtualServerId" INTEGER,
    "targetType" TEXT,
    "targetId" TEXT,
    "outcome" TEXT NOT NULL,
    "resultCode" TEXT,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "completedAt" DATETIME
);

-- CreateTable
CREATE TABLE "ActivityJournalTarget" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "serverConfigId" INTEGER NOT NULL,
    "virtualServerId" INTEGER NOT NULL,
    "enabled" BOOLEAN NOT NULL DEFAULT true,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL
);

-- CreateTable
CREATE TABLE "ClientActivity" (
    "id" TEXT NOT NULL PRIMARY KEY,
    "observedAt" DATETIME NOT NULL,
    "serverConfigId" INTEGER NOT NULL,
    "virtualServerId" INTEGER NOT NULL,
    "connectionGeneration" INTEGER NOT NULL DEFAULT 0,
    "eventKind" TEXT NOT NULL,
    "clientId" INTEGER NOT NULL,
    "nickname" TEXT,
    "uniqueId" TEXT,
    "databaseId" INTEGER,
    "clientType" INTEGER,
    "classification" TEXT NOT NULL DEFAULT 'unknown',
    "identityProvenance" TEXT NOT NULL DEFAULT 'none',
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- CreateIndex
CREATE UNIQUE INDEX "User_username_key" ON "User"("username");

-- CreateIndex
CREATE UNIQUE INDEX "RefreshToken_token_key" ON "RefreshToken"("token");

-- CreateIndex
CREATE UNIQUE INDEX "UserServerAccess_userId_serverConfigId_key" ON "UserServerAccess"("userId", "serverConfigId");

-- CreateIndex
CREATE UNIQUE INDEX "BotVariable_flowId_name_scope_key" ON "BotVariable"("flowId", "name", "scope");

-- CreateIndex
CREATE INDEX "BotExecutionLog_executionId_idx" ON "BotExecutionLog"("executionId");

-- CreateIndex
CREATE INDEX "BotExecutionLog_flowId_idx" ON "BotExecutionLog"("flowId");

-- CreateIndex
CREATE INDEX "BotExecutionLog_timestamp_idx" ON "BotExecutionLog"("timestamp");

-- CreateIndex
CREATE UNIQUE INDEX "PlaylistSong_playlistId_songId_key" ON "PlaylistSong"("playlistId", "songId");

-- CreateIndex
CREATE INDEX "VideoQueueEntry_musicBotId_position_idx" ON "VideoQueueEntry"("musicBotId", "position");

-- CreateIndex
CREATE INDEX "IptvChannelPick_serverConfigId_lastStreamedAt_idx" ON "IptvChannelPick"("serverConfigId", "lastStreamedAt");

-- CreateIndex
CREATE INDEX "IptvChannel_playlistId_idx" ON "IptvChannel"("playlistId");

-- CreateIndex
CREATE INDEX "IptvChannel_playlistId_name_idx" ON "IptvChannel"("playlistId", "name");

-- CreateIndex
CREATE INDEX "ChatCommand_serverConfigId_idx" ON "ChatCommand"("serverConfigId");

-- CreateIndex
CREATE UNIQUE INDEX "ChatCommand_serverConfigId_name_key" ON "ChatCommand"("serverConfigId", "name");

-- CreateIndex
CREATE UNIQUE INDEX "Widget_token_key" ON "Widget"("token");

-- CreateIndex
CREATE INDEX "MusicRequest_serverConfigId_requestedAt_idx" ON "MusicRequest"("serverConfigId", "requestedAt");

-- CreateIndex
CREATE UNIQUE INDEX "MusicRequest_serverConfigId_musicBotId_url_key" ON "MusicRequest"("serverConfigId", "musicBotId", "url");

-- CreateIndex
CREATE INDEX "AdminAuditEvent_operationId_idx" ON "AdminAuditEvent"("operationId");

-- CreateIndex
CREATE INDEX "AdminAuditEvent_createdAt_idx" ON "AdminAuditEvent"("createdAt");

-- CreateIndex
CREATE INDEX "AdminAuditEvent_actorUserId_idx" ON "AdminAuditEvent"("actorUserId");

-- CreateIndex
CREATE INDEX "AdminAuditEvent_action_idx" ON "AdminAuditEvent"("action");

-- CreateIndex
CREATE INDEX "AdminAuditEvent_connectionId_idx" ON "AdminAuditEvent"("connectionId");

-- CreateIndex
CREATE INDEX "AdminAuditEvent_outcome_idx" ON "AdminAuditEvent"("outcome");

-- CreateIndex
CREATE INDEX "AdminAuditEvent_createdAt_id_idx" ON "AdminAuditEvent"("createdAt", "id");

-- CreateIndex
CREATE INDEX "ActivityJournalTarget_enabled_idx" ON "ActivityJournalTarget"("enabled");

-- CreateIndex
CREATE UNIQUE INDEX "ActivityJournalTarget_serverConfigId_virtualServerId_key" ON "ActivityJournalTarget"("serverConfigId", "virtualServerId");

-- CreateIndex
CREATE INDEX "ClientActivity_serverConfigId_virtualServerId_observedAt_idx" ON "ClientActivity"("serverConfigId", "virtualServerId", "observedAt");

-- CreateIndex
CREATE INDEX "ClientActivity_observedAt_idx" ON "ClientActivity"("observedAt");

-- CreateIndex
CREATE INDEX "ClientActivity_serverConfigId_virtualServerId_id_idx" ON "ClientActivity"("serverConfigId", "virtualServerId", "id");

