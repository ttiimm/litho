use std::env;


fn main() -> Result<(), litho::Error> {
    let client_id = env!("CLIENT_ID");
    let client_secret = env!("CLIENT_SECRET");
    let refresh_uri = "https://oauth2.googleapis.com/token";
    let username = whoami::username();
    let keyring = keyring::Keyring::new(&client_id, &username);

    if env::var("CLEAR_TOKEN").is_ok() {
        keyring.delete_password().unwrap();
    }

    let token_fetcher = litho::TokenFetcher::new(&client_id, &client_secret, &refresh_uri);

    let refresh_token: Result<String, litho::Error> = keyring.get_password().or_else(|_| {
        println!("Token not found, authorizing");
        let to_store = token_fetcher.fetch_refresh().unwrap();
        // println!("to_store={}", to_store);
        keyring.set_password(&to_store).unwrap();
        Ok(to_store)
    });

    let access_token = token_fetcher.fetch_access(&refresh_token.unwrap()).unwrap();
    let cwd = env::current_dir().unwrap();
    let media_fetcher = litho::MediaFetcher::new(
        "https://photoslibrary.googleapis.com", &access_token, &cwd);
    let album = media_fetcher.fetch_media()?;
    media_fetcher.write_media(album).unwrap();
    Ok(())
}
