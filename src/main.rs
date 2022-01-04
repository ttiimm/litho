use std::env;
use std::fs;
use structopt::StructOpt;


#[derive(StructOpt)]
struct Cli {
    number: u32
}

fn main() -> Result<(), litho::Error> {
    let client_id = env!("CLIENT_ID");
    let client_secret = env!("CLIENT_SECRET");

    let args = Cli::from_args();

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
    let mut photos_dir = env::current_dir().unwrap();
    photos_dir.push("photos");
    fs::create_dir_all(&photos_dir).unwrap();
    let media_fetcher = litho::MediaFetcher::new(
        "https://photoslibrary.googleapis.com", &access_token, &photos_dir);
    let album = media_fetcher.fetch_media(args.number)?;
    media_fetcher.write_media(album).unwrap();
    Ok(())
}
