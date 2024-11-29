use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{migrate::MigrateDatabase, prelude::FromRow, Sqlite, SqlitePool};
use tokio::sync::Mutex;

const DB_URL: &str = "sqlite://songs.db";

#[derive(Clone, FromRow, Debug, Serialize, Deserialize)]
struct Song {
    #[serde(default)]
    id: i64,

    title: String,
    artist: String,
    genre: String,

    #[serde(default)]
    play_count: i64,
}

#[derive(Serialize)]
struct SongNotFound<'a> {
    error: &'a str,
}

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
}

#[tokio::main]
async fn main() {
    let user_count = Arc::new(Mutex::new(0));

    if !Sqlite::database_exists(DB_URL).await.unwrap_or(false) {
        match Sqlite::create_database(DB_URL).await {
            Ok(_) => {}
            Err(error) => panic!("error: {}", error),
        }
    }

    let db = SqlitePool::connect(DB_URL)
        .await
        .expect("Failed to connect to database");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS songs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        title VARCHAR(250) NOT NULL,
        artist VARCHAR(250) NOT NULL,
        genre VARCHAR(250) NOT NULL,
        play_count INTEGER DEFAULT 0);",
    )
    .execute(&db)
    .await
    .expect("Failed to create table");

    let state = AppState { db };

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

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("Unable to bind to port 8080");
    println!("The server is currently listening on localhost:8080.");
    axum::serve(listener, app)
        .await
        .expect("Infallible server error");
}

#[axum::debug_handler]
async fn add_song(State(state): State<AppState>, Json(payload): Json<Song>) -> Json<Song> {
    let result = sqlx::query("INSERT INTO songs (title, artist, genre) VALUES (?, ?, ?)")
        .bind(&payload.title)
        .bind(&payload.artist)
        .bind(&payload.genre)
        .execute(&state.db)
        .await
        .expect("Failed to insert song");

    Json(Song {
        id: result.last_insert_rowid(),
        ..payload
    })
}

async fn search_song(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<Song>> {
    let mut query_builder = vec![String::from("SELECT * FROM songs ")];

    for (key, value) in params {
        if key != "title" && key != "artist" && key != "genre" {
            continue;
        }

        if query_builder.len() == 1 {
            query_builder.push(String::from("WHERE "));
        } else {
            query_builder.push(String::from("AND "));
        }

        query_builder.push(format!("{} LIKE '%{}%' ", key, value));
    }

    let query = query_builder.join("");

    let song_results = sqlx::query_as::<_, Song>(&query)
        .fetch_all(&state.db)
        .await
        .expect("Failed to fetch songs");

    Json(song_results)
}

async fn play_song(
    State(state): State<AppState>,
    Path(params): Path<HashMap<String, String>>,
) -> impl IntoResponse {
    const ERROR_JSON: Json<SongNotFound> = Json(SongNotFound {
        error: "Song not found",
    });

    let song_id = match params.get("id") {
        Some(id) => match id.parse::<i64>() {
            Ok(id) => id,
            Err(_) => return (StatusCode::BAD_REQUEST, ERROR_JSON.into_response()),
        },
        None => return (StatusCode::BAD_REQUEST, ERROR_JSON.into_response()),
    };

    let song = sqlx::query_as::<_, Song>("SELECT * FROM songs WHERE id = ?")
        .bind(song_id)
        .fetch_one(&state.db)
        .await
        .ok();

    let mut song = match song {
        Some(song) => song,
        None => return (StatusCode::NOT_FOUND, ERROR_JSON.into_response()),
    };

    song.play_count += 1;

    sqlx::query("UPDATE songs SET play_count = ? WHERE id = ?")
        .bind(song.play_count)
        .bind(song_id)
        .execute(&state.db)
        .await
        .expect("Failed to update play count");

    (StatusCode::OK, Json(song).into_response())
}
