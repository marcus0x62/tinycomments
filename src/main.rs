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

use actix_web::{get, http::header::ContentType, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder, Result};
use base64::prelude::*;
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::prelude::*;
use std::time::SystemTime;
use std::sync::Mutex;

struct AppState {
    db_conn: Mutex<sqlite::Connection>
}

#[derive(Serialize,Deserialize)]
struct Comments {
    comments: Vec<Comment>
}

#[derive(Serialize,Deserialize)]
struct Comment {
    id: i64,
    timestamp: i64,
    parent: i64,
    poster_name: String,
    comment: String,
    votes: i64
}

#[derive(Deserialize)]
struct NewComment {
    name: String,
    email: String,
    comment: String,
    parent: i64
}

#[derive(Serialize,Deserialize)]
struct NewCommentStatus {
    code: u16,
    status: String
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .app_data(web::Data::new(AppState {
                db_conn: Mutex::new(sqlite::open("comments.sqlite").unwrap())
            }))
            .service(post_comment)
            .service(get_comments)
            .service(get_root)
    })
    .bind(("127.0.0.1", 3000))?
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

#[post("/comment/{article}")]
async fn post_comment(
    article: web::Path<String>,
    data: web::Form<NewComment>,
    state: web::Data<AppState>,
    req: HttpRequest
) -> web::Json<NewCommentStatus> {
    let query = r#"INSERT INTO comments (article, parent, poster_name, poster_email, comment, moderated, votes, timestamp)
                                        VALUES(?, ?, ?, ?, ?, true, 1, ?);"#;

    if let Ok(t) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        let bind_ip: String;

        let ip = if let Some(ip) = req.headers().get("x-forwarded-for") {
            ip.to_str().unwrap()
        } else if let Some(ip) = req.headers().get("x-real-ip") {
            ip.to_str().unwrap()
        } else if let Some(ip) = req.peer_addr() {
            bind_ip = ip.ip().to_string();
            &bind_ip[..]
        } else {
            println!("Could not find any IP for this client.");
            ""
        };

        let decoded_article: String;

        if let Ok(decode) = BASE64_STANDARD.decode(article.to_string()) {
            if let Ok(utf8) = String::from_utf8(decode) {
                decoded_article = utf8;
            } else {
                return web::Json(NewCommentStatus { code: 500, status: String::from("Could not create utf8 string from bytes") });
            }
        } else {
            return web::Json(NewCommentStatus {
                code: 500,
                status: String::from(format!("Could not base64 decode '{}'", article.to_string()))
            });
        }

        println!("{} Posting comment for '{:?}' for client {}",
            DateTime::from_timestamp(t.as_secs() as i64, 0).unwrap(),
            decoded_article,
            ip);

        match state.db_conn.lock() {
            Ok(conn) => {
                let mut statement = conn.prepare(query).unwrap();
                statement.bind((1, &ammonia::clean(&article[..])[..])).unwrap();
                statement.bind((2, data.parent)).unwrap();
                statement.bind((3, &ammonia::clean(&data.name[..])[..])).unwrap();
                statement.bind((4, &ammonia::clean(&data.email[..])[..])).unwrap();
                statement.bind((5, &ammonia::clean_text(&data.comment[..])[..])).unwrap();
                statement.bind((6, t.as_secs() as i64)).unwrap();

                if let Err(e) = statement.next() {
                    web::Json(NewCommentStatus { code: 500, status: String::from(format!("Could not add comment: {e}")) })
                } else {
                    web::Json(NewCommentStatus { code: 200, status: String::from("OK") })
                }
            }
            Err(e) => { web::Json(NewCommentStatus { code: 500, status: String::from(format!("DB Error: {:?}", e)) }) }
        }
    } else {
        web::Json(NewCommentStatus { code: 500, status: String::from("Could not generate timestamp") })
    }
}

#[get("/comments/{article}")]
async fn get_comments(article: web::Path<String>, state: web::Data<AppState>, req: HttpRequest) -> Result<impl Responder> {
    let myarticle = article.to_string();

    let query = r#"SELECT id, parent, poster_name, timestamp, comment, votes FROM comments
                          WHERE article = ? AND id > 0 AND moderated = true ORDER BY timestamp ASC;"#;

    let mut comments = Comments { comments: vec![] };

    if let Ok(t) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        let bind_ip: String;

        let ip = if let Some(ip) = req.headers().get("x-forwarded-for") {
            ip.to_str().unwrap()
        } else if let Some(ip) = req.headers().get("x-real-ip") {
            ip.to_str().unwrap()
        } else if let Some(ip) = req.peer_addr() {
            bind_ip = ip.ip().to_string();
            &bind_ip[..]
        } else {
            println!("Could not find any IP for this client.");
            ""
        };

        let decoded_article: String;

        if let Ok(decode) = BASE64_STANDARD.decode(article.to_string()) {
            if let Ok(utf8) = String::from_utf8(decode) {
                decoded_article = utf8;
            } else {
                return Ok(web::Json(comments));
            }
        } else {
            return Ok(web::Json(comments));
        }

        println!("{} Getting comments for '{}' for client {}",
            DateTime::from_timestamp(t.as_secs() as i64, 0).unwrap(),
            decoded_article,
            ip);

        match state.db_conn.lock() {
            Ok(conn) => {
                for row in conn
                    .prepare(query)
                    .unwrap()
                    .into_iter()
                    .bind((1, &myarticle[..]))
                    .unwrap()
                    .map(|row| row.unwrap())
                {
                    comments.comments.push(Comment {
                        id: row.read::<i64,_>("id"),
                        timestamp: row.read::<i64,_>("timestamp"),
                        parent: row.read::<i64, _>("parent"),
                        poster_name: String::from(row.read::<&str, _>("poster_name")),
                        comment: String::from(row.read::<&str, _>("comment")),
                        votes: row.read::<i64, _>("votes")
                    });
                }
            }
            Err(e) => { }
        }
    }
    Ok(web::Json(comments))
}
