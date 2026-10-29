# Spotify araması / Spotify search

`!ara <şarkı>` her zaman YouTube Music ve YouTube'da arar. Spotify geliştirici
bilgileri girilirse Spotify kataloğunda da arar; bu sonuçlar `[Spotify]` diye
listelenir. Numara seçildiğinde şarkı adı ve sanatçı Spotify'dan alınır, ses
YouTube'da eşleştirilip çalınır. Spotify sesi şifreli (DRM) olduğu için bot
Spotify'dan ses indirmez; YouTube çerezi bu yüzden yine gereklidir.

Bilgiler girilmezse `!ara` eskisi gibi çalışır: üçüncü grup, YouTube'da
"spotify official audio" diye yapılan bir aramadır ve `[YouTube]` olarak görünür.

## Bilgileri almak

1. <https://developer.spotify.com/dashboard> adresine Spotify hesabınızla girin.
2. **Create app** deyin. Ad ve açıklama serbest; Redirect URI için `http://127.0.0.1` yazın,
   **Web API** kutusunu işaretleyin ve kaydedin.
3. Uygulamanın **Settings** sayfasından **Client ID** ve **Client secret** değerlerini alın.

## Sunucuya eklemek

Kurulum klasöründeki `.env` dosyasına (canlı kurulumda `/opt/teamhub/.env`) ekleyin:

```dotenv
SPOTIFY_CLIENT_ID=...
SPOTIFY_CLIENT_SECRET=...
SPOTIFY_MARKET=TR
```

Ardından yalnızca Manager servisini yeniden oluşturun:

```bash
docker compose --env-file .env -f compose.yml --profile manager up -d --no-deps --force-recreate manager
```

Client secret bir paroladır: Git'e eklemeyin, sohbete veya kayıtlara yapıştırmayın.
Sızdığını düşünürseniz Spotify panelinden yenileyin.

## English

`!ara` always searches YouTube Music and YouTube. With `SPOTIFY_CLIENT_ID` and
`SPOTIFY_CLIENT_SECRET` (a free Spotify developer app, client-credentials flow) it also
searches the Spotify catalogue; chosen Spotify results are matched and played from
YouTube, so YouTube cookies are still required. `SPOTIFY_MARKET` (default `TR`) sets the
catalogue country. Without credentials the previous YouTube-only behaviour is kept.
Keep the client secret private and out of Git.
