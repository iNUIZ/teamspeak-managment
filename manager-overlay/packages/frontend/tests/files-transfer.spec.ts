import { test, expect } from '@playwright/test';

test('TR file browser navigates protocol directories, uploads/downloads/deletes and stays localized', async ({page, request}) => {
  await request.post('/__test/reset'); await request.post('/__test/dashboard?scenario=normal'); await request.post('/__test/auth?on');
  await page.addInitScript(() => localStorage.setItem('teamhub-language', JSON.stringify({state:{language:'tr'},version:0})));
  const folders = new Set<string>(); const files = new Map<string, string>();
  const sent: string[] = []; let rejectMkdir = false;
  await page.route('**/api/servers/*/vs/*/channels', route => route.fulfill({json:[{cid:1, channel_name:'Dosyalar'}]}));
  await page.route('**/api/servers/*/vs/*/files/**', async route => {
    const url = new URL(route.request().url()); const method = route.request().method();
    if (url.pathname.endsWith('/summary')) return route.fulfill({json:{summaries:[],scannedAt:Date.now(),deliveredAt:Date.now()}});
    if (url.pathname.endsWith('/mkdir')) {
      if (rejectMkdir) return route.fulfill({status:400,json:{error:'The server rejected this file path',reason:'file_path_invalid'}});
      const path = route.request().postDataJSON().dirname; sent.push(path);
      const name = path.replace('Türkçe klasör','Turkce klasor'); folders.add(name);
      return route.fulfill({status:201,json:{success:true,name,renamed:name!==path}});
    }
    if (url.pathname.endsWith('/upload')) {
      expect(route.request().postData()).toContain('/Turkce klasor');
      files.set('/Turkce klasor/deneme.txt','fixture-bytes');
      return route.fulfill({status:201,json:{success:true,name:'/Turkce klasor/deneme.txt',size:13}});
    }
    if (url.pathname.endsWith('/download')) return route.fulfill({body:'fixture-bytes',contentType:'application/octet-stream'});
    if (method === 'DELETE') { files.delete(route.request().postDataJSON().name); return route.fulfill({json:{success:true}}); }
    const path = url.searchParams.get('path') || '/';
    const result = [...folders].filter(n => n.slice(0,n.lastIndexOf('/')) === (path === '/' ? '' : path)).map(n=>({name:n.split('/').pop(),type:'0',size:'0',datetime:'1792610264504'}));
    result.push(...[...files].filter(([n])=>n.slice(0,n.lastIndexOf('/'))===path).map(([n,content])=>({name:n.split('/').pop(),type:'1',size:String(content.length),datetime:'1792610264504'})));
    return route.fulfill({json:result});
  });
  await page.goto('/login'); await page.getByLabel('Kullanıcı adı',{exact:true}).fill('admin'); await page.getByLabel('Parola',{exact:true}).fill('test-password');
  await page.getByRole('button',{name:'Giriş yap',exact:true}).click(); await expect(page).toHaveURL('/dashboard');
  await page.goto('/files'); await page.getByRole('button',{name:/Dosyalar/}).click();
  await expect(page.getByText('Boş klasör',{exact:true})).toBeVisible();
  for (const term of ['Not checked.', 'Empty directory','No files in this directory.','Size','Modified','item(s)']) await expect(page.getByText(term,{exact:true})).toHaveCount(0);
  await page.getByRole('button',{name:'Yeni klasör',exact:true}).click(); await page.getByRole('dialog').getByRole('textbox').fill('Türkçe klasör');
  await page.getByRole('button',{name:'Oluştur',exact:true}).click(); await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByText('Turkce klasor',{exact:true})).toBeVisible();
  await page.getByText('Turkce klasor',{exact:true}).click();
  await page.getByLabel('Yüklenecek dosyayı seçin',{exact:true}).setInputFiles({name:'deneme.txt',mimeType:'text/plain',buffer:Buffer.from('fixture-bytes')});
  await expect(page.getByText('deneme.txt',{exact:true})).toBeVisible();
  const downloaded = page.waitForEvent('download'); await page.getByRole('button',{name:'İndir deneme.txt',exact:true}).click();
  expect((await downloaded).suggestedFilename()).toBe('deneme.txt');
  await page.getByRole('button',{name:'Sil deneme.txt',exact:true}).click(); await page.getByRole('dialog').getByRole('button',{name:'Sil',exact:true}).click();
  await expect(page.getByText('deneme.txt',{exact:true})).toHaveCount(0);
  await page.getByRole('button',{name:'Üst klasöre git',exact:true}).click();
  rejectMkdir = true;
  await page.getByRole('button',{name:'Yeni klasör',exact:true}).click(); await page.getByRole('dialog').getByRole('textbox').fill('reddedilen'); await page.getByRole('button',{name:'Oluştur',exact:true}).click();
  await expect(page.getByText('Sunucu bu dosya yolunu kabul etmedi',{exact:true})).toBeVisible();
  await expect(page.getByRole('dialog')).toBeVisible(); await page.getByRole('button',{name:'İptal',exact:true}).click();
  await page.getByRole('button',{name:'İngilizce',exact:true}).click(); await expect(page.getByRole('button',{name:'Upload file',exact:true})).toBeVisible();
  await page.getByRole('button',{name:'Türkçe',exact:true}).click(); await page.setViewportSize({width:390,height:844});
  await expect(page.getByRole('button',{name:'Dosya yükle',exact:true})).toBeVisible();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  expect(sent).toEqual(['/Türkçe klasör']);
});
