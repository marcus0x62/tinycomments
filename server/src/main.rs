/*
 * Copyright (c) 2024 Marcus Butler
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

use actix_web::{
    get, http::header::ContentType, post, web, App, HttpRequest, HttpResponse, HttpServer,
};
use base64::prelude::*;
use chrono::DateTime;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use sqlite::Value::Null;
use std::fs::File;
use std::io::prelude::*;
use std::str;
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod config;
mod email;
mod pow;

struct AppState {
    config: config::ConfigFile,
    db_conn: Mutex<sqlite::Connection>,
    pow: pow::PowTable,
}

#[derive(Serialize, Deserialize)]
struct IdRequest {
    name: String,
    email: String,
    challenge: Option<String>,
    secret: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct IdResponse {
    commenter_id: String,
    code: u16,
    status: String,
    challenge: Option<String>,
    key: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct GetCommentsRequest {
    commenter_id: String,
    article: String,
    challenge: Option<String>,
    secret: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct GetCommentsResponse {
    code: u16,
    status: String,
    comments: Vec<Comment>,
    challenge: Option<String>,
    key: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Comment {
    id: i64,
    timestamp: i64,
    parent: i64,
    poster_name: String,
    comment: String,
    votes: i64,
    myvote: i64,
}

#[derive(Deserialize)]
struct NewCommentRequest {
    article: String,
    commenter_id: String,
    comment: String,
    parent: i64,
    challenge: Option<String>,
    secret: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct NewCommentResponse {
    code: u16,
    status: String,
    challenge: Option<String>,
    key: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct VoteRequest {
    voter_id: String,
    comment_id: i64,
    vote: i64,
    challenge: Option<String>,
    secret: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct VoteResponse {
    code: u16,
    status: String,
    challenge: Option<String>,
    key: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct GetPowResponse {
    code: u16,
    key: String,
    challenge: String,
}

#[derive(Serialize, Deserialize)]
struct ValidatePowRequest {
    challenge: String,
    secret: String,
}

#[derive(Serialize, Deserialize)]
struct ValidatePowResponse {
    code: u16,
    status: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = match config::ConfigFile::new_from_file("config.toml") {
        Ok(config) => config,
        Err(e) => panic!("Unable to read config file: {e}"),
    };

    let tracing_level = match config.debug {
        config::DebugLevel::Info => Level::INFO,
        config::DebugLevel::Debug => Level::DEBUG,
        config::DebugLevel::Trace => Level::TRACE,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing_level)
        .without_time()
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Could not set default global tracing subscriber");

    info!("Starting tracing log for Tinycomments");

    let db_conn = Mutex::new(sqlite::open(&config.db_path).unwrap());

    match db_conn.lock() {
        Ok(conn) => {
            let mut statement = conn.prepare("PRAGMA foreign_keys = ON;").unwrap();
            if let Err(e) = statement.next() {
                panic!("Could not enable foreign key support: {e:?}");
            }
        }
        Err(e) => {
            panic!("Could not get DB lock: {e:?}");
        }
    }

    let bind_addr = config.bind_address.clone();
    let bind_port = config.bind_port;

    let state = web::Data::new(AppState {
        config,
        db_conn,
        pow: pow::PowTable::new(),
    });

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(id)
            .service(post_comment)
            .service(get_comments)
            .service(vote)
            .service(get_root)
            .service(get_pow)
            .service(validate_pow)
    })
    .bind((bind_addr, bind_port))?
    .run()
    .await
}

#[get("/")]
async fn get_root(_state: web::Data<AppState>) -> HttpResponse {
    let mut handle = File::open("comments.html").expect("Unable to open file");
    let mut contents = String::new();
    let _ = handle.read_to_string(&mut contents);

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(contents)
}

#[post("/id/")]
async fn id(
    data: web::Form<IdRequest>,
    state: web::Data<AppState>,
    req: HttpRequest,
) -> web::Json<IdResponse> {
    let query = r#"INSERT INTO ids VALUES (?, ?, ?);"#;

    let clean_name = ammonia::clean(&data.name[..]);
    let clean_email = ammonia::clean(&data.email[..]);

    let mut response = IdResponse {
        code: 200,
        status: String::from("OK"),
        commenter_id: String::from(""),
        challenge: None,
        key: None,
    };

    if let Some(result) = state.pow.handle(&get_client_ip(&req), &data.challenge, &data.secret) {
        response.code = result.code;
        response.status = result.status.unwrap_or(String::from(""));
        response.challenge = result.challenge;
        response.key = result.key;

        return web::Json(response);
    }

    if let Ok(t) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        let client_ip = get_client_ip(&req);

        let mut rand_bytes = [0u8; 32];
        thread_rng().fill(&mut rand_bytes);

        let commenter_id = hex::encode(rand_bytes);

        info!(
            "{} Generating new ID '{}' for name: '{}' email: '{}' for client {}",
            DateTime::from_timestamp(t.as_secs() as i64, 0).unwrap(),
            commenter_id,
            clean_name,
            clean_email,
            client_ip
        );

        match state.db_conn.lock() {
            Ok(conn) => {
                let mut statement = conn.prepare(query).unwrap();
                statement.bind((1, &commenter_id[..])).unwrap();
                statement.bind((2, &clean_name[..])).unwrap();
                statement.bind((3, &clean_email[..])).unwrap();

                if let Err(e) = statement.next() {
                    response.code = 500;
                    response.status = format!("Could not insert new ID: {e}");

                    web::Json(response)
                } else {
                    response.commenter_id = commenter_id;
                    web::Json(response)
                }
            }
            Err(e) => {
                response.code = 500;
                response.status = format!("DB Error: {:?}", e);

                web::Json(response)
            }
        }
    } else {
        response.code = 500;
        response.status = String::from("Could not generate timestamp");

        web::Json(response)
    }
}

#[post("/comment/post/")]
async fn post_comment(
    data: web::Form<NewCommentRequest>,
    state: web::Data<AppState>,
    req: HttpRequest,
) -> web::Json<NewCommentResponse> {
    let query = r#"INSERT INTO comments (article, commenter_id, parent, comment, moderated, timestamp)
                                        VALUES(?, ?, ?, ?, true, ?);"#;

    let mut response = NewCommentResponse {
        code: 200,
        status: String::from("OK"),
        challenge: None,
        key: None,
    };

    if let Some(result) = state.pow.handle(&get_client_ip(&req), &data.challenge, &data.secret) {
        response.code = result.code;
        response.status = result.status.unwrap_or(String::from(""));
        response.challenge = result.challenge;
        response.key = result.key;

        return web::Json(response);
    }

    let commenter_id = &ammonia::clean(&data.commenter_id[..])[..];
    let clean_comment_text = &ammonia::clean_text(&data.comment[..])[..];

    if let Ok(t) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        let client_ip = get_client_ip(&req);

        let decoded_article: String;
        if let Some(decode) = base64_decode(data.article.clone()) {
            decoded_article = decode;
        } else {
            response.code = 500;
            response.status = format!("Could not base64 decode '{}'", data.article);
            return web::Json(response);
        }

        info!(
            "{} Posting comment for '{}' for client {} with id '{}'",
            DateTime::from_timestamp(t.as_secs() as i64, 0).unwrap(),
            decoded_article,
            client_ip,
            commenter_id,
        );

        match state.db_conn.lock() {
            Ok(conn) => {
                let mut statement = conn.prepare(query).unwrap();
                statement
                    .bind((1, &ammonia::clean(&data.article[..])[..]))
                    .unwrap();
                statement.bind((2, commenter_id)).unwrap();

                if data.parent == 0 {
                    statement.bind((3, Null)).unwrap();
                } else {
                    statement.bind((3, data.parent)).unwrap();
                }

                statement.bind((4, clean_comment_text)).unwrap();
                statement.bind((5, t.as_secs() as i64)).unwrap();

                if let Err(e) = statement.next() {
                    response.code = 500;
                    response.status = format!("Could not add comment: {e}");
                    web::Json(response)
                } else {
                    if state.config.enable_email_notifications {
                        if let Some((name, _email)) = get_commenter_info(&conn, commenter_id) {
                            let _ = email::send_email(
                                &state,
                                &decoded_article,
                                &name,
                                clean_comment_text,
                            );
                        } else {
                            info!("Unable to send notification email");
                        }
                    }
                    web::Json(response)
                }
            }
            Err(e) => {
                response.code = 500;
                response.status = format!("DB Error: {:?}", e);
                web::Json(response)
            }
        }
    } else {
        response.code = 500;
        response.status = String::from("Could not generate timestamp");
        web::Json(response)
    }
}

#[post("/comment/get/")]
async fn get_comments(
    data: web::Form<GetCommentsRequest>,
    state: web::Data<AppState>,
    req: HttpRequest,
) -> web::Json<GetCommentsResponse> {
    let query = r#"SELECT id, parent, ids.name AS poster_name, timestamp, comment, COALESCE(SUM(v1.vote),0) + 1 AS votes,
                          COALESCE((SELECT v2.vote FROM votes v2 WHERE v2.voter_id = ? AND v2.comment_id = id), 0) AS myvote
                          FROM comments
                          LEFT JOIN ids on comments.commenter_id = ids.commenter_id
                          LEFT JOIN votes v1 on comments.id = v1.comment_id
                          WHERE article = ? AND id > 0 AND moderated = true
                          GROUP BY comments.id
                          ORDER BY timestamp ASC;"#;

    let mut response = GetCommentsResponse {
        code: 200,
        status: String::from("OK"),
        comments: vec![],
        challenge: None,
        key: None,
    };

    if let Some(result) = state.pow.handle(&get_client_ip(&req), &data.challenge, &data.secret) {
        response.code = result.code;
        response.status = result.status.unwrap_or(String::from(""));
        response.challenge = result.challenge;
        response.key = result.key;

        return web::Json(response);
    }

    if let Ok(t) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        let client_ip = get_client_ip(&req);

        let decoded_article: String;
        if let Some(decode) = base64_decode(data.article.clone()) {
            decoded_article = decode;
        } else {
            response.code = 500;
            response.status = format!("Unable to decode supplied article id: {}", data.article);
            return web::Json(response);
        }

        info!(
            "{} Getting comments for '{}' for client {}",
            DateTime::from_timestamp(t.as_secs() as i64, 0).unwrap(),
            decoded_article,
            client_ip
        );

        match state.db_conn.lock() {
            Ok(conn) => {
                for row in conn
                    .prepare(query)
                    .unwrap()
                    .into_iter()
                    .bind((1, &data.commenter_id[..]))
                    .unwrap()
                    .bind((2, &data.article[..]))
                    .unwrap()
                    .map(|row| row.unwrap())
                {
                    let mut parent: i64 = 0;
                    if let Some(cell) = row.read::<Option<i64>, _>("parent") {
                        parent = cell;
                    }

                    response.comments.push(Comment {
                        id: row.read::<i64, _>("id"),
                        timestamp: row.read::<i64, _>("timestamp"),
                        parent,
                        poster_name: String::from(row.read::<&str, _>("poster_name")),
                        comment: String::from(row.read::<&str, _>("comment")),
                        votes: row.read::<i64, _>("votes"),
                        myvote: row.read::<i64, _>("myvote"),
                    });
                }
            }
            Err(e) => {
                response.code = 500;
                response.status = format!("Database error: {:?}", e);
                return web::Json(response);
            }
        }
    } else {
        response.code = 500;
        response.status = String::from("Unable to get system time");
    }

    web::Json(response)
}

#[post("/comment/vote/")]
async fn vote(
    data: web::Form<VoteRequest>,
    state: web::Data<AppState>,
    req: HttpRequest,
) -> web::Json<VoteResponse> {
    let upsert_query = r#"INSERT INTO votes VALUES (?, ?, ?) ON CONFLICT(comment_id, voter_id) DO UPDATE SET vote = ?;"#;
    let unvote_query = r#"DELETE FROM votes WHERE comment_id = ? AND voter_id = ?"#;

    let voter_id = ammonia::clean(&data.voter_id[..]);
    let comment_id = data.comment_id;
    let vote = data.vote;

    let mut response = VoteResponse {
        code: 200,
        status: String::from("OK"),
        challenge: None,
        key: None,
    };

    if let Some(result) = state.pow.handle(&get_client_ip(&req), &data.challenge, &data.secret) {
        response.code = result.code;
        response.status = result.status.unwrap_or(String::from(""));
        response.challenge = result.challenge;
        response.key = result.key;

        return web::Json(response);
    }

    if !(-1..=1).contains(&vote) {
        response.code = 500;
        response.status = String::from("Invalid vote");

        return web::Json(response);
    }

    if let Ok(t) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        let client_ip = get_client_ip(&req);

        info!(
            "{} Casting vote '{}' for commenter: '{}' for client {}",
            DateTime::from_timestamp(t.as_secs() as i64, 0).unwrap(),
            vote,
            voter_id,
            client_ip
        );

        match state.db_conn.lock() {
            Ok(conn) => {
                let mut statement = if vote == 0 {
                    let mut statement = conn.prepare(unvote_query).unwrap();
                    statement.bind((1, comment_id)).unwrap();
                    statement.bind((2, &voter_id[..])).unwrap();

                    statement
                } else {
                    let mut statement = conn.prepare(upsert_query).unwrap();
                    statement.bind((1, comment_id)).unwrap();
                    statement.bind((2, &voter_id[..])).unwrap();
                    statement.bind((3, vote)).unwrap();
                    statement.bind((4, vote)).unwrap();

                    statement
                };

                if let Err(e) = statement.next() {
                    response.code = 500;
                    response.status = format!("Could not vote: {e}");
                    web::Json(response)
                } else {
                    web::Json(response)
                }
            }
            Err(e) => {
                response.code = 500;
                response.status = format!("DB Error: {:?}", e);

                web::Json(response)
            }
        }
    } else {
        response.code = 500;
        response.status = String::from("Could not generate timestamp");
        web::Json(response)
    }
}

#[post("/pow/get/")]
async fn get_pow(state: web::Data<AppState>, req: HttpRequest) -> web::Json<GetPowResponse> {
    match state.pow.get_challenge(&get_client_ip(&req)) {
        Some(pow) => web::Json(GetPowResponse {
            code: 401,
            key: pow.key,
            challenge: pow.challenge,
        }),
        None => web::Json(GetPowResponse {
            code: 300,
            key: String::from(""),
            challenge: String::from("Challenge not required."),
        }),
    }
}

#[post("/pow/validate/")]
async fn validate_pow(
    data: web::Form<ValidatePowRequest>,
    state: web::Data<AppState>,
    req: HttpRequest,
) -> web::Json<ValidatePowResponse> {
    match state
        .pow
        .validate_pow(&get_client_ip(&req), &data.challenge, &data.secret)
    {
        Ok(_) => web::Json(ValidatePowResponse {
            code: 200,
            status: String::from("OK"),
        }),
        Err(e) => web::Json(ValidatePowResponse {
            code: 500,
            status: e,
        }),
    }
}

fn get_commenter_info(
    conn: &MutexGuard<'_, sqlite::Connection>,
    commenter_id: &str,
) -> Option<(String, String)> {
    let query = r#"SELECT name, email FROM ids WHERE commenter_id = ?"#;

    if let Some(row) = conn
        .prepare(query)
        .unwrap()
        .into_iter()
        .bind((1, commenter_id))
        .unwrap()
        .map(|row| row.unwrap())
        .next()
    {
        Some((
            String::from(row.read::<&str, _>("name")),
            String::from(row.read::<&str, _>("email")),
        ))
    } else {
        info!("error getting commenter_id");
        None
    }
}

fn get_client_ip(req: &HttpRequest) -> String {
    if let Some(ip) = req.headers().get("x-forwarded-for") {
        if let Ok(ip_str) = ip.to_str() {
            String::from(ip_str)
        } else {
            String::from("")
        }
    } else if let Some(ip) = req.headers().get("x-real-ip") {
        if let Ok(ip_str) = ip.to_str() {
            String::from(ip_str)
        } else {
            String::from("")
        }
    } else if let Some(ip) = req.peer_addr() {
        ip.ip().to_string()
    } else {
        String::from("")
    }
}

fn base64_decode(input: String) -> Option<String> {
    if let Ok(decode) = BASE64_STANDARD.decode(input) {
        if let Ok(utf8) = String::from_utf8(decode) {
            Some(utf8)
        } else {
            None
        }
    } else {
        None
    }
}
