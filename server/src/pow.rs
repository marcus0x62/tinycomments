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
use hmac::{Hmac, Mac};
use rand::{thread_rng, Rng};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

type HmacSha256 = Hmac<Sha256>;

pub struct Pow {
    pub key: String,
    pub challenge: String,
}

pub struct PowChallenge {
    pub client_ip: String,
    pub key: [u8; 32],
}

pub struct PowError {
    pub code: u16,
    pub status: Option<String>,
    pub challenge: Option<String>,
    pub key: Option<String>,
}

pub struct PowTable {
    challenges: Mutex<HashMap<String, PowChallenge>>,
    transactions: Mutex<HashMap<String, [Option<Instant>; 32]>>,
}

impl PowTable {
    pub fn new() -> Self {
        PowTable {
            challenges: Mutex::new(HashMap::new()),
            transactions: Mutex::new(HashMap::new()),
        }
    }

    pub fn handle(&self, ip: &String, challenge: &Option<String>, secret: &Option<String>) -> Option<PowError> {
        if let Some(challenge) = challenge {
            if let Some(secret) = secret {
                if let Err(_e) = self.validate_pow(ip, challenge, secret)
                {
                    return Some(PowError {
                        code: 403,
                        status: Some(String::from("Challenge not accepted.")),
                        challenge: None,
                        key: None,
                    });
                }
            } else {
                return Some(PowError {
                    code: 500,
                    status: Some(String::from("Challenge proof incomplete: no secret provided")),
                    challenge: None,
                    key: None,
                });
            }
        } else if let Some(challenge) = self.get_challenge(ip) {
            return Some(PowError {
                code: 401,
                status: None,
                challenge: Some(challenge.challenge),
                key: Some(challenge.key),
            });
        }

        None
    }

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

    pub fn get_challenge(&self, ip: &str) -> Option<Pow> {
        if let Ok(count) = self.get_txcount(ip, true) {
            if count > 5 {
                if let Ok(pow) = self.generate_pow(ip, 16 + count - 5) {
                    return Some(pow);
                }
            }
        }

        None
    }

    pub fn generate_pow(&self, ip: &str, bits: u32) -> Result<Pow, String> {
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

    pub fn validate_pow(
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
