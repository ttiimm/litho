use base64::encode_config;
use chrono::{NaiveDateTime, Datelike};
use rand::Rng;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tiny_http;

use std::convert::TryInto;
use std::fs::{self, File, create_dir_all};
use std::path::{PathBuf, Path};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::Duration;
use std::vec::Vec;


const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
    abcdefghijklmnopqrstuvwxyz\
    0123456789-.~_";

const PAGE_SIZE: u32 = 25;

const PAUSE_FETCH: Duration = Duration::from_secs(1);
const PAUSE_WRITE: Duration = Duration::from_millis(250);


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
    start_filter: YearMonthDay,
    end_filter: YearMonthDay
}


pub struct MediaWriter<'a> {
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


#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct YearMonthDay {
    pub year: i32,
    pub month: u32,
    pub day: u32,
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
            auth_uri,
            redirect_uri,
            refresh_uri,
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
        let server = tiny_http::Server::http(format!("{}:{}", TokenFetcher::HOST, TokenFetcher::PORT))
            .unwrap();

        let m = Mutex::new(Some(tx.clone()));
        thread::spawn(move || {
            for request in server.recv() {
                println!("Request received. {} {}", request.method(), request.url());
                let code = extract_code(request.url());
                let result: Result<&str> = match code {
                    Some(x) => {
                        if let Some(tx) = m.lock().unwrap().take() {
                            tx.send(x).unwrap();
                        }
                        Ok("Authorization complete.")
                    }
                    None => Ok("Authorization failed."),
                };
                let response = tiny_http::Response::from_string(result.unwrap());
                let _ = request.respond(response);
            }
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
        // println!("Code was: {}", code);
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


fn extract_code(url: &str) -> Option<String> {
    let re = Regex::new(r"code=(.*?)(&|$)").unwrap();
    let captures = re.captures(url).unwrap();
    Some(String::from(captures.get(1).unwrap().as_str()))
}

impl<'a> MediaFetcher<'a> {

    pub fn new(base_uri: &'a str, access_token: &'a str, start_filter: YearMonthDay, end_filter: YearMonthDay) -> MediaFetcher<'a> {
        MediaFetcher {
            base_uri,
            access_token,
            start_filter,
            end_filter
        }
    }

    pub fn fetch_media(&self, number: u32) -> Result<Vec<Media>> {
        let client = reqwest::blocking::Client::new();
        let uri = format!("{}/v1/mediaItems:search", self.base_uri);
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

        return Ok(album.media_items);
    }

    fn fetch_next(&self, client: &reqwest::blocking::Client, uri: &str,
                    bearer_token: &str, page_size: u32,
                    next_page: Option<String>) -> Result<Album> {
        let mut body = json!({
            "orderBy": "MediaMetadata.creation_time",
            "filters": {
                "dateFilter": {
                    "ranges": [{"startDate": self.start_filter,
                                "endDate": self.end_filter}]
                }
            },
            "pageSize": page_size
        });

        match next_page {
            Some(token) => body["pageToken"] = json!(token),
            None => (),
        }
        // println!("{}", body.to_string());
        let album_response = client.post(uri)
            .header("Authorization", bearer_token)
            .body(body.to_string())
            .send()
            .unwrap()
            .text()
            .unwrap();
        // println!("album={}", album_response);
        let album: Album = serde_json::from_str(&album_response)?;
        // println!("album.next_page_token={}", album.next_page_token);
        Ok(album)
    }
}


pub fn most_recent_date(mut base: PathBuf) -> Option<YearMonthDay> {
    let year = last_entry(base.as_path())?;
    base.push(year.as_str());
    let month = last_entry(base.as_path())?;
    base.push(month.as_str());
    let day = last_entry(base.as_path())?;
    // println!("ymd: {} {} {}", year, month, day);
    Some(YearMonthDay{
        year: year.parse::<i32>().unwrap(),
        month: month.parse::<u32>().unwrap(),
        day: day.parse::<u32>().unwrap()
    })
}


fn last_entry(base: &Path) -> Option<String> {
    // println!("base: {}", base.display());
    let paths = fs::read_dir(base);
    match paths {
        Ok(paths) => { 
            let mut sorted: Vec<_> = paths.map(|r| r.unwrap()).collect();
            sorted.sort_by_key(|entry| entry.path());
            let result = if sorted.len() > 0 {
                Some(sorted.last().unwrap().file_name().to_string_lossy().to_string())
            } else {
                None
            };
            result
        },
        _ => None
    }
}


impl<'a> MediaWriter<'a> {

    pub fn new(album_dir: &'a PathBuf) -> MediaWriter<'a> {
        MediaWriter {
            album_dir,
        }
    }

    pub fn write_media(&self, media: Vec<Media>, number: u32) -> Result<u64> {
        let path = PathBuf::from(self.album_dir);
        let mut i = 0;
        let written = media.iter()
            .fold(0, |accum, media| {
                if i == number {
                    return accum;
                }
                print!("[{}/{}]\t", i + 1, number);
                i += 1;
                let written = self.write_file(&mut path.clone(), media).unwrap();
                return accum + written
            });
        Ok(written)
    }

    fn write_file(&self, dir: &mut PathBuf, media: &Media) -> Result<u64> {
        let created_on = NaiveDateTime::parse_from_str(
            media.media_metadata.creation_time.as_str(), "%Y-%m-%dT%H:%M:%S%Z").unwrap();
        // println!("created_on {}", created_on);
        let year = created_on.year().to_string();
        dir.push(&year);
        let month = created_on.month();
        dir.push(format!("{:02}", month));
        let day = created_on.day();
        dir.push(format!("{:02}", day));
        create_dir_all(dir.as_path()).unwrap();
        println!("{}/{}/{}", &year, month, day);
        // println!("{}/{}/{} {}", &year, month, day, media.id);
        dir.push(&media.filename);
        // println!("path={:?}", dir.as_path());
        if dir.exists() {
            return Ok(0);
        } 
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


#[cfg(test)]
mod tests {

    use std::fs;
    use std::path::PathBuf;
    use std::path::Path;
    use tempfile::tempdir;

    use crate::YearMonthDay;

    use super::extract_code;
    use super::most_recent_date;

    #[test]
    fn test_most_recent() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        create_all_dirs(temp_dir.path(), "2023", "09", "30");
        create_all_dirs(temp_dir.path(), "2023", "09", "29");
        create_all_dirs(temp_dir.path(), "2023", "08", "30");
        create_all_dirs(temp_dir.path(), "2022", "07", "28");
        let mut base = PathBuf::from(temp_dir.path());
        base.push("photos");
        let result = most_recent_date(base).unwrap();
        let expected = YearMonthDay {
            year: 2023,
            month: 9,
            day: 30,
        };
        assert_eq!(expected, result);
        Ok(())
    }

    #[test]
    fn test_most_recent_empty() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let mut base = PathBuf::from(temp_dir.path());
        base.push("photos");
        fs::create_dir_all(&base).unwrap();
        let result = most_recent_date(base);
        assert_eq!(None, result);
        Ok(())
    }

    fn create_all_dirs(base: &Path, year: &str, month: &str, day: &str) {
        let mut temp_path = PathBuf::from(base);
        temp_path.push("photos");
        temp_path.push(year);
        temp_path.push(month);
        temp_path.push(day);
        fs::create_dir_all(&temp_path).unwrap();
    }

    #[test]
    fn test_extract_code() {
        let result = extract_code("http://127.0.0.1:7878/?code=abcdefg&scope=some_scope")
            .unwrap();
        assert_eq!("abcdefg", result)
    }

    #[test]
    fn test_extract_code_at_end() {
        let result = extract_code("http://127.0.0.1:7878/?scope=some_scope&code=abcdefg")
            .unwrap();
        assert_eq!("abcdefg", result)
    }

    #[test]
    #[should_panic]
    fn test_extract_code_missing() {
        extract_code("http://127.0.0.1:7878/?error=barf&scope=some_scope");
    }
}
