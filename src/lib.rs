use base64::encode_config;
use rand::Rng;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use simple_server;

use std::convert::TryInto;
use std::fs::File;
use std::path::PathBuf;
use std::sync::{mpsc, Mutex};
use std::{time, thread};
use std::time::Duration;
use std::vec::Vec;


const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
    abcdefghijklmnopqrstuvwxyz\
    0123456789-.~_";

const PAGE_SIZE: u32 = 25;

const PAUSE_FETCH: Duration = time::Duration::from_secs(1);
const PAUSE_WRITE: Duration = time::Duration::from_millis(250);


type Result<T> = std::result::Result<T, Error>;


#[derive(Debug, Clone)]
pub enum Error{
    SerError,
    FetchError,
    IOError
}


pub struct TokenFetcher<'a> {
    client_id: &'a str,
    client_secret: &'a str,
    code_verifier: String,
    auth_uri: reqwest::Url,
    redirect_uri: String,
    refresh_uri: &'a str,
}


pub struct MediaFetcher<'a> {
    base_uri: &'a str,
    access_token: &'a str,
    album_dir: &'a PathBuf,
}


#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub media_items: Vec<Media>,
    #[serde(default)]
    pub next_page_token: Option<String>
}


#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub id: String,
    pub base_url: String,
    pub mime_type: String,
    pub media_metadata: MediaMetadata,
    pub filename: String,
}


#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadata {
    pub creation_time: String,
}


impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Error {
        // XXX: need to map the errors so that the underlying failure message
        // can be used
        print!("Err {}", err);
        Error::SerError
    }
}


impl<'a> TokenFetcher<'a> {
    const HOST: &'static str = "localhost";
    const PORT: &'static str = "7878";

    pub fn new(client_id: &'a str, client_secret: &'a str, refresh_uri: &'a str) -> TokenFetcher<'a> {
        let mut rng = rand::thread_rng();
        let code_verifier: Vec<u8> = (0..128)
            .map(|_| {
                let i = rng.gen_range(0..CHARS.len());
                CHARS[i]
            })
            .collect();
        let code_challenge = TokenFetcher::gen_code_challenge(&code_verifier);
        let redirect_uri = format!("http://{}:{}", TokenFetcher::HOST, TokenFetcher::PORT);
        let auth_uri = TokenFetcher::build_auth_url(&client_id, &code_challenge, &redirect_uri);
        TokenFetcher {
            client_id,
            client_secret,
            code_verifier: String::from_utf8(code_verifier).unwrap(),
            auth_uri: auth_uri,
            redirect_uri: redirect_uri,
            refresh_uri: refresh_uri,
        }
    }

    fn gen_code_challenge(code_verifier: &Vec<u8>) -> String {
        let mut sha = Sha256::new();
        sha.update(code_verifier);
        let sha_hash = sha.finalize();
        encode_config(sha_hash, base64::URL_SAFE_NO_PAD)
    }

    fn build_auth_url(client_id: &str, code_challenge: &str, redirect_uri: &str) -> reqwest::Url {
        let mut url = reqwest::Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
        url.query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", "https://www.googleapis.com/auth/photoslibrary.readonly",)
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256");
        url
    }

    fn start(&self, tx: mpsc::Sender<String>) {
        let m = Mutex::new(Some(tx.clone()));
        let mut server = simple_server::Server::new(move |request, mut response| {
            println!("Request received. {} {}", request.method(), request.uri());
            let code = extract_code(request);
            match code {
                Some(x) => {
                    if let Some(tx) = m.lock().unwrap().take() {
                        tx.send(x).unwrap();
                    }
                    Ok(response.body(format!("Authorization complete.").as_bytes().to_vec())?)
                }
                None => Ok(response.body("Ok".as_bytes().to_vec())?),
            }
        });
        server.dont_serve_static_files();
        thread::spawn(move || {
            server.listen(TokenFetcher::HOST, TokenFetcher::PORT);
        });
    }

    fn stop(&self) {}

    pub fn fetch_refresh(&self) -> Result<String> {
        let (tx, rx) = mpsc::channel();
        self.start(tx);
        println!(
            "Open your browser and authorize access:\n  {}",
            self.auth_uri.as_str()
        );
        let code = rx.recv().unwrap();
        println!("Code was: {}", code);
        self.stop();
        self.refresh([("client_id", self.client_id),
                 ("client_secret", self.client_secret),
                 ("code", &code),
                 ("code_verifier", &self.code_verifier),
                 ("grant_type", "authorization_code"),
                 ("redirect_uri", &self.redirect_uri),],
                "refresh_token")
    }

    pub fn fetch_access(&self, refresh_token: &str) -> Result<String> {
        self.refresh(
            [("client_id", &self.client_id),
             ("client_secret", &self.client_secret),
             ("code", ""),
             ("code_verifier", ""),
             ("grant_type", "refresh_token"),
             ("refresh_token", &refresh_token),],
            "access_token")
    }

    fn refresh(&self, params: [(&str, &str); 6], field: &str) -> Result<String> {
        // println!("params={:?}", params);
        let client = reqwest::blocking::Client::new();
        let request = client.post(self.refresh_uri)
            .form(&params);
        let result = request.send();
        match result {
            Ok(r) => {
                let refresh_value: serde_json::Value = serde_json::from_str(&r.text().unwrap()).unwrap();
                // println!("field={}", field);
                // println!("text={}", refresh_value);
                let value = refresh_value[field].as_str().unwrap();
                // println!("refresh_value={}", value);
                Ok(String::from(value))
            },
            Err(_) => Err(Error::FetchError),
        }
    }
}


fn extract_code<T>(request: simple_server::Request<T>) -> Option<String> {
    let path_query = request.uri().path_and_query()?;
    let query = path_query.query()?;
    let re = Regex::new(r"code=(.*?)(&|$)").unwrap();
    let captures = re.captures(query).unwrap();
    Some(String::from(captures.get(1).unwrap().as_str()))
}

#[test]
fn test_extract_code() {
    let request = simple_server::Request::builder()
        .uri("http://127.0.0.1:7878/?code=abcdefg&scope=some_scope")
        .body(())
        .unwrap();
    let result = extract_code(request).unwrap();
    assert_eq!("abcdefg", result)
}

#[test]
fn test_extract_code_at_end() {
    let request = simple_server::Request::builder()
        .uri("http://127.0.0.1:7878/?scope=some_scope&code=abcdefg")
        .body(())
        .unwrap();
    let result = extract_code(request).unwrap();
    assert_eq!("abcdefg", result)
}

#[test]
#[should_panic]
fn test_extract_code_missing() {
    let request = simple_server::Request::builder()
        .uri("http://127.0.0.1:7878/?error=barf&scope=some_scope")
        .body(())
        .unwrap();
    extract_code(request);
}


impl<'a> MediaFetcher<'a> {

