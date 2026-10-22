import { hasUiTranslation, translate, useLanguageStore } from './i18n';

const known: Record<string,string> = {
  b_virtualserver_info_view:'Sunucu bilgilerini görüntüleme', b_virtualserver_connectioninfo_view:'Sunucu bağlantı bilgilerini görüntüleme',
  b_virtualserver_channel_list:'Kanalları listeleme', b_virtualserver_client_list:'Kullanıcıları listeleme',
  b_virtualserver_modify_name:'Sunucu adını değiştirme', b_virtualserver_modify_password:'Sunucu parolasını değiştirme',
  b_virtualserver_modify_maxclients:'Sunucu kapasitesini değiştirme', b_virtualserver_modify_hostmessage:'Sunucu karşılama mesajını değiştirme',
  b_virtualserver_modify_hostbanner:'Sunucu afişini değiştirme', b_virtualserver_modify_hostbutton:'Sunucu düğmesini değiştirme',
  b_virtualserver_start:'TeamSpeak sunucusunu başlatma', b_virtualserver_stop:'TeamSpeak sunucusunu durdurma',
  b_virtualserver_create:'TeamSpeak sunucusu oluşturma', b_virtualserver_delete:'TeamSpeak sunucusunu silme',
  b_virtualserver_snapshot_create:'Yapılandırma yedeği oluşturma', b_virtualserver_snapshot_deploy:'Yapılandırma yedeğini geri yükleme',
  b_virtualserver_token_add:'Yetki anahtarı oluşturma', b_virtualserver_token_list:'Yetki anahtarlarını listeleme', b_virtualserver_token_delete:'Yetki anahtarını silme',
  b_virtualserver_ban_list:'Yasakları listeleme', b_virtualserver_ban_add:'Yasak ekleme', b_virtualserver_ban_delete:'Yasak kaldırma',
  b_virtualserver_log_view:'Sunucu günlüklerini görüntüleme', b_virtualserver_notify_register:'Sunucu olaylarına abone olma',
  b_channel_create_permanent:'Kalıcı kanal oluşturma', b_channel_create_semi_permanent:'Yarı kalıcı kanal oluşturma', b_channel_create_temporary:'Geçici kanal oluşturma',
  b_channel_modify_name:'Kanal adını değiştirme', b_channel_modify_topic:'Kanal konusunu değiştirme', b_channel_modify_description:'Kanal açıklamasını değiştirme',
  b_channel_modify_password:'Kanal parolasını değiştirme', b_channel_modify_maxclients:'Kanal kapasitesini değiştirme',
  b_channel_join_permanent:'Kalıcı kanala katılma', b_channel_join_temporary:'Geçici kanala katılma', b_channel_join_ignore_password:'Parola olmadan kanala katılma',
  b_channel_delete_permanent:'Kalıcı kanal silme', b_channel_delete_temporary:'Geçici kanal silme',
  b_client_server_textmessage_send:'Sunucuya mesaj gönderme', b_client_channel_textmessage_send:'Kanala mesaj gönderme', b_client_offline_textmessage_send:'Çevrimdışı mesaj gönderme',
  b_client_ignore_bans:'Yasakları atlama', b_client_skip_channelgroup_permissions:'Kanal grubu yetkilerini atlama',
  i_client_kick_from_server_power:'Sunucudan çıkarma gücü', i_client_needed_kick_from_server_power:'Sunucudan çıkarılmak için gereken güç',
  i_client_kick_from_channel_power:'Kanaldan çıkarma gücü', i_client_needed_kick_from_channel_power:'Kanaldan çıkarılmak için gereken güç',
  i_client_ban_power:'Kullanıcı yasaklama gücü', i_client_needed_ban_power:'Yasaklanmak için gereken güç',
  i_client_move_power:'Kullanıcı taşıma gücü', i_client_needed_move_power:'Taşınmak için gereken güç',
  i_client_poke_power:'Dürtme gücü', i_client_needed_poke_power:'Dürtülmek için gereken güç',
  i_client_private_textmessage_power:'Özel mesaj gönderme gücü', i_client_needed_private_textmessage_power:'Özel mesaj almak için gereken güç',
  i_client_talk_power:'Konuşma gücü', i_client_needed_talk_power:'Konuşmak için gereken güç',
  i_ft_file_upload_power:'Dosya yükleme gücü', i_ft_needed_file_upload_power:'Dosya yüklemek için gereken güç',
  i_ft_file_download_power:'Dosya indirme gücü', i_ft_needed_file_download_power:'Dosya indirmek için gereken güç',
  i_ft_file_browse_power:'Dosya listeleme gücü', i_ft_needed_file_browse_power:'Dosya listelemek için gereken güç',
  i_ft_file_delete_power:'Dosya silme gücü', i_ft_needed_file_delete_power:'Dosya silmek için gereken güç',
  i_ft_directory_create_power:'Klasör oluşturma gücü', i_ft_needed_directory_create_power:'Klasör oluşturmak için gereken güç',
  i_group_modify_power:'Grup değiştirme gücü', i_group_needed_modify_power:'Grup değiştirmek için gereken güç',
  i_group_member_add_power:'Gruba üye ekleme gücü', i_group_needed_member_add_power:'Gruba üye eklemek için gereken güç',
  i_group_member_remove_power:'Gruptan üye çıkarma gücü', i_group_needed_member_remove_power:'Gruptan üye çıkarmak için gereken güç',
  i_icon_id:'Simge kimliği', b_icon_manage:'Simgeleri yönetme', i_max_icon_filesize:'En büyük simge boyutu',
};

