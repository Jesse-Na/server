use std::sync::{Arc, Mutex};

use axum::{
    debug_handler,
    routing::{get, post}, Extension, Json, Router
};
use kv::Store;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Song {
    #[serde(default)]
    id: usize,

    title: String,
    artist: String,
    genre: String,

    #[serde(default)]
    play_count: u32,
}

#[tokio::main]
async fn main() {
    let user_count = Arc::new(Mutex::new(0));
    let cfg = kv::Config::new("./song_db");
    let store = Store::new(cfg).unwrap();

    let app = Router::new()
        .route("/", get(|| async { "Welcome to the Rust-powered web server!" }))
        .route("/count", get(|| async move {
            let mut user_count = user_count.lock().unwrap();
            *user_count += 1;
            format!("Visit count: {}", *user_count)
        }))
        .route("/songs/new", post(add_song))
        // .route("/songs/search", get(|| async {
        //     "Search for songs"
        // }))
        .layer(Extension(store));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("The server is currently listening on localhost:8080.");
    axum::serve(listener, app).await.unwrap();
}

#[debug_handler]
async fn add_song(Extension(store): Extension<Store>, Json(payload): Json<Song>, ) -> String {
    dbg!(&payload);

    let db = store.bucket::<kv::Integer, kv::Json<Song>>(Some("songs")).unwrap();

    let id = db.len() + 1;

    let song = Song {
        id,
        ..payload
    };

    db.set(&kv::Integer::from(id), &kv::Json(song.clone())).unwrap();

    db.flush_async().await.unwrap();

    for item in db.iter() {
        let item = item.unwrap();
        let key: kv::Integer = item.key().unwrap();
        let value = item.value::<kv::Json<Song>>().unwrap();
        println!("key: {}, value: {}", usize::from(key), value);
    }

    serde_json::to_string(&song).unwrap()
}