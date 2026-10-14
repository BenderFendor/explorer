use notify::{EventKind, RecursiveMode, Watcher};
use std::{path::Path, time::Duration};

/// Calls `changed` after each burst of changes in `folder` until the future is dropped.
pub async fn folder(folder: &Path, mut changed: impl FnMut()) {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let Ok(mut watcher) =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok_and(|event| !matches!(event.kind, EventKind::Access(_))) {
                let _ = sender.send(());
            }
        })
    else {
        return;
    };
    if watcher.watch(folder, RecursiveMode::NonRecursive).is_err() {
        return;
    }
    while receiver.recv().await.is_some() {
        // Coalesce bursts such as a large paste into one refresh.
        tokio::time::sleep(Duration::from_millis(200)).await;
        while receiver.try_recv().is_ok() {}
        changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::tests::Fixture;

    #[test]
    fn reports_created_files_once_per_burst() {
        let fixture = Fixture::new();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = count.clone();
        let path = fixture.0.clone();
        let watching = runtime.spawn(async move {
            folder(&path, || {
                seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            })
            .await
        });
        std::thread::sleep(Duration::from_millis(150));
        for index in 0..20 {
            std::fs::write(fixture.0.join(format!("{index}")), "x").unwrap();
        }
        std::thread::sleep(Duration::from_millis(700));
        watching.abort();
        assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
}
