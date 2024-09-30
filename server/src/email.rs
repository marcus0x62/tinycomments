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

use actix_web::web;
use lettre::message::header::ContentType as LettreContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use std::result::Result;

pub fn send_email(
    state: &web::Data<crate::AppState>,
    url: &String,
    commenter: &String,
    comment_text: &str,
) -> Result<(), String> {
    let msg = Message::builder()
        .from(
            format!(
                "{} <{}>",
                state.config.email_sender_name.clone().unwrap(),
                state.config.email_sender_address.clone().unwrap(),
            )
            .parse()
            .unwrap(),
        )
        .to(state
            .config
            .email_notify_address
            .clone()
            .unwrap()
            .parse()
            .unwrap())
        .subject(format!("New comment from {commenter}"))
        .header(LettreContentType::TEXT_HTML)
        .body(format!(
            r#"<p>A new comment was posted on {url} by {commenter}:</p>
<blockquote>{comment_text}</blockquote>
<p>Click <a href="{url}">here</a> to view the comment.</p>"#,
        ))
        .unwrap();

    let mailer = if let Some(user) = &state.config.email_smtp_user {
        let bind_pass: String;

        let pass = if let Some(pass) = &state.config.email_smtp_pass {
            pass
        } else {
            bind_pass = String::from("");
            &bind_pass
        };

        SmtpTransport::relay(&state.config.email_smtp_host.clone().unwrap())
            .unwrap()
            .credentials(Credentials::new(user.to_owned(), pass.to_owned()))
            .build()
    } else {
        SmtpTransport::relay(&state.config.email_smtp_host.clone().unwrap())
            .unwrap()
            .build()
    };

    // Send the email
    match mailer.send(&msg) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Unable to send message: {e:?}")),
    }
}
