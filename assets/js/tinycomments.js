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

var TINYCOMMENTS_PATH = '/tinycomments';

async function get_comments() {
    let b64 = btoa(normalize_uri());
    let url = `${TINYCOMMENTS_PATH}/comment/get/`;

    let commenter_id = await get_commenter_id('', '', false);
    let comment_data = new URLSearchParams();
    comment_data.append('commenter_id', commenter_id);
    comment_data.append('article', b64);

    let json;

    try {
        let res = await fetch(url, { method: 'POST', body: comment_data });
        json = await res.json();
    } catch (error) {
        update_status(`Error getting comments: ${error}`);
        return null;
    }

    if (json['code'] == 401) {
        update_status('Solving client-puzzle due to request volume...');
        let secret = await solve_pow(json['challenge'], json['key']);
        comment_data.append('challenge', json['challenge']);
        comment_data.append('secret', secret);

        update_status('Client puzzle solved.');
        try {
            let res = await fetch(url, { method: 'POST', body: comment_data });
            json = await res.json();

            if (json['code'] != 200) {
                update_status(`Could not get comments with supplied challenge. Error ${json['code']}: ${json['status']}`);
                return null;
            }
        } catch (error) {
            update_status(`Error getting comments: ${error}`);
            return null;
        }
    }

    let root = document.getElementById('rootCommentList');
    while (root.firstChild) {
        root.removeChild(root.firstChild);
    }

    let n_comments = 0;

    for (row of json['comments']) {
        n_comments += 1;

        let li = document.createElement('li');
        let div = document.createElement('div');
        let name_date = document.createElement('p');
        let comment = document.createElement('p');
        let replyp = document.createElement('p');
        let replyb = document.createElement('input');
        let replydiv = document.createElement('div');

        div.id = `comment-${row['id']}`;

        let date = new Date(row['timestamp'] * 1000);
        name_date.textContent = 'On ' + date.toLocaleString('en-us') + ` ${row['poster_name']} wrote: (${row['votes']} upvotes!)`;
        comment.innerHTML = row['comment'];

        replyp.id = `replybox-${row['id']}`;

        replyb.type = 'button';
        replyb.value = 'Reply';

        let closure = function(id) {
            return function() {
                reply_box_show(id);
            }
        }(row['id']);

        replyb.addEventListener('click', closure);
        replyp.append(replyb);

        replydiv.id = `replydiv-${row['id']}`;

        let myvote = row['myvote'];

        let votediv = document.createElement('div');
        let upvote = document.createElement('a');
        upvote.id = `upvote-${row['id']}`;
        upvote.textContent = 'Upvote!';
        upvote.style.cursor = 'pointer';

        if (myvote == 1) {
            let closure = function(id) {
                return function() {
                    vote(id, 0);
                }
            }(row['id']);

            upvote.addEventListener('click', closure);
            upvote.style.fontWeight = 'bold';
        } else {
            let closure = function(id) {
                return function() {
                    vote(id, 1);
                }
            }(row['id']);
            upvote.addEventListener('click', closure);
            upvote.style.fontWeight = null;
        }

        let downvote = document.createElement('a');
        downvote.id = `downvote-${row['id']}`;
        downvote.textContent = 'Downvote :(';
        downvote.style.cursor = 'pointer';

        if (myvote == -1) {
            let closure = function(id) {
                return function() {
                    vote(id, 0);
                }
            }(row['id']);
            downvote.addEventListener('click', closure);
            downvote.style.fontWeight = 'bold';
        } else {
            let closure = function(id) {
                return function() {
                    vote(id, -1);
                }
            }(row['id']);
            downvote.addEventListener('click', closure);
            downvote.style.fontWeight = null;
        }

        votediv.append(upvote);
        votediv.append(downvote);

        div.append(name_date);
        div.append(votediv);
        div.append(comment);
        div.append(replyp);
        div.append(replydiv);

        li.append(div);

        if (row['parent'] == 0) {
            root.append(li);
        } else {
            let replylist = document.getElementById(`replylist-${row['parent']}`);
            if (replylist) {
                replylist.append(li);
            } else {
                let replydiv = document.getElementById(`replydiv-${row['parent']}`);
                let replylist = document.createElement('ul');
                replylist.id = `replylist-${row['parent']}`;
                replydiv.append(replylist);
                replylist.append(li);
            }
        }
    }

    document.getElementById('commentCount').textContent = `There are ${n_comments} comments on this post.`;
}

