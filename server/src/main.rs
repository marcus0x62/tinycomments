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
use hmac::{Hmac, Mac};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlite::Value::Null;
use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::str;
use std::sync::Mutex;
use std::time::{Instant, SystemTime};

struct AppState {
    db_conn: Mutex<sqlite::Connection>,
    challenges: Mutex<HashMap<String, PowChallenge>>,
    transactions: Mutex<HashMap<String, [Option<Instant>; 32]>>,
}

struct Pow {
    key: String,
    challenge: String,
}

struct PowChallenge {
    client_ip: String,
    key: [u8; 32],
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

type HmacSha256 = Hmac<Sha256>;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db_conn = Mutex::new(sqlite::open("comments.sqlite").unwrap());
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

    let state = web::Data::new(AppState {
        db_conn,
        challenges: Mutex::new(HashMap::new()),
        transactions: Mutex::new(HashMap::new()),
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

    if let Some(challenge) = &data.challenge {
        if let Some(secret) = &data.secret {
            if let Err(_e) = state.validate_pow(&get_client_ip(&req), challenge, secret) {
                response.code = 403;
                response.status = String::from("Challenge not accepted.");
                return web::Json(response);
            }
        } else {
            response.code = 500;
            response.status = String::from("Challenge proof incomplete: no secret provided");
            return web::Json(response);
        }
    } else if let Some(challenge) = state.get_challenge(&get_client_ip(&req)) {
        response.code = 401;
        response.challenge = Some(challenge.challenge);
        response.key = Some(challenge.key);

        return web::Json(response);
    }

    if let Ok(t) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        let client_ip = get_client_ip(&req);

        let mut rand_bytes = [0u8; 32];
        thread_rng().fill(&mut rand_bytes);

        let commenter_id = hex::encode(rand_bytes);

        println!(
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

    if let Some(challenge) = &data.challenge {
        if let Some(secret) = &data.secret {
            if let Err(_e) = state.validate_pow(&get_client_ip(&req), challenge, secret) {
                response.code = 403;
                response.status = String::from("Challenge not accepted.");
                return web::Json(response);
            }
        } else {
            response.code = 500;
            response.status = String::from("Challenge proof incomplete: no secret provided");
            return web::Json(response);
        }
    } else if let Some(challenge) = state.get_challenge(&get_client_ip(&req)) {
        response.code = 401;
        response.challenge = Some(challenge.challenge);
        response.key = Some(challenge.key);

        return web::Json(response);
    }

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

        println!(
            "{} Posting comment for '{}' for client {} with id '{}'",
            DateTime::from_timestamp(t.as_secs() as i64, 0).unwrap(),
            decoded_article,
            client_ip,
            &ammonia::clean(&data.commenter_id[..])[..]
        );

        match state.db_conn.lock() {
            Ok(conn) => {
                let mut statement = conn.prepare(query).unwrap();
                statement
                    .bind((1, &ammonia::clean(&data.article[..])[..]))
                    .unwrap();
                statement
                    .bind((2, &ammonia::clean(&data.commenter_id[..])[..]))
                    .unwrap();

                if data.parent == 0 {
                    statement.bind((3, Null)).unwrap();
                } else {
                    statement.bind((3, data.parent)).unwrap();
                }

                statement
                    .bind((4, &ammonia::clean_text(&data.comment[..])[..]))
                    .unwrap();
                statement.bind((5, t.as_secs() as i64)).unwrap();

                if let Err(e) = statement.next() {
                    response.code = 500;
                    response.status = format!("Could not add comment: {e}");
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

    if let Some(challenge) = &data.challenge {
        if let Some(secret) = &data.secret {
            if let Err(_e) = state.validate_pow(&get_client_ip(&req), challenge, secret) {
                response.code = 403;
                response.status = String::from("Challenge not accepted.");
                return web::Json(response);
            }
        } else {
            response.code = 500;
            response.status = String::from("Challenge proof incomplete: no secret provided");
            return web::Json(response);
        }
    } else if let Some(challenge) = state.get_challenge(&get_client_ip(&req)) {
        response.code = 401;
        response.challenge = Some(challenge.challenge);
        response.key = Some(challenge.key);

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

        println!(
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

    if let Some(challenge) = &data.challenge {
        if let Some(secret) = &data.secret {
            if let Err(_e) = state.validate_pow(&get_client_ip(&req), challenge, secret) {
                response.code = 403;
                response.status = String::from("Challenge not accepted.");
                return web::Json(response);
            }
        } else {
            response.code = 500;
            response.status = String::from("Challenge proof incomplete: no secret provided");
            return web::Json(response);
        }
    } else if let Some(challenge) = state.get_challenge(&get_client_ip(&req)) {
        response.code = 401;
        response.challenge = Some(challenge.challenge);
        response.key = Some(challenge.key);

        return web::Json(response);
    }

    if !(-1..=1).contains(&vote) {
        response.code = 500;
        response.status = String::from("Invalid vote");

        return web::Json(response);
    }

    if let Ok(t) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        let client_ip = get_client_ip(&req);

        println!(
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
    match state.get_challenge(&get_client_ip(&req)) {
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
    match state.validate_pow(&get_client_ip(&req), &data.challenge, &data.secret) {
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

impl AppState {
    fn get_txcount(&self, ip: &str, add_transaction: bool) -> Result<u32, String> {
        match self.transactions.lock() {
            Ok(mut txhash) => {
                let now = Instant::now();
                let mut new_instants: [Option<Instant>; 32] = [None; 32];

                match txhash.get(ip) {
                    Some(txvec) => {
                        let mut tx_count = 0;
                        let mut i = 0;

                        for tx in txvec.iter().flatten() {
                            if tx.elapsed().as_secs() < 30 {
                                tx_count += 1;
                                new_instants[i] = Some(*tx);
                                i += 1;
                            }
                        }

                        if add_transaction {
                            if i < 32 {
                                new_instants[i] = Some(now);
                            } else {
                                new_instants.sort();
                                new_instants[31] = Some(now);
                            }
                            txhash.insert(ip.to_owned(), new_instants);
                        }

                        Ok(tx_count)
                    }
                    None => {
                        if add_transaction {
                            new_instants[0] = Some(now);
                            txhash.insert(ip.to_owned(), new_instants);

                            Ok(1)
                        } else {
                            Ok(0)
                        }
                    }
                }
            }
            Err(e) => Err(format!("Error getting transaction lock: {e:?}")),
        }
    }

    fn get_challenge(&self, ip: &str) -> Option<Pow> {
        if let Ok(count) = self.get_txcount(ip, true) {
            if count > 5 {
                if let Ok(pow) = self.generate_pow(ip, 16 + count - 5) {
                    return Some(pow);
                }
            }
        }

        None
    }

    fn generate_pow(&self, ip: &str, bits: u32) -> Result<Pow, String> {
        let mut rng = thread_rng();

        let mut key_rand_bytes = [0u8; 32];
        rng.fill(&mut key_rand_bytes);

        let hexkey = hex::encode(key_rand_bytes);

        let mut mac = HmacSha256::new_from_slice(hexkey.as_bytes()).expect("?!?");
        let secret = format!("{}", rng.gen_range(0..u64::pow(2, bits)));
        mac.update(secret.as_bytes());
        let res = mac.finalize();

        let challenge = hex::encode(res.into_bytes());
        match self.challenges.lock() {
            Ok(mut hash) => {
                hash.insert(
                    challenge.clone(),
                    PowChallenge {
                        client_ip: ip.to_owned(),
                        key: key_rand_bytes,
                    },
                );

                Ok(Pow {
                    key: hexkey.to_string(),
                    challenge,
                })
            }
            Err(e) => Err(format!("Could not get lock: {e:?}")),
        }
    }

    fn validate_pow(
        &self,
        ip: &String,
        client_challenge: &str,
        client_secret: &str,
    ) -> Result<String, String> {
        match self.challenges.lock() {
            Ok(mut hash) => match hash.get(client_challenge) {
                Some(challenge) => {
                    if challenge.client_ip != *ip {
                        return Err(String::from("Forbidden. Client IP Mismatch."));
                    }

                    let mut mac = HmacSha256::new_from_slice(hex::encode(challenge.key).as_bytes())
                        .expect("Cannot make hmac instance");

                    mac.update(client_secret.as_bytes());
                    let res = mac.finalize();

                    let computed = hex::encode(res.into_bytes());

                    if computed == *client_challenge {
                        hash.remove(client_challenge);
                        Ok(String::from("Ok"))
                    } else {
                        Err(String::from("Forbidden"))
                    }
                }
                None => Err(String::from("Invalid challenge")),
            },
            Err(e) => Err(format!("Internal server error: {e:?}")),
        }
    }
}
