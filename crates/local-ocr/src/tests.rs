use super::*;
use std::time::Duration;

#[tokio::test]
async fn busy_and_timeouts_retain_slots_until_workers_finish() {
    let mut handles = Vec::new();
    let mut started = Vec::new();
    let mut releases = Vec::new();
    for _ in 0..MAX_CONCURRENT_JOBS {
        let (start, seen) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        started.push(seen);
        releases.push(release);
        handles.push(tokio::spawn(bounded(
            Instant::now() + Duration::from_millis(500),
            move || {
                let _ = start.send(());
                let _ = wait.recv_timeout(Duration::from_secs(3));
                Ok(())
            },
        )));
    }
    for start in started {
        tokio::time::timeout(Duration::from_secs(2), start)
            .await
            .unwrap()
            .unwrap();
    }
    assert_eq!(
        bounded(Instant::now() + Duration::from_secs(2), || Ok(())).await,
        Err(OcrError::Busy)
    );
    for handle in handles {
        assert_eq!(handle.await.unwrap(), Err(OcrError::DeadlineExceeded));
    }
    // Awaiters timed out, but both jobs are still really executing.
    assert_eq!(
        bounded(Instant::now() + Duration::from_secs(2), || Ok(())).await,
        Err(OcrError::Busy)
    );
    for release in releases {
        release.send(()).unwrap();
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        while job_slots().available_permits() != MAX_CONCURRENT_JOBS {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        bounded(Instant::now() + Duration::from_secs(2), || Ok(7)).await,
        Ok(7)
    );
}
