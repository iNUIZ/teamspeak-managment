//! Print the current title of a radio stream.
//!
//! cargo run -p teamhub-media --example icy_title -- <url>

#[tokio::main]
async fn main() {
    let url = std::env::args().nth(1).expect("usage: icy_title <url>");
    println!("{:?}", teamhub_media::icy::fetch_title(&url).await);
}
