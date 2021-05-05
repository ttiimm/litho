use reqwest;

use std::env;
use std::fs::File;
use std::path::Path;


fn main() -> Result<(), litho::TokenError> {
    let client_id = env!("CLIENT_ID");
    let client_secret = env!("CLIENT_SECRET");
    let refresh_uri = "https://oauth2.googleapis.com/token";
    let username = whoami::username();
    let keyring = keyring::Keyring::new(&client_id, &username);

    if env::var("CLEAR_TOKEN").is_ok() {
        keyring.delete_password();
    }

    let token_fetcher = litho::TokenFetcher::new(&client_id, &client_secret, &refresh_uri);

    let refresh_token: Result<String, litho::TokenError> = keyring.get_password().or_else(|_| {
        println!("Token not found, authorizing");
        let to_store = token_fetcher.fetch_refresh().unwrap();
        // println!("to_store={}", to_store);
        keyring.set_password(&to_store);
        Ok(to_store)
    });

    let access_token = token_fetcher.fetch_access(&refresh_token.unwrap()).unwrap();

    let client = reqwest::blocking::Client::new();
    let album_response = client
        .get("https://photoslibrary.googleapis.com/v1/mediaItems")
        .header("Authorization", format!("Bearer {}", access_token).as_str())
        .query(&[("pageSize", "1")])
        .send()
        .unwrap()
        .text()
        .unwrap();
    let album: serde_json::Value = serde_json::from_str(&album_response).unwrap();
    let media_item = &album["mediaItems"][0];
    let base_url = media_item["baseUrl"].as_str().unwrap();
    let filename = media_item["filename"].as_str().unwrap();

    write_pic(filename, base_url);
    Ok(())
}

fn write_pic(filename: &str, base_url: &str) -> Result<u64, litho::TokenError> {
    let path = Path::new(filename);
    let mut file = File::create(path).unwrap();
    match reqwest::blocking::get(base_url) {
        Err(_) => Err(litho::TokenError),
        Ok(mut response) => {
            let file_len = response.copy_to(&mut file).unwrap();
            Ok(file_len)
        }
    }
}
