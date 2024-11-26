use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use kv::{Bucket, Codec, Store};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

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

#[derive(Clone)]
struct AppState<'a> {
    db: Bucket<'a, kv::Integer, kv::Json<Song>>,
    is_dirty: Arc<Mutex<bool>>,
}

#[tokio::main]
async fn main() {
    let user_count = Arc::new(Mutex::new(0));
    let is_dirty = Arc::new(Mutex::new(false));
    let cfg = kv::Config::new("./song_db");
    let store = Store::new(cfg).unwrap();
    let db = store
        .bucket::<kv::Integer, kv::Json<Song>>(Some("songs"))
        .unwrap();
    let state = AppState {
        db,
        is_dirty: Arc::clone(&is_dirty),
    };

    tokio::spawn(async move {
        let db = store
            .bucket::<kv::Integer, kv::Json<Song>>(Some("songs"))
            .unwrap();

        loop {
            let mut flush = false;
            {
                let mut dirty = is_dirty.lock().await;
                if *dirty {
                    flush = true;
                    *dirty = false;
                }
            }

            if flush {
                db.flush_async().await.unwrap();
            }
        }
    });

    let app = Router::new()
        .route(
            "/",
            get(|| async { "Welcome to the Rust-powered web server!" }),
        )
        .route(
            "/count",
            get(|| async move {
                let mut user_count = user_count.lock().await;
                *user_count += 1;
                format!("Visit count: {}", *user_count)
            }),
        )
        .route("/songs/new", post(add_song))
        .route("/songs/search", get(search_song))
        .route("/songs/play/:id", get(play_song))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("The server is currently listening on localhost:8080.");
    axum::serve(listener, app).await.unwrap();
}

async fn add_song(State(state): State<AppState<'_>>, Json(payload): Json<Song>) -> Json<Song> {
    let db = &state.db;

    let id = db.len() + 1;
    let song = kv::Json(Song { id, ..payload });

    db.set(&kv::Integer::from(id), &song).unwrap();

    let mut dirty = state.is_dirty.lock().await;
    *dirty = true;

    Json(song.into_inner())
}

async fn search_song(
    State(state): State<AppState<'_>>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<Song>> {
    let db = &state.db;

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
    State(state): State<AppState<'_>>,
    Path(params): Path<HashMap<String, String>>,
) -> impl IntoResponse {
    const ERROR_JSON: Json<SongNotFound> = Json(SongNotFound {
        error: "Song not found",
    });

    let db = &state.db;

    let id = match params.get("id") {
        Some(id) => id.parse::<usize>().unwrap(),
        None => return (StatusCode::NOT_FOUND, ERROR_JSON.into_response()),
    };

    let mut song = match db.get(&kv::Integer::from(id)) {
        Ok(item) => match item {
            Some(song) => song,
            None => return (StatusCode::NOT_FOUND, ERROR_JSON.into_response()),
        },
        Err(_) => return (StatusCode::NOT_FOUND, ERROR_JSON.into_response()),
    };

    song.0.play_count += 1;

    db.set(&kv::Integer::from(id), &song).unwrap();
    let mut is_dirty = state.is_dirty.lock().await;
    *is_dirty = true;

    (StatusCode::OK, Json(song.into_inner()).into_response())
}
