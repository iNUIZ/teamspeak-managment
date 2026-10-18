export const TS6_SERVER_DOCS = {
  home: 'https://nuiz.mintlify.site/index',
  quickstart: 'https://nuiz.mintlify.site/index',
  httpQuery: 'https://nuiz.mintlify.site/index',
  sshQuery: 'https://nuiz.mintlify.site/index',
  authentication: 'https://nuiz.mintlify.site/index',
  queryOverview: 'https://nuiz.mintlify.site/index',
  portsNetworking: 'https://nuiz.mintlify.site/index',
  dockerConfig: 'https://nuiz.mintlify.site/index',
  security: 'https://nuiz.mintlify.site/index',
  githubDocker: 'https://github.com/iNUIZ/teamspeak-managment',
} as const;

export interface SetupDocLink {
  label: string;
  url: string;
}
