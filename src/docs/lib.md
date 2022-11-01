# aNotify

Watch files and directories for filesystem changes.

### Watching a file for changes

```no_run
# use anotify::*;
# use anotify::events::EventFilterType;
# use futures::{Stream, StreamExt};
# #[tokio::main]
# async fn main() -> errors::Result<()> {
// Get an anotify instance with the default platform bindings
let anotify = Anotify::new()?;

// Start watching a file
let stream = anotify.watch(
    "src/lib.rs",
    events::EventFilterType::Write
).await?;

while let Some(event) = anotify.next() {
    eprintln!("Got Event: {:?}", event?);
}
# }
```

### Using a custom platform

```ignore
# use anotify::*;
# use anotify::events::EventFilterType;
# use futures::{Stream, StreamExt};
# #[tokio::main]
# async fn main() -> errors::Result<()> {
// Get an anotify instance with the default platform bindings
let anotify = Anotify::builder()
    .with_platform::<TestPlatform>()
    .with_buffer(128)
    .with_runtime(tokio::runtime::Handle::current())
    .build()?;

// Start watching a file
let event = anotify.next(
    "src/lib.rs",
    events::EventFilterType::Write
).await?;

if let Ok(event) = event.await {
    eprintln!("Got Event: {:?}", event);
}
# }
```
