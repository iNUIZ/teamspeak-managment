import { test, expect, type Page, type APIRequestContext } from '@playwright/test';

async function signIn(page:Page,request:APIRequestContext) {
  await request.post('/__test/reset'); await request.post('/__test/dashboard?scenario=normal');
  await request.post('/__test/docs?on'); await request.post('/__test/auth?on');
  await page.route('**/api/audit**',r=>r.fulfill({json:{items:[],nextCursor:null,retention:{maxAgeDays:30,maxRows:100000}}}));
  await page.route('**/api/activity-journal/**',r=>r.fulfill({json:r.request().url().includes('/status')?{statuses:[],retention:{days:7,perConnectionSid:10000,global:100000}}:{items:[],nextCursor:null}}));
  await page.route('**/api/settings/media-diagnostics',r=>r.fulfill({json:{checkedAt:new Date().toISOString(),overall:'ok',checks:[],sidecar:{mode:'url',status:'ok',message:'Technical provider data'},encoders:{status:'ok',message:'Technical encoder data',items:[]},warnings:[]}}));
  await page.goto('/login'); await page.getByLabel('Username',{exact:true}).fill('admin');
  await page.getByLabel('Password',{exact:true}).fill('test-password'); await page.getByRole('button',{name:'Sign In',exact:true}).click();
  await expect(page).toHaveURL('/dashboard'); await page.getByRole('button',{name:'Türkçe',exact:true}).click();
}

test('TR navigation, empty states and media presentation across panel routes',async({page,request})=>{
  const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));await signIn(page,request);
  const routes=[['/dashboard','Operations Voice'],['/servers','TeamSpeak sunucuları'],['/channels','Kanallar'],['/clients','Kullanıcılar'],['/server-groups','Sunucu grupları'],['/channel-groups','Kanal grupları'],['/permissions','Yetkiler'],['/bans','Yasaklar'],['/tokens','Yetki anahtarları'],['/files','Dosya yöneticisi'],['/complaints','Şikâyetler'],['/messages','Mesajlar'],['/reports?view=audit','Raporlar ve tanılama'],['/reports?view=activity','Raporlar ve tanılama'],['/reports?view=media','Raporlar ve tanılama'],['/bots','Bot akışları'],['/bot-hub','Bot merkezi'],['/media-bots','Medya kütüphanesi'],['/iptv','IPTV'],['/settings?tab=connections','Ayarlar']];
  for(const [route,title] of routes){
    await page.goto(route); await expect(page.locator('main').getByRole('heading',{name:title,exact:true}).first()).toBeVisible();
    await expect(page.locator('html')).toHaveAttribute('lang','tr');
    const visible=await page.locator('main').innerText();
    expect(visible,route).not.toMatch(/Live (client|channel) updates active|No complaints found|No clients found|Imports up to|Channel empty|Playlist data up to date|SEÇ SERVER GROUP|Sidecar\b/);
  }
  expect(errors).toEqual([]);
  await page.goto('/permissions'); await expect(page.getByText('Sunucu grupları seçin',{exact:true})).toBeVisible();
  await page.getByRole('button',{name:'İngilizce',exact:true}).click();
  await expect(page.getByRole('heading',{name:'Permissions',exact:true})).toBeVisible();
  await page.getByRole('button',{name:'Türkçe',exact:true}).click();await page.reload();
  await expect(page.getByRole('heading',{name:'Yetkiler',exact:true})).toBeVisible();
  await page.getByRole('button',{name:'Açık temaya geç',exact:true}).click();
  await page.getByRole('button',{name:'Koyu temaya geç',exact:true}).click();
  await expect(page.getByRole('button',{name:'Açık temaya geç',exact:true})).toBeVisible();
});

