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

use actix_web::{get, http::header::ContentType, post, web, App, HttpResponse, HttpServer, Responder, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::prelude::*;
use std::time::SystemTime;

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
            .service(post_comment)
            .service(get_comments)
            .service(get_root)
    })
    .bind(("127.0.0.1", 3000))?
    .run()
    .await
}

#[get("/")]
async fn get_root() -> HttpResponse {
    let mut handle = File::open("comments.html").expect("Unable to open file");
    let mut contents = String::new();
    let _ = handle.read_to_string(&mut contents);

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(contents)
}

#[post("/comment/{uri}")]
async fn post_comment(uri: web::Path<String>, data: web::Form<NewComment>) -> web::Json<NewCommentStatus> {
    let conn = sqlite::open("comments.sqlite").unwrap();

    let query = "INSERT INTO comments (article,parent,poster_name,poster_email,comment,moderated,votes,timestamp) VALUES(?,?,?,?,?,true,1,?);";

    if let Ok(t) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        let mut statement = conn.prepare(query).unwrap();
        statement.bind((1, &ammonia::clean(&uri[..])[..])).unwrap();
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
    } else {
        web::Json(NewCommentStatus { code: 500, status: String::from("Could not generate timestamp") })
    }
}

#[get("/comments/{article}")]
async fn get_comments(article: web::Path<String>) -> Result<impl Responder> {
    let myarticle = format!("{}", &article);
    let conn = sqlite::open("comments.sqlite").unwrap();

    let query = "SELECT id, parent, poster_name, timestamp, comment, votes FROM comments WHERE article = ? AND id > 0 AND moderated = true ORDER BY timestamp ASC;";

    let mut comments = Comments { comments: vec![] };

    println!("Getting comments for '{}'", &article);
    for row in conn
        .prepare(query)
        .unwrap()
        .into_iter()
        .bind((1, &myarticle[..]))
        .unwrap()
        .map(|row| row.unwrap())
        {
            let comment = Comment {
                id: row.read::<i64,_>("id"),
                timestamp: row.read::<i64,_>("timestamp"),
                parent: row.read::<i64, _>("parent"),
                poster_name: String::from(row.read::<&str, _>("poster_name")),
                comment: String::from(row.read::<&str, _>("comment")),
                votes: row.read::<i64, _>("votes")
            };
            comments.comments.push(comment);
        }

    Ok(web::Json(comments))
}