function reply_box_show(id) {
    let replybox = document.getElementById(`replybox-${id}`);

    while (replybox.firstChild) {
        replybox.removeChild(replybox.firstChild);
    }

    let div = document.createElement('div');
    div.innerHTML = `Name: <input type='text' id='replyName-${id}'/><br/>`;
    div.innerHTML += `Email: <input type='text' id='replyEmail-${id}'/><br/>`;
    div.innerHTML += `Comment: <textarea id='replyCommentText-${id}'></textarea><br/>`;
    div.innerHTML += `<input type='button' id='reply-${id}' value='Reply!'/>`;
    div.innerHTML += `<input type='button' id='cancel-${id}' value='Cancel'/>`;
    replybox.append(div);

    let reply_closure = function(id) {
        return function() {
            reply_comment(id);
        }
    }(id);

    let cancel_closure = function(id) {
        return function() {
            reply_box_hide(id);
        }
    }(id);

    let replybutton = document.getElementById(`reply-${id}`);
    replybutton.addEventListener('click', reply_closure);
    let cancelbutton = document.getElementById(`cancel-${id}`);
    cancelbutton.addEventListener('click', cancel_closure);
}

function reply_box_hide(id) {
    let replybox = document.getElementById(`replybox-${id}`);

    while(replybox.firstChild) {
        replybox.removeChild(replybox.firstChild);
    }

    let replyb = document.createElement('input');
    replyb.type = 'button';
    replyb.value = 'Reply';

    let closure = function(id) {
        return function() {
            reply_box_show(id);
        }
    }(id);

    replyb.addEventListener('click', closure);

    replybox.append(replyb);
}

function root_comment() {
    post_comment(document.getElementById('commentName').value, document.getElementById('commentEmail').value,
                 document.getElementById('commentText').value, 0);
}

function reply_comment(parent) {
    post_comment(document.getElementById(`replyName-${parent}`).value, document.getElementById(`replyEmail-${parent}`).value,
                 document.getElementById(`replyCommentText-${parent}`).value, parent);
}

async function get_commenter_id(name, email, force=false) {
    let url = `${TINYCOMMENTS_PATH}/id/`;

    let commenter_id = localStorage.getItem('tinycomments_commenter_id');
    if (commenter_id && commenter_id.length > 0 && force == false) {
        return commenter_id;
    } else {
        let id_data = new URLSearchParams();
        id_data.append('name', name);
        id_data.append('email', email);

        let json;

        try {
            let res = await fetch(url, { method: 'POST', body: id_data });
            json = await res.json();
        } catch (error) {
            update_status(`Error getting poster id: ${error}`);
            return null;
        }

        if (json['code'] == 401) {
            update_status('Solving client-puzzle due to request volume...');
            let secret = await solve_pow(json['challenge'], json['key']);
            id_data.append('challenge', json['challenge']);
            id_data.append('secret', secret);

            update_status('Client puzzle solved.');
            try {
                let res = await fetch(url, { method: 'POST', body: id_data });
                json = await res.json();

                if (json['code'] != 200) {
                    update_status(`Could not get id with supplied challenge. Error ${json['code']}: ${json['status']}`);
                    return null;
                }
            } catch (error) {
                update_status(`Error getting id: ${error}`);
                return null;
            }
        }

        if (json['code'] == 200) {
            localStorage.setItem('tinycomments_commenter_id', json['commenter_id']);
            return json['commenter_id'];
        } else {
            update_status(`Unable to generate id: ${json['status']}`);
            return null;
        }
    }
}

