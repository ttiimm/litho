use httpmock::{MockServer, Regex};
use httpmock::Method::*;
use serde_json::json;
use tempfile::tempdir;

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
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
            .query_param("pageSize", "25")
            .header("Authorization", "Bearer myaccesstoken");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "mediaItems": [
                    {"id": "abc123",
                     "baseUrl": "myurl",
                     "filename": "foo",
                     "mimeType": "image/jpeg",
                     "mediaMetadata": {
                        "creationTime": "2014-10-02T15:01:23.045123456Z"
                    }}],
            }));
    });

    let mock_endpoint = server.url("");
    let mf = litho::MediaFetcher::new(&mock_endpoint, "myaccesstoken");
    let result: litho::Album = mf.fetch_media(3).unwrap();

    mock.assert();
    assert_eq!(None, result.next_page_token);
    assert_eq!("foo", result.media_items[0].filename);
    assert_eq!("myurl", result.media_items[0].base_url);
    assert_eq!("abc123", result.media_items[0].id);
    Ok(())
}

#[test]
fn test_fetch_media_pagination() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start();

    let mock_first = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/mediaItems")
            .query_param("pageSize", "25")
            .header("Authorization", "Bearer myaccesstoken")
            .matches(|req| {
                !req.query_params
                    .as_ref()
                    .unwrap()
                    .iter()
                    .any(|(k, _)| k.eq("pageToken"))
            });
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "mediaItems": [
                    {"id": "abc123",
                     "baseUrl": "myurl",
                     "filename": "foo",
                     "mimeType": "image/jpeg",
                     "mediaMetadata": {
                        "creationTime": "2014-10-02T15:01:23.045123456Z"
                     }}],
                "nextPageToken": "the_next_page"
                }));
    });

    let mock_last = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/mediaItems")
            .query_param("pageSize", "25")
            .query_param("pageToken", "the_next_page")
            .header("Authorization", "Bearer myaccesstoken");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "mediaItems": [
                    {"id": "xyz123",
                     "baseUrl": "anotherurl",
                     "filename": "bar",
                     "mimeType": "image/jpeg",
                     "mediaMetadata": {
                        "creationTime": "2013-10-02T15:01:23.045123456Z"
                     }}],
                }));
    });

    let mock_endpoint = server.url("");
    let mf = litho::MediaFetcher::new(&mock_endpoint, "myaccesstoken");
    let result: litho::Album = mf.fetch_media(3).unwrap();

    mock_first.assert();
    mock_last.assert();

    assert_eq!(None, result.next_page_token);

    // first request
    assert_eq!("foo", result.media_items[0].filename);
    assert_eq!("myurl", result.media_items[0].base_url);
    assert_eq!("abc123", result.media_items[0].id);

    // last request
    assert_eq!("bar", result.media_items[1].filename);
    assert_eq!("anotherurl", result.media_items[1].base_url);
    assert_eq!("xyz123", result.media_items[1].id);

    Ok(())
}

#[test]
fn test_write_media() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start();
    let binary_content = b"\xca\xfe\xba\xbe";

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new(r#"/v1/mediaItems/.*"#).unwrap());
        then.status(200)
            .body(binary_content);
    });

    let temp_dir = tempdir()?;
    let temp_path_buf = PathBuf::from(temp_dir.path());
    let media_writer = litho::MediaWriter::new(&temp_path_buf);
    let album = init_album(&server);
    let result = media_writer.write_media(album, 2);

    mock.assert_hits(2);
    assert_eq!(8, result.unwrap());

    let mut path_buf_test = temp_path_buf.clone();
    path_buf_test.push("2014/10/02/test.jpg");
    // println!("path={:?}", path_buf_test.as_path());
    assert_write_media(&path_buf_test, &binary_content);

    let mut path_buf_camping = temp_path_buf.clone();
    path_buf_camping.push("2014/10/03/camping.jpg");
    assert_write_media(&path_buf_camping, &binary_content);
    Ok(())
}

#[test]
fn test_write_media_when_album_has_more() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start();
    let binary_content = b"\xca\xfe\xba\xbe";

    let mock = server.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new(r#"/v1/mediaItems/.*"#).unwrap());
        then.status(200)
            .body(binary_content);
    });

    let temp_dir = tempdir()?;
    let temp_path_buf = PathBuf::from(temp_dir.path());
    let media_writer = litho::MediaWriter::new(&temp_path_buf);
    let album = init_album(&server);
    let result = media_writer.write_media(album, 1);

    mock.assert();
    assert_eq!(4, result.unwrap());

    let mut path_buf_test = temp_path_buf.clone();
    path_buf_test.push("2014/10/02/test.jpg");
    // println!("path={:?}", path_buf_test.as_path());
    assert_write_media(&path_buf_test, &binary_content);

    let mut path_buf_camping = temp_path_buf.clone();
    path_buf_camping.push("2014/10/03/camping.jpg");
    assert!(!path_buf_camping.as_path().exists());
    Ok(())
}

fn assert_write_media(file_to_check: &PathBuf, binary_content: &[u8; 4]) {
    let mut file = File::open(file_to_check).expect("Unable to open result file");
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect("Unable to read the file");
    let buffer_contents = &buffer.as_ref();
    assert_eq!(binary_content, buffer_contents);
}

fn init_album(server: &MockServer) -> litho::Album {
    let mut media_items = Vec::new();

    let base_url_test = server.url("/v1/mediaItems/123");
    let metadata_test = litho::MediaMetadata{
        creation_time:  String::from("2014-10-02T15:01:23.045123456Z")
    };
    let test_pic = litho::Media{
        id:  String::from("abc123"),
        media_metadata: metadata_test,
        mime_type: String::from("image/jpeg"),
        base_url: base_url_test.clone(),
        filename: String::from("test.jpg"),
    };

    let base_url_camping = server.url("/v1/mediaItems/456");
    let metadata_camping = litho::MediaMetadata{
        creation_time:  String::from("2014-10-03T15:01:23.045123456Z")
    };
    let camping_pic = litho::Media{
        id:  String::from("abc123"),
        media_metadata: metadata_camping,
        mime_type: String::from("image/jpeg"),
        base_url: base_url_camping.clone(),
        filename: String::from("camping.jpg"),
    };
    media_items.push(test_pic);
    media_items.push(camping_pic);
    litho::Album{
        media_items: media_items, 
        next_page_token: None
    }
}
