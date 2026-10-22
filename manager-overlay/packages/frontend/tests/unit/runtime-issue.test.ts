import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';
import { useLanguageStore } from '../../src/lib/i18n';
import { runtimeIssueSummary } from '../../src/lib/runtime-issue';
afterEach(()=>useLanguageStore.setState({language:'tr'}));
test('native bot failures keep their code and translate the known description',()=>{
  useLanguageStore.setState({language:'tr'});
  assert.equal(runtimeIssueSummary('TS3 error 1538: invalid parameter','Fallback'),'TeamSpeak hatası 1538: geçersiz parametre');
});
test('unknown provider messages use a Turkish explanation, not an invented translation',()=>{
  useLanguageStore.setState({language:'tr'});
  assert.equal(runtimeIssueSummary('New provider diagnostic text','The bot reported a problem. Open technical details for the original message.'),'Bot bir sorun bildirdi. Özgün mesaj için teknik ayrıntıları açın.');
});
test('EN keeps original evidence and permission failures remain distinct from cookies',()=>{
  useLanguageStore.setState({language:'en'});
  assert.equal(runtimeIssueSummary('TS3 error 1538: invalid parameter','Fallback'),'TS3 error 1538: invalid parameter');
  useLanguageStore.setState({language:'tr'});
  assert(!runtimeIssueSummary('TS3 error 2568: insufficient client permissions','Fallback').includes('insufficient'));
});
