# Litho
As a parent of two young children, I use Google photos extensibly to back up and make 
all my photos easily accessible on all devices and to share with family and friends.
I've never been a fan of sharing on social media, so appreciate having an easy to use
service that lets me maintain privacy.

In the last couple years, Google has made it increasingly difficult to easily synchronize 
photo back ups from their service onto local storage. Their Backup and Sync application 
will readily upload files from a computer or phone, but no longer supports automatically 
downloading them to a local drive. This is my attempt to fill that gap and make it easy for 
people to back up their photos from Google onto local storage.

This has also been a learning project to get started with the Rust programming language
and am using the project as a way to learn more about it. I've found the project is
complex enough to expose me to a variety of technical challenges, but nothing too complicated,
aside from the expected fights with the compiler :D

## What works
The project is currently in development and so far I've mostly been working on getting 
something working end-to-end.

When run the program checks for a refresh token (stored via [keyring-rs](https://github.com/hwchen/keyring-rs))
If found, then it uses that to gain an access token and downloads the most recent `n` photo(s) in
the Google photo account to the current directory.

If no refresh token is available, then it'll ask the user to open the browser to begin granting
access to the user's Google photos account. This will result in gaining a refresh token, storing
it, and proceeding to start downloading the specified number of media items. The media items
are fetched from latest to earliest taken.

I'm currently working on an efficient way to synchronize the metadata stored on disk with what is
stored on Google.

## Building it
I will eventually publish some binaries of the application, but for now if you want to try it you
have to build it yourself.

```
$ cargo build
```

When the application is run, it expects the `CLIENT_ID` and `CLIENT_SECRET` to be set as environment
variables, otherwise it'll panick. When I publish a binary, I'll bake the ones I use into the binary
but until then, you'll need to obtain some from Google and set them yourself.

When running you can specify the amount of media to download via the `number` argument.

```
litho 0.1.0

USAGE:
    litho.exe <number>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <number>
```
