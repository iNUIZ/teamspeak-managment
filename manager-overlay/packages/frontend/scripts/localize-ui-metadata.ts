/** Mechanical migration of authored metadata at its render boundary, never user data. */
import fs from 'node:fs';
const root='src/';
const targets: Record<string,string[]> = {
 'components/bots/TemplateGallery.tsx':['cat.label','cat.description','tpl.name','tpl.description','f.label','selected.description','selected.name','field.label','opt.label'],
 'components/connections/ConnectionSetupHelpDialog.tsx':['item.label','step.title','tip.label','scenario.label'],
 'components/connections/ConnectionSetupWizard.tsx':['title','STEPS[step]','suggestedScenario.label','s.label','s.description','scenario.label','scenario.hostHint','scenario.notes','firewallGuide.body','webqueryPortGuide.body','guide.body','apiKeyGuide.body','sshGuide.body','FIELD_HELP.name','FIELD_HELP.host','FIELD_HELP.useHttps','FIELD_HELP.metricsEnabled','FIELD_HELP.sshPort','FIELD_HELP.sshUsername','FIELD_HELP.sshPassword'],
 'components/media/NowPlaying.tsx':['d.label'],
 'components/video/VideoOptions.tsx':['o.label'],
 'pages/bot-hub/MusicSource.tsx':['v.label'],
 'pages/media-bots/CommandsTab.tsx':['preset.description'],
};
for(const [file,expressions] of Object.entries(targets)) {
 let source=fs.readFileSync(root+file,'utf8');
 for(const expression of expressions) source=source.replaceAll('{'+expression+'}', '{uiText('+expression+')}');
 fs.writeFileSync(root+file,source);
}
