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

use serde::Deserialize;
use std::{fs::File, io::prelude::*};

#[derive(Deserialize, Debug)]
pub enum DebugLevel {
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub bind_address: String,
    pub bind_port: u16,
    pub debug: DebugLevel,
    pub db_path: String,
}

impl ConfigFile {
    pub fn new_from_file(file: &str) -> Result<Self, String> {
        let mut handle = match File::open(file) {
            Ok(handle) => handle,
            Err(e) => return Err(format!("Unable to open config file: {e:?}")),
        };

        let mut config_text = String::new();
        let _ = handle.read_to_string(&mut config_text);
        match toml::from_str(&config_text) {
            Ok(cfg) => Ok(cfg),
            Err(e) => Err(format!("Unable to parse config file: {e:?}")),
        }
    }
}
