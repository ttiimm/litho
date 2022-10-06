use chrono::{Datelike, Local};
use litho::YearMonthDay;
use structopt::StructOpt;

use std::env;
use std::fs;


#[derive(StructOpt)]
struct Cli {
    number: Option<u32>
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

    let most_recent_path = photos_dir.clone();
    let start_filter = litho::most_recent_date(most_recent_path)
        .unwrap_or(YearMonthDay{year: 1970, month: 1, day: 1});
    let today = Local::today();
    let end_filter = YearMonthDay { year: today.year(), month: today.month(), day: today.day() };
    let media_fetcher = litho::MediaFetcher::new(
        "https://photoslibrary.googleapis.com", &access_token, start_filter, end_filter);
    let number = args.number.unwrap_or(u32::MAX);
    let media = media_fetcher.fetch_media(number)?;
    let media_writer = litho::MediaWriter::new(&photos_dir);
    media_writer.write_media(media, number).unwrap();
    Ok(())
}
