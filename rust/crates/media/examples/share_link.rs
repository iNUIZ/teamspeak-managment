//! Print the tracks behind a Spotify or Apple Music link.
//!
//! cargo run -p teamhub-media --example share_link -- <url>

use teamhub_media::links;

#[tokio::main]
async fn main() {
    let url = std::env::args().nth(1).expect("usage: share_link <url>");
    let found = if links::is_spotify_share_url(&url) { links::resolve_spotify(&url).await } else { links::resolve_apple_music(&url).await };
    match found {
        Ok(found) => {
            println!("title: {:?}, tracks: {}", found.title, found.tracks.len());
            for track in found.tracks.iter().take(3) {
                println!("  {:?}", track.query());
            }
        }
        Err(error) => println!("error: {error}"),
    }
}
