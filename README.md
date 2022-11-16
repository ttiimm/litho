# Litho

As a parent of young children, I use Google photos extensibly to back up and make 
all my photos easily accessible on all devices as well as to share with family and friends.
I've never been a fan of sharing on social media, so I appreciate having an easy to use
service that lets me maintain privacy.

In the last couple years, Google has made it increasingly difficult to easily synchronize 
photo back ups from their service onto local storage. Their Backup and Sync application 
will readily upload files from a computer or phone, [but no longer supports automatically 
downloading](https://support.google.com/photos/answer/9316089) them to a local drive. This 
is my attempt to fill that gap and make it easy for people to back up their photos from Google
onto local storage.

An alternative is to use Google's take out, but feels pretty heavy handed and is asynchronous
causing one to wait for the zip to be ready. I'd prefer to fetch all missing photos to local 
storage periodically.

This has also been a learning project to get started with the Rust programming language
and I am using the project as a way to learn more about it. I've found the project is
complex enough to expose me to a variety of technical challenges, but nothing too complicated,
aside from the expected fights with the compiler :D

## What works
The program is now working end-to-end well enough that I was able to transfer my Google library.

Here's the basic flow.

When the program is run, it checks for a refresh token (stored via [keyring-rs](https://github.com/hwchen/keyring-rs))
If found, then it uses that to fetch an access token. 

If no refresh token is available, then it'll ask the user to open the browser to begin granting
access to the user's Google photos account. This will result in gaining an refresh and access token, storing
the refresh token, and proceeding to start downloading the specified number of media items. The media items
are fetched from latest to most recent.

When the download commences, the application will start fetching the metadata of the user's library 
starting from the oldest photo forward.

Simultanesouly it will start downloading the media to the current working directory under a 
directory called `photos`. By convention each photo is stored under a directory based on the date 
it was created and the file name, so if a photo was taken on March 3, 2018 with the name photo.jpg, 
then it will be stored at the path

`$CWD/photos/2018/03/03/photo.jpg`

The application runs until the specified limit (or all) media are downloaded. If the application exits
before all media are downloaded, then when run again it will resume from the most recent item. Any items
with duplicate names are considered to be the same and not re-downloaded. If photos were added prior to
the last item downloaded, then those will not be fetched.

## Usage

```
litho 0.1.0
A utility for fetching photos from Google.

Photos will be downloaded into the current directory into the path:

$CWD/photos/yyyy/mm/dd/file-name.jpg

where the directory structure is based off the date the photo was taken (or uploaded on) and the original file name. The
Google Photo API may not return the media at the original resolution and will not include all orginal metadata like the
geolocation. Use Google Take Out to fetch the original if a true back up is desired.

USAGE:
    litho [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -l, --limit <limit>    an optional limit of the number of photos to fetch
```

## Building it

[![Rust-CI/CD](https://github.com/ttiimm/litho/actions/workflows/build.yml/badge.svg)](https://github.com/ttiimm/litho/actions/workflows/build.yml)

```
$ CLIENT_ID=XXX CLIENT_SECRET=YYY cargo build
```

When the application is run, it expects the `CLIENT_ID` and `CLIENT_SECRET` to be set as environment
variables, otherwise it'll panic. The published binaries, have some baked in, but if you plan to build
independently then you'll need to grab some from [Google](https://developers.google.com/learn/pathways/gcp/get-started-projects).

For regular development, I use a `build.rs` file this in order to avoid having to set each time. 

```
fn main() {
    println!("cargo:rustc-env=CLIENT_ID=XXX);
    println!("cargo:rustc-env=CLIENT_SECRET=YYY);
}
```