const words:Record<string,string>={
  virtualserver:'TeamSpeak sunucusu',serverinstance:'Sunucu hizmeti',serverquery:'Sunucu sorgusu',channel:'Kanal',client:'Kullanıcı',group:'Grup',
  ft:'Dosya aktarımı',file:'Dosya',directory:'Klasör',server:'Sunucu',instance:'Hizmet',query:'Sorgu',
  create:'Oluşturma',delete:'Silme',modify:'Değiştirme',view:'Görüntüleme',list:'Listeleme',add:'Ekleme',remove:'Kaldırma',
  upload:'Yükleme',download:'İndirme',browse:'Listeleme',rename:'Yeniden adlandırma',transfer:'Aktarma',copy:'Kopyalama',
  needed:'Gereken',power:'Yetki gücü',max:'En fazla',min:'En az',total:'Toplam',size:'Boyut',filesize:'Dosya boyutu',
  bandwidth:'Bant genişliği',limit:'Sınır',limits:'Sınırlar',quota:'Kota',time:'Süre',duration:'Süre',count:'Sayı',
  name:'Ad',topic:'Konu',description:'Açıklama',password:'Parola',icon:'Simge',id:'Kimlik',uid:'Benzersiz kimlik',
  info:'Bilgi',connectioninfo:'Bağlantı bilgisi',connection:'Bağlantı',version:'Sürüm',log:'Günlük',read:'Okuma',write:'Yazma',
  permission:'Yetki',permissions:'Yetkiler',permissionlist:'Yetki listesi',permlist:'Yetki listesi',perm:'Yetki',
  servergroup:'Sunucu grubu',channelgroup:'Kanal grubu',servergroups:'Sunucu grupları',channelgroups:'Kanal grupları',
  permanent:'Kalıcı',temporary:'Geçici',semi:'Yarı',default:'Varsayılan',template:'Şablon',admin:'Yönetici',guest:'Misafir',
  talk:'Konuşma',talker:'Konuşmacı',speaker:'Konuşmacı',priority:'Öncelikli',whisper:'Fısıltı',whisperlist:'Fısıltı listesi',
  voice:'Ses',codec:'Ses biçimi',quality:'Kalite',encryption:'Şifreleme',security:'Güvenlik',
  kick:'Çıkarma',ban:'Yasaklama',move:'Taşıma',poke:'Dürtme',complaint:'Şikâyet',complaints:'Şikâyetler',
  send:'Gönderme',receive:'Alma',textmessage:'Metin mesajı',message:'Mesaj',private:'Özel',offline:'Çevrimdışı',
  join:'Katılma',subscribe:'Abonelik',unsubscribe:'Abonelikten çıkma',member:'Üye',members:'Üyeler',
  from:'Kaynağından',to:'Hedefine',ignore:'Yok sayma',skip:'Atlama',negate:'Tersleme',grant:'Atama',
  maxclients:'Kullanıcı sınırı',maxfamilyclients:'Kanal ailesi kullanıcı sınırı',unlimited:'Sınırsız',
  channelneededtalkpower:'Konuşmak için gereken güç',reserved:'Ayrılmış',slots:'Kontenjan',
  hostmessage:'Karşılama mesajı',hostbanner:'Sunucu afişi',hostbutton:'Sunucu düğmesi',
  token:'Yetki anahtarı',snapshot:'Yapılandırma yedeği',deploy:'Geri yükleme',start:'Başlatma',stop:'Durdurma',
  notify:'Olay bildirimi',register:'Kaydetme',unregister:'Kaydı kaldırma',notifyregister:'Olay kaydı',
  avatar:'Kullanıcı görseli',away:'Uzakta',idle:'Hareketsiz',is:'Olma',can:'İzin',own:'Kendisine ait',
  global:'Genel',local:'Yerel',all:'Tümü',other:'Diğer',others:'Diğerleri',
  phonetic:'Sesli okunuş',nickname:'Kullanıcı adı',unique:'Benzersiz',identifier:'Kimlik',
  latency:'Gecikme',packetloss:'Paket kaybı',flood:'İstek yoğunluğu',antiflood:'Yoğunluk koruması',
  welcome:'Karşılama',banner:'Afiş',button:'Düğme',url:'Bağlantı',host:'Ana makine',port:'Port',
  db:'Veritabanı',database:'Veritabanı',edit:'Düzenleme',force:'Zorla',event:'Olay',events:'Olaylar',
  retrieval:'Alma',interval:'Aralık',passworded:'Parolalı',order:'Sıra',sortorder:'Sıralama',parent:'Üst kanal',
};

/** Native descriptions stay untouched in EN. Unknown extensions retain an honest technical identity. */
export function permissionLabel(permission:{permid:number;permsid:string;permdesc:string}):string {
  if(useLanguageStore.getState().language==='tr') {
    if(known[permission.permsid])return known[permission.permsid];
    if(permission.permdesc&&hasUiTranslation(permission.permdesc))return translate(permission.permdesc);
    const tokens=permission.permsid.replace(/^[bi]_/, '').split('_');
    if(tokens.length>1&&tokens.every(token=>words[token]))return tokens.map(token=>words[token]).join(' · ');
    return translate('Permission #{0}',permission.permid);
  }
  const description=permission.permdesc.trim();
  if(description.length>=3&&description.toLowerCase()!==permission.permsid.toLowerCase())return description;
  const readable=permission.permsid.replace(/^[bi]_/, '').replace(/_/g, ' ').trim();
  return readable?readable.charAt(0).toUpperCase()+readable.slice(1):permission.permsid;
}
