# aNotify

Async Bindings for the iNotify api

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
