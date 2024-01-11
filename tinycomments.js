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

async function get_comments() {
    let b64 = btoa(document.baseURI);
    let url = `http://localhost:3000/comments/${b64}`;
    console.log(`Fetching comments from ${url}`);

    let res = await fetch(url);
    let json = await res.json();

    let root = document.getElementById("rootCommentList");
    while (root.firstChild) {
        root.removeChild(root.firstChild);
    }

    let n_comments = 0;

    for (row of json['comments']) {
        n_comments += 1;

        let li = document.createElement("li");
        let div = document.createElement("div");
        let name_date = document.createElement("p");
        let comment = document.createElement("p");
        let replyp = document.createElement("p");
        let replyb = document.createElement("input");
        let replydiv = document.createElement("div");

        div.id = "comment-" + row['id'];

        let date = new Date(row['timestamp'] * 1000);
        name_date.textContent = "On " + date.toLocaleString('en-us') + " " + row['poster_name'] + " wrote:";
        comment.innerHTML = row['comment'];

        replyp.id = "replybox-" + row['id'];

        let replylink = `reply_box_show(${row['id']});`;
        replyb.type = 'button';
        replyb.value = "Reply";
        replyb.setAttribute('onclick', replylink);
        replyp.append(replyb);

        replydiv.id = "replydiv-" + row['id'];

        div.append(name_date);
        div.append(comment);
        div.append(replyp);
        div.append(replydiv);

        li.append(div);

        if (row['parent'] == 0) {
            root.append(li);
        } else {
            let replylist = document.getElementById("replylist-" + row['parent']);
            if (replylist) {
                replylist.append(li);
            } else {
                let replydiv = document.getElementById("replydiv-" + row['parent']);
                let replylist = document.createElement("ul");
                replylist.id = "replylist-" + row['parent'];
                replydiv.append(replylist);
                replylist.append(li);
            }
        }
    }

    document.getElementById("commentCount").textContent = `There are ${n_comments} comments on this post.`;
}

function reply_box_show(id) {
    let replybox = document.getElementById("replybox-" + id);

    while (replybox.firstChild) {
        replybox.removeChild(replybox.firstChild);
    }

    let div = document.createElement("div");
    div.innerHTML = "Name: <input type='text' id='replyName-" + id + "'/><br/>";
    div.innerHTML += "Email: <input type='text' id='replyEmail-" + id + "'/><br/>";
    div.innerHTML += "Comment: <textarea id='replyCommentText-" + id + "'></textarea><br/>";
    div.innerHTML += "<input type='button' value='Reply!' onClick='reply_comment(" + id + ");'/>";
    div.innerHTML += "<input type='button' value='Cancel' onClick='reply_box_hide(" + id + ");'/>";
    replybox.append(div);
}

function reply_box_hide(id) {
    let replybox = document.getElementById(`replybox-${id}`);

    while(replybox.firstChild) {
        replybox.removeChild(replybox.firstChild);
    }

    let replyb = document.createElement("input");
    let replylink = `reply_box_show(${id});`;
    replyb.type = "button";
    replyb.value = "Reply";
    replyb.setAttribute('onclick', replylink);

    replybox.append(replyb);
}

function root_comment() {
    post_comment(document.getElementById("commentName").value, document.getElementById("commentEmail").value,
                 document.getElementById("commentText").value, 0);
}

function reply_comment(parent) {
    post_comment(document.getElementById(`replyName-${parent}`).value, document.getElementById(`replyEmail-${parent}`).value,
                 document.getElementById(`replyCommentText-${parent}`).value, parent);
}

async function post_comment(name, email, comment, parent) {
    let b64 = btoa(document.baseURI);
    let url = `http://localhost:3000/comment/${b64}`;
    console.log(`Posting comment to ${url}`);

    let comment_data = new URLSearchParams();
    comment_data.append("comment", comment);
    comment_data.append("name", name);
    comment_data.append("email", email);
    comment_data.append("parent", parent);

    try {
        let res = await fetch(url, {
            method: "POST",
            body: comment_data
        });

        let json = await res.json();

        document.getElementById("commentStatus").textContent = json["status"];
        get_comments();
    } catch (error) {
        document.getElementById("commentStatus").textContent = `Error posting comment: ${error}`;
    }
}