test('TR authored template names, descriptions and connection help are localized',async({page,request})=>{
  await signIn(page,request);await page.goto('/bots');await page.getByRole('button',{name:'Şablondan oluştur',exact:true}).click();
  const dialog=page.getByRole('dialog');
  await expect(dialog.getByText('Saat kanalı',{exact:true})).toBeVisible();
  const templateText=await dialog.innerText();expect(templateText).not.toMatch(/Clock Channel|Online Counter|Support System|Temp Channel Creator|Timed Reminder|Webhook →/);
  await dialog.getByText('Saat kanalı',{exact:true}).click();
  await expect(dialog.getByRole('heading')).toHaveText('Ayarla: Saat kanalı');
  await expect(dialog.getByRole('spinbutton',{name:/^Kanal kimliği/})).toBeVisible();
  await page.keyboard.press('Escape');await page.goto('/settings?tab=connections');await page.getByRole('button',{name:'Yardım',exact:true}).click();
  await dialog.getByRole('button',{name:'TeamSpeak’i hazırla',exact:true}).click();
  await expect(dialog.getByText('WebQuery HTTP’yi aç',{exact:true})).toBeVisible();
  const help=await dialog.innerText();expect(help).not.toMatch(/In your TeamSpeak|Connect via SSH|Enable HTTP|Default port|Set serveradmin/);
  await dialog.getByRole('button',{name:'Kurulum sihirbazını başlat',exact:true}).click();
  await expect(dialog.getByRole('heading',{name:'Bağlantı kurulum sihirbazı',exact:true})).toBeVisible();
  expect(await dialog.innerText()).not.toMatch(/Where is your|Where is|same machine|separate stacks|TeamSpeak runs|Run again|No local server/);
  await dialog.getByRole('button',{name:'Sonraki',exact:true}).click();
  await expect(dialog.getByText('Adım 2 / 4: WebQuery bağlantısı',{exact:true})).toBeVisible();
  await expect.poll(()=>dialog.evaluate(element=>element.scrollTop)).toBe(0);
});

test('TR preserves English user data and works on a small viewport',async({page,request})=>{
  await signIn(page,request);await page.setViewportSize({width:390,height:844});await page.goto('/bot-hub');
  await expect(page.getByText('Neon Skyline — Demo Ensemble',{exact:true}).first()).toBeVisible();
  await expect(page.getByText('Aurora Radio',{exact:true})).toBeVisible();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  await page.goto('/complaints');await expect(page.getByText('Şikâyet bulunamadı',{exact:true})).toBeVisible();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
});

test('TR live bot errors are localized without concealing the native diagnostic or changing playing state',async({page,request})=>{
  await page.route('**/api/music-bots/media',async route=>{
    const response=await route.fetch();const bots=await response.json();
    await route.fulfill({json:bots.map((bot:any,index:number)=>({...bot,lastError:index===0?'TS3 error 1538: invalid parameter':'New provider diagnostic text',automation:{autoDj:true,stayInChannel:true,sourceKind:'radio',playing:true}}))});
  });
  await signIn(page,request);await page.goto('/bot-hub');
  const issue=page.getByRole('alert').filter({hasText:'TeamSpeak hatası 1538: geçersiz parametre'});
  await expect(issue).toBeVisible();
  const name=page.getByText('Aurora Radio',{exact:true});
  expect(await name.evaluate(element=>element.scrollWidth<=element.clientWidth)).toBe(true);
  await expect(issue.locator('pre')).not.toBeVisible();
  await expect(page.getByText('Neon Skyline — Demo Ensemble',{exact:true}).first()).toBeVisible();
  await expect(page.getByText('Bot bir sorun bildirdi. Özgün mesaj için teknik ayrıntıları açın.',{exact:true})).toBeVisible();
  await issue.getByText('Teknik çıktı (özgün dilinde)',{exact:true}).click();
  await expect(issue.locator('pre')).toHaveText('TS3 error 1538: invalid parameter');
  await expect(issue.locator('pre')).toBeVisible();
  await issue.getByText('Teknik çıktı (özgün dilinde)',{exact:true}).click();
  await expect(issue.locator('pre')).not.toBeVisible();
  await page.getByRole('button',{name:'İngilizce',exact:true}).click();
  await expect(page.getByRole('alert').filter({hasText:'TS3 error 1538: invalid parameter'})).toBeVisible();
  await page.getByRole('button',{name:'Türkçe',exact:true}).click();
  await expect(issue).toBeVisible();
});
