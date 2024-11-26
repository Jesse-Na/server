use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use kv::{Codec, Store};
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

#[derive(Serialize)]
struct SongNotFound<'a> {
    error: &'a str,
}

#[tokio::main]
async fn main() {
    let user_count = Arc::new(Mutex::new(0));
    let cfg = kv::Config::new("./song_db");
    let store = Store::new(cfg).unwrap();

    let app = Router::new()
        .route(
            "/",
            get(|| async { "Welcome to the Rust-powered web server!" }),
        )
        .route(
            "/count",
            get(|| async move {
                let mut user_count = user_count.lock().unwrap();
                *user_count += 1;
                format!("Visit count: {}", *user_count)
            }),
        )
        .route("/songs/new", post(add_song))
        .route("/songs/search", get(search_song))
        .route("/songs/play/:id", get(play_song))
        .layer(Extension(store));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("The server is currently listening on localhost:8080.");
    axum::serve(listener, app).await.unwrap();
}

async fn add_song(Extension(store): Extension<Store>, Json(payload): Json<Song>) -> Json<Song> {
    let db = store
        .bucket::<kv::Integer, kv::Json<Song>>(Some("songs"))
        .unwrap();

    let id = db.len() + 1;
    let song = Song { id, ..payload };

    db.set(&kv::Integer::from(id), &kv::Json(song.clone()))
        .unwrap();
    db.flush_async().await.unwrap();

    Json(song)
}

async fn search_song(
    Extension(store): Extension<Store>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<Song>> {
    let db = store
        .bucket::<kv::Integer, kv::Json<Song>>(Some("songs"))
        .unwrap();

    let songs = db
        .iter()
        .filter_map(|item| {
            let song = match item {
                Ok(item) => match item.value::<kv::Json<Song>>() {
                    Ok(song) => song.into_inner(),
                    Err(_) => return None,
                },
                Err(_) => return None,
            };

            for (key, value) in &params {
                match key.as_str() {
                    "title" => {
                        if !song
                            .title
                            .to_lowercase()
                            .contains(value.to_lowercase().as_str())
                        {
                            return None;
                        }
                    }
                    "artist" => {
                        if !song
                            .artist
                            .to_lowercase()
                            .contains(value.to_lowercase().as_str())
                        {
                            return None;
                        }
                    }
                    "genre" => {
                        if !song
                            .genre
                            .to_lowercase()
                            .contains(value.to_lowercase().as_str())
                        {
                            return None;
                        }
                    }
                    _ => {}
                }
            }

            Some(song)
        })
        .collect::<Vec<Song>>();

    Json(songs)
}

async fn play_song(
    Extension(store): Extension<Store>,
    Path(params): Path<HashMap<String, String>>,
) -> impl IntoResponse {
    const ERROR_JSON: Json<SongNotFound> = Json(SongNotFound {
        error: "Song not found",
    });

    let db = store
        .bucket::<kv::Integer, kv::Json<Song>>(Some("songs"))
        .unwrap();

    let id = match params.get("id") {
        Some(id) => id.parse::<usize>().unwrap(),
        None => return (StatusCode::NOT_FOUND, ERROR_JSON.into_response()),
    };

    let mut song = match db.get(&kv::Integer::from(id)) {
        Ok(item) => match item {
            Some(song) => song.into_inner(),
            None => return (StatusCode::NOT_FOUND, ERROR_JSON.into_response()),
        },
        Err(_) => return (StatusCode::NOT_FOUND, ERROR_JSON.into_response()),
    };

    song.play_count += 1;

    db.set(&kv::Integer::from(id), &kv::Json(song.clone()))
        .unwrap();
    db.flush_async().await.unwrap();

    (StatusCode::OK, Json(song).into_response())
}