    pub fn new(base_uri: &'a str, access_token: &'a str, album_dir: &'a PathBuf) -> MediaFetcher<'a> {
        MediaFetcher {
            base_uri,
            access_token,
            album_dir,
        }
    }

    pub fn fetch_media(&self, number: u32) -> Result<Album> {
        let client = reqwest::blocking::Client::new();
        let uri = format!("{}/v1/mediaItems", self.base_uri);
        // println!("self.access_token={}", self.access_token);
        let bearer_token = format!("Bearer {}", self.access_token);
        let mut album = self.fetch_next(&client, &uri, &bearer_token, PAGE_SIZE, None)?;
        let mut total = album.media_items.len();
        let number_us: usize =  number.try_into().unwrap();
        while album.next_page_token.is_some() && total < number_us {
            let next_album = self.fetch_next(&client, &uri, &bearer_token, PAGE_SIZE,
                album.next_page_token)?;
            total += next_album.media_items.len();
            album.media_items.extend(next_album.media_items.into_iter());
            album.next_page_token = next_album.next_page_token;
            thread::sleep(PAUSE_FETCH);
        }

        return Ok(album);
    }

    fn fetch_next(&self, client: &reqwest::blocking::Client, uri: &str,
                    bearer_token: &str, page_size: u32,
                    next_page: Option<String>) -> Result<Album> {
        let mut query = vec![("pageSize", page_size.to_string())];

        match next_page {
            Some(token) => query.push(("pageToken", token)),
            None => (),
        }
        // println!("query={:?}", query);
        let album_response = client.get(uri)
            .header("Authorization", bearer_token)
            .query(&query)
            .send()
            .unwrap()
            .text()
            .unwrap();
        // println!("album={}", album_response);
        let album: Album = serde_json::from_str(&album_response)?;
        // println!("album.next_page_token={}", album.next_page_token);
        Ok(album)
    }

    pub fn write_media(&self, album: Album) -> Result<u64> {
        let path = PathBuf::from(self.album_dir);
        let mut i = 0;
        let written = album.media_items.iter()
            .fold(0, |accum, media| {
                println!("{}/{}", i, album.media_items.len());
                i += 1;
                return accum + self.write_file(&mut path.clone(), media).unwrap();
            });
        Ok(written)
    }

    fn write_file(&self, dir: &mut PathBuf, media: &Media) -> Result<u64> {
        dir.push(&media.filename);
        // println!("path={}", path.display());
        let mut file = File::create(dir.as_path()).unwrap();
        match reqwest::blocking::get(&media.base_url) {
            Err(_) => Err(Error::IOError),
            Ok(mut response) => {
                let file_len = response.copy_to(&mut file).unwrap();
                thread::sleep(PAUSE_WRITE);
                Ok(file_len)
            }
        }
    }
}
