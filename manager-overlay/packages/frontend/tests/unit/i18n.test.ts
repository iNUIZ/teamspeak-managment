import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';
import { hasUiTranslation, translate, translateUiError, useLanguageStore } from '../../src/lib/i18n';
import { apiErrorPresentation } from '../../src/lib/api-error';
import { hubFacts, hubHeadline, queuePosition } from '../../src/lib/bot-hub';
import { videoOptionsSummary } from '../../src/lib/video-options';
import { permissionLabel } from '../../src/lib/permission-labels';
import { pageRangeLabel } from '../../src/lib/pager';

afterEach(() => useLanguageStore.setState({language:'tr'}));
test('TR keeps whitespace, interpolates without modifying user data, switches back to EN', () => {
  useLanguageStore.setState({language:'tr'});
  assert.equal(translate('  Play  '),'  Oynat  ');
  assert.equal(translate('Added {0} to the queue.','My English Song {1}'),'My English Song {1} sıraya eklendi.');
  assert.equal(translate('My English Song'),'My English Song');
  for(const name of ['constructor','toString','__proto__'])assert.equal(translate(name),name);
  useLanguageStore.setState({language:'en'});
  assert.equal(translate('Added {0} to the queue.','My English Song'),'Added My English Song to the queue.');
  useLanguageStore.setState({language:'tr'});
  assert.equal(translate('Create Widget'),'Bileşen oluştur');
});
test('unknown service prose is localized safely without a false translation or secret leakage', () => {
  useLanguageStore.setState({language:'tr'});
  const result=apiErrorPresentation({response:{status:502,data:{error:'Provider exploded token=secret',details:'Some new upstream English error'}}},'Could not load songs');
  assert.equal(result.title,'Şarkılar yüklenemedi');
  assert.equal(result.message,'Ayrıntılar için teknik günlükleri inceleyin.');
  assert.equal(translateUiError('New undocumented upstream error','Also unknown'),'Bir hata oluştu');
  useLanguageStore.setState({language:'en'});
  assert.equal(translateUiError('Original service error','Fallback'),'Original service error');
});
test('dynamic media state, queue and options are Turkish while machine values stay intact', () => {
  useLanguageStore.setState({language:'tr'});
  assert.equal(queuePosition({queue:[1,2],currentIndex:0}),'Parça 1 / 2');
  const bot:any={status:'connected',session:null,music:null,video:null};
  assert.equal(hubHeadline(bot),'Bağlı, medya bekliyor');
  assert.deepEqual(hubFacts({...bot,music:{live:true,position:1}},Date.now()),['Canlı radyo','Süre: 0:01']);
  const options:any={quality:'auto',encoder:'auto',noViewerTimeout:'900',sourceMode:'auto'};
  const summary=videoOptionsSummary(options);
  assert(!/stops after|Default|Detect|encoder/.test(summary),summary);
  assert.equal(options.quality,'auto');
  assert(hasUiTranslation('Copy privilege key'));
  assert.equal(pageRangeLabel(0,1,50,'songs'),'0 şarkı');
  assert.equal(pageRangeLabel(60,1,50,'channels'),'1–50 / 60 kanal');
});

test('TeamSpeak permission identifiers stay stable and unknown extension descriptions are not invented',()=>{
  useLanguageStore.setState({language:'tr'});
  const known={permid:42,permsid:'i_ft_file_upload_power',permdesc:'File upload power'};
  assert.equal(permissionLabel(known),'Dosya yükleme gücü');
  assert.equal(known.permsid,'i_ft_file_upload_power');
  assert.equal(permissionLabel({...known,permid:99,permsid:'extension_custom',permdesc:'New extension English description'}),'Yetki #99');
  useLanguageStore.setState({language:'en'});
  assert.equal(permissionLabel(known),'File upload power');
});
