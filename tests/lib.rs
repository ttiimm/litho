use httpmock::MockServer;
use httpmock::Method::*;
use serde_json::json;

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
fn test_fetch_media() {
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/mediaItems")
            .query_param("pageSize", "1")
            .header("Authorization", "Bearer myaccesstoken");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({"mediaItems": [
                {"baseUrl": "myurl",
                 "filename": "foo"}]}));
    });

    let mock_endpoint = server.url("");
    let mf = litho::MediaFetcher::new(&mock_endpoint, "myaccesstoken");
    let result: litho::Album = mf.fetch_media().unwrap();

    mock.assert();
    assert_eq!("foo", result.media_items[0].filename);
    assert_eq!("myurl", result.media_items[0].base_url)
}
