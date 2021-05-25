use httpmock::MockServer;
use httpmock::Method::*;
use serde_json::json;
use tempfile::NamedTempFile;

use std::env;
use std::fs::File;
use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[test]
fn test_fetch_refresh() {
    let server = MockServer::start();
    let mock_endpoint = server.url("/token");
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || { 
        let tf = litho::TokenFetcher::new("myclientid", "myclientsecret", &mock_endpoint);
        let refresh_token = tf.fetch_refresh().unwrap();
        tx.send(refresh_token).unwrap(); 
    });

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/token")
            .body_contains("code=mycode");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({"refresh_token": "yourrefreshtoken"}));
    });
    let redirect_uri = format!("http://localhost:7878?code=mycode");
    reqwest::blocking::Client::new()
        .get(redirect_uri).send().unwrap();
    let result = rx.recv_timeout(Duration::from_secs(3)).unwrap();

    mock.assert();
    assert_eq!("yourrefreshtoken", result)
}

#[test]
fn test_fetch_access() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/token")
            .body_contains("refresh_token=myrefreshtoken");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({"access_token": "youraccesstoken"}));
    });

    let mock_endpoint = &server.url("/token");
    let tf = litho::TokenFetcher::new("myclientid", "myclientsecret", mock_endpoint);
    let result = tf.fetch_access("myrefreshtoken").unwrap();

    mock.assert();
    assert_eq!("youraccesstoken", result)
}

#[test]
fn test_fetch_media() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/mediaItems")
            .query_param("pageSize", "1")
            .header("Authorization", "Bearer myaccesstoken");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({"mediaItems": [
                {"id": "abc123",
                 "baseUrl": "myurl",
                 "filename": "foo"}]}));
    });

    let mock_endpoint = server.url("");
    let cwd = env::current_dir()?;
    let mf = litho::MediaFetcher::new(&mock_endpoint, "myaccesstoken", &cwd);
    let result: litho::Album = mf.fetch_media().unwrap();

    mock.assert();
    assert_eq!("foo", result.media_items[0].filename);
    assert_eq!("myurl", result.media_items[0].base_url);
    assert_eq!("abc123", result.media_items[0].id);
    Ok(())
}

#[test]
fn test_write_media() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start();
    let binary_content = b"\xca\xfe\xba\xbe";

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/mediaItems/123");
        then.status(200)
            .body(binary_content);
    });

    let file = NamedTempFile::new()?;
    let temp_path =  file.path();
    let mut temp_path_buf = temp_path.parent().unwrap().to_path_buf();
    let mock_endpoint = server.url("");
    let media_fetcher = litho::MediaFetcher::new(&mock_endpoint, "myaccesstoken", &temp_path_buf);
    let mut media_items = Vec::new();
    let mock_base_url = server.url("/v1/mediaItems/123");
    let metadata = litho::MediaMetadata{
        creation_time:  String::from("2014-10-02T15:01:23.045123456Z")
    };
    let media_item = litho::Media{
        id:  String::from("abc123"),
        media_metadata: metadata,
        mime_type: String::from("image/jpeg"),
        base_url: mock_base_url,
        filename: String::from("test.jpg"),
    };
    media_items.push(media_item);
    let album = litho::Album{media_items};
    let result = media_fetcher.write_media(album);

    mock.assert();
    assert_eq!(4, result.unwrap());

    temp_path_buf.push("test.jpg");
    let mut file = File::open(temp_path_buf).expect("Unable to open result file");
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect("Unable to read the file");
    let buffer_contents = &buffer.as_ref();
    assert_eq!(binary_content, buffer_contents);
    Ok(())
}
