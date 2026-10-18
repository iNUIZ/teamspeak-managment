/** UI / build identity for this fork (sourced from package.json). */

const version = typeof __APP_VERSION__ === 'string' && __APP_VERSION__ ? __APP_VERSION__ : '0.3.7';

export const APP_VERSION = version;
export const APP_REPOSITORY_URL = 'https://github.com/iNUIZ/teamspeak-managment';
export const APP_DOCUMENTATION_URL = 'https://nuiz.mintlify.site/index';

/** Public fork label shown in the panel footer and About card. */
export const APP_VERSION_LABEL = `Teamspeak Managment v${version}`;