async function post_comment(name, email, comment, parent) {
    let commenter_id = await get_commenter_id(name, email, false);
    if (commenter_id.length == 0) {
        return; // status text is handled by get_commenter_id
    }

    let b64 = btoa(normalize_uri());
    let url = `${TINYCOMMENTS_PATH}/comment/post/`;

    let comment_data = new URLSearchParams();
    comment_data.append('article', b64);
    comment_data.append('commenter_id', commenter_id);
    comment_data.append('comment', comment);
    comment_data.append('parent', parent);

    let json;

    try {
        let res = await fetch(url, { method: 'POST', body: comment_data });
        json = await res.json();
    } catch (error) {
        update_status(`Error posting comment: ${error}`);
    }

    if (json['code'] == 401) {
        update_status('Solving client-puzzle due to request volume...');
        let secret = await solve_pow(json['challenge'], json['key']);
        comment_data.append('challenge', json['challenge']);
        comment_data.append('secret', secret);

        update_status('Client puzzle solved.');
        try {
            let res = await fetch(url, { method: 'POST', body: comment_data });
            json = await res.json();

            if (json['code'] != 200) {
                update_status(`Could not post comment with supplied challenge. Error ${json['code']}: ${json['status']}`);
                return null;
            }
        } catch (error) {
            update_status(`Error posting comment: ${error}`);
            return null;
        }
    }

    get_comments();
}

async function vote(comment_id, vote) {
    let url = `${TINYCOMMENTS_PATH}/comment/vote/`;

    let commenter_id = await get_commenter_id('', '', false);
    if (commenter_id.length == 0) {
        return; // status text is handled by get_commenter_id
    }

    if (vote < -1 || vote > 1) {
        update_status('Invalid vote!');
    }

    let vote_data = new URLSearchParams();
    vote_data.append('comment_id', comment_id);
    vote_data.append('voter_id', commenter_id);
    vote_data.append('vote', vote);

    let json;
    try {
        let res = await fetch(url, { method: 'POST', body: vote_data });
        json = await res.json();
    } catch (error) {
        update_status(`Error casting vote: ${error}`);
    }

    if (json['code'] == 401) {
        update_status('Solving client-puzzle due to request volume...');
        let secret = await solve_pow(json['challenge'], json['key']);
        vote_data.append('challenge', json['challenge']);
        vote_data.append('secret', secret);

        update_status('Client puzzle solved.');
        try {
            let res = await fetch(url, { method: 'POST', body: vote_data });
            json = await res.json();

            if (json['code'] != 200) {
                update_status(`Could not vote with supplied challenge. Error ${json['code']}: ${json['status']}`);
                return null;
            }
        } catch (error) {
            update_status(`Error voting: ${error}`);
            return null;
        }
    }

    if (json['code'] == 200) {
        let closure = function(id) {
            return function() {
                vote(id, 0);
            }
        }(comment_id);

        if (vote == 1) {
            let elem = document.getElementById(`upvote-${comment_id}`);

            elem.addEventListener('click', closure);
        } else if (vote == -1) {
            let elem = document.getElementById(`downvote-${comment_id}`);
            elem.addEventListener('click', closure);
        } else if (vote == 0) {
            let up = document.getElementById(`upvote-${comment_id}`);
            let down = document.getElementById(`downvote-${comment_id}`);
        }

        update_status(`Vote successfully cast.`);
        get_comments();
    } else {
        update_status(`Unable to vote: ${json['status']}`);
    }
}

function normalize_uri() {
    const UriRegex = new RegExp('^([^#]+)#?.*$');

    let match = document.baseURI.match(UriRegex);

    return match[1];
}

function update_status(status) {
    document.getElementById('commentStatus').textContent = status;
}

async function solve_pow(challenge, key) {
    var enc = new TextEncoder('utf-8');

    for (let i = 0; i < Math.pow(2,32); i++) {
        let mykey = await window.crypto.subtle.importKey("raw", enc.encode(key),
                                                         { name: "HMAC", hash: {name: "SHA-256"}}, false, ["sign", "verify"]);

        let signature = await window.crypto.subtle.sign("HMAC", mykey, enc.encode(`${i}`));

        let computed = '';
        let computed_buffer = new Uint8Array(signature, 0, signature.len);
        for (let j = 0; j < computed_buffer.length; j++) {
            computed += computed_buffer[j].toString(16).padStart(2, '0');
        }

        if (computed == challenge) {
            return i;
        }
    }
}

(function(){
    window.addEventListener('load', (e) => {
        TINYCOMMENTS_PATH = {{- with .Site.Params.tinycommentsPath }} '{{ . }}'; {{- else }} '/tinycomments'; {{- end }}

        get_comments();

        let button = document.getElementById('commentButton');

        let closure = function() {
            return function() {
                root_comment();
            }
        }();

        button.addEventListener('click', closure);
    });
})();
