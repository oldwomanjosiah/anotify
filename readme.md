# aNotify

Async Bindings for the iNotify api

This crate is still a work in progress! The core functionality is there, but it's not all there (and some claimed features are not currently functional), so I wouldn't reccomend using it _yet_.

```rust
extern crate anotify;
extern crate eyre;

let mut owner = anotify::new()
    .wrap_err("Creating anotify instance")?;

let file_watch = owner.file(PathBuf::from("./readme.md"))?
    .open(true)
    .watch()?;

file_watch.await
    .wrap_err("anoitfy closed before readme was opened")?;

let directory_watch = owner.dir(PathBuf::from("./src/"))?
    .modify(true)
    .watch()?;

while let Some(event) = directory_watch.next().await
    .wrap_err("anotify closed before any directory events seen")? {
    println!("Got: {event}");
}
```
