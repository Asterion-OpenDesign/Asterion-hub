//! Download an installer, resumably, and prove it is the file the signed catalog described.
//!
//! The file name on disk is derived from the SHA-256 and the package kind, never from the URL or from
//! anything the server says, so a hostile name cannot steer the file anywhere. Bytes go to `<name>.part`;
//! only a file whose size and hash both match is renamed to its final name. A killed process, a dropped
//! connection or a cancel leaves the `.part` behind, and the next attempt continues from it.

use crate::error::{HubError, Result};
use crate::manifest::PackageKind;
use crate::net::{classify, UrlPolicy};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use url::Url;

const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

pub struct DownloadSpec<'a> {
    pub url: &'a str,
    pub size: u64,
    pub sha256: &'a str,
    pub kind: PackageKind,
    pub dest_dir: &'a Path,
}

pub fn final_path(spec: &DownloadSpec) -> PathBuf {
    spec.dest_dir.join(format!("{}.{}", &spec.sha256[..32], spec.kind.extension()))
}

fn part_path(spec: &DownloadSpec) -> PathBuf {
    spec.dest_dir.join(format!("{}.{}.part", &spec.sha256[..32], spec.kind.extension()))
}

pub async fn sha256_file(path: &Path) -> Result<(String, u64)> {
    let mut f = tokio::fs::File::open(path).await.map_err(|e| HubError::io("open file to hash", e))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut n = 0u64;
    loop {
        let r = f.read(&mut buf).await.map_err(|e| HubError::io("read file to hash", e))?;
        if r == 0 {
            break;
        }
        h.update(&buf[..r]);
        n += r as u64;
    }
    Ok((hex::encode(h.finalize()), n))
}

/// Re-check a file that is already in the downloads directory. Called again right before an installer
/// runs: the window between "verified" and "executed" is where a local swap would happen.
pub async fn verify_file(path: &Path, size: u64, sha256: &str) -> Result<()> {
    let (hash, n) = sha256_file(path).await?;
    if n != size {
        return Err(HubError::SizeMismatch { expected: size, actual: n });
    }
    if hash != sha256 {
        return Err(HubError::HashMismatch { expected: sha256.to_string(), actual: hash });
    }
    Ok(())
}

/// Returns the path of the verified file. `progress(done, total)` is called as bytes arrive.
pub async fn download_verified(
    client: &reqwest::Client,
    policy: &UrlPolicy,
    spec: &DownloadSpec<'_>,
    cancel: &CancellationToken,
    mut progress: impl FnMut(u64, u64) + Send,
) -> Result<PathBuf> {
    let url = Url::parse(spec.url).map_err(|_| HubError::UrlRefused("artifact URL is not valid".into()))?;
    policy.check(&url)?;
    tokio::fs::create_dir_all(spec.dest_dir).await.map_err(|e| HubError::io("create downloads directory", e))?;

    let dest = final_path(spec);
    let part = part_path(spec);

    // An earlier, complete, verified download is reusable (repair without downloading again).
    if dest.exists() {
        match verify_file(&dest, spec.size, spec.sha256).await {
            Ok(()) => {
                progress(spec.size, spec.size);
                return Ok(dest);
            }
            Err(_) => {
                let _ = tokio::fs::remove_file(&dest).await;
            }
        }
    }

    // Continue an interrupted download if there is one.
    let mut offset = match tokio::fs::metadata(&part).await {
        Ok(m) if m.len() <= spec.size => m.len(),
        Ok(_) => {
            let _ = tokio::fs::remove_file(&part).await;
            0
        }
        Err(_) => 0,
    };

    let mut hasher = Sha256::new();
    if offset > 0 {
        let mut f = tokio::fs::File::open(&part).await.map_err(|e| HubError::io("reopen partial download", e))?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let r = f.read(&mut buf).await.map_err(|e| HubError::io("read partial download", e))?;
            if r == 0 {
                break;
            }
            hasher.update(&buf[..r]);
        }
    }

    if offset < spec.size {
        let mut req = client.get(url.clone());
        if offset > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let resp = tokio::select! {
            _ = cancel.cancelled() => return Err(HubError::Cancelled),
            r = req.send() => r.map_err(classify)?,
        };
        let status = resp.status();
        let mut file;
        if status == reqwest::StatusCode::PARTIAL_CONTENT && offset > 0 {
            let ok = resp
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("bytes "))
                .and_then(|v| v.split('-').next())
                .and_then(|v| v.parse::<u64>().ok())
                == Some(offset);
            if !ok {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(HubError::DownloadInterrupted("server answered a different range than requested".into()));
            }
            file = tokio::fs::OpenOptions::new().append(true).open(&part).await.map_err(|e| HubError::io("append to partial download", e))?;
        } else if status.is_success() {
            // 200: the server sent the whole file (it ignored Range, or there was nothing to resume).
            offset = 0;
            hasher = Sha256::new();
            file = tokio::fs::File::create(&part).await.map_err(|e| HubError::io("create partial download", e))?;
        } else {
            return Err(HubError::HttpStatus { status: status.as_u16(), what: "installer" });
        }

        // Refuse before downloading a gigabyte that cannot be right.
        if let Some(len) = resp.content_length() {
            if offset + len != spec.size {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(HubError::SizeMismatch { expected: spec.size, actual: offset + len });
            }
        }

        progress(offset, spec.size);
        let mut done = offset;
        let mut stream = resp.bytes_stream();
        loop {
            let next = tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = file.flush().await;
                    return Err(HubError::Cancelled);
                }
                n = tokio::time::timeout(IDLE_TIMEOUT, stream.next()) => n,
            };
            let chunk = match next {
                Err(_) => {
                    let _ = file.flush().await;
                    return Err(HubError::DownloadInterrupted("no data received for 30 seconds".into()));
                }
                Ok(None) => break,
                Ok(Some(Err(e))) => {
                    let _ = file.flush().await;
                    return Err(HubError::DownloadInterrupted(classify(e).to_string()));
                }
                Ok(Some(Ok(c))) => c,
            };
            done += chunk.len() as u64;
            if done > spec.size {
                drop(file);
                let _ = tokio::fs::remove_file(&part).await;
                return Err(HubError::SizeMismatch { expected: spec.size, actual: done });
            }
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(|e| HubError::io("write download", e))?;
            progress(done, spec.size);
        }
        file.flush().await.map_err(|e| HubError::io("flush download", e))?;
        file.sync_all().await.map_err(|e| HubError::io("sync download", e))?;
        drop(file);
        if done != spec.size {
            // Connection ended early. Keep the partial file: the next attempt resumes it.
            return Err(HubError::DownloadInterrupted(format!("connection closed after {done} of {} bytes", spec.size)));
        }
    }

    let actual = hex::encode(hasher.finalize());
    if actual != spec.sha256 {
        // Corrupt or substituted bytes must never be resumed or reused.
        let _ = tokio::fs::remove_file(&part).await;
        return Err(HubError::HashMismatch { expected: spec.sha256.to_string(), actual });
    }
    // A zero-length rename guard: the file on disk is re-measured, not assumed.
    let mut f = tokio::fs::File::open(&part).await.map_err(|e| HubError::io("reopen download", e))?;
    let on_disk = f.seek(std::io::SeekFrom::End(0)).await.map_err(|e| HubError::io("measure download", e))?;
    drop(f);
    if on_disk != spec.size {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(HubError::SizeMismatch { expected: spec.size, actual: on_disk });
    }
    tokio::fs::rename(&part, &dest).await.map_err(|e| HubError::io("finalize download", e))?;
    Ok(dest)
}

/// Remove downloaded installers and partial files, optionally keeping one verified file.
pub fn clean_downloads(dir: &Path, keep: Option<&Path>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if keep.is_some_and(|k| k == p) {
            continue;
        }
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
pub(crate) mod testserver {
    //! A deliberately small HTTP/1.1 server whose behaviour each test scripts. It exists because the
    //! interesting failures (ignored Range, truncated bodies, lying lengths) are exactly what library
    //! test servers refuse to produce.
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    pub struct Req {
        pub path: String,
        pub range_from: Option<u64>,
    }

    pub enum Reply {
        /// Honour Range properly.
        Body(Vec<u8>),
        /// Always send the whole body with 200, whatever Range says.
        IgnoreRange(Vec<u8>),
        /// Promise `declared` bytes, send `body`, then drop the connection.
        Truncated { body: Vec<u8>, declared: u64 },
        /// Send exactly this raw response.
        Status(u16),
        Redirect(String),
        /// Send `body` of a promised `declared` bytes, then go quiet without closing.
        Hang { body: Vec<u8>, declared: u64 },
    }

    pub struct Server {
        pub base: String,
        pub hits: Arc<AtomicUsize>,
    }

    impl Server {
        pub fn start(handler: impl Fn(&Req) -> Reply + Send + Sync + 'static) -> Server {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let hits = Arc::new(AtomicUsize::new(0));
            let h2 = hits.clone();
            let handler = Arc::new(handler);
            std::thread::spawn(move || {
                for conn in listener.incoming().flatten() {
                    let handler = handler.clone();
                    h2.fetch_add(1, Ordering::SeqCst);
                    std::thread::spawn(move || {
                        let _ = serve(conn, &*handler);
                    });
                }
            });
            Server { base, hits }
        }
        pub fn url(&self, path: &str) -> String {
            format!("{}{}", self.base, path)
        }
    }

    fn serve(mut conn: TcpStream, handler: &dyn Fn(&Req) -> Reply) -> std::io::Result<()> {
        let mut rd = BufReader::new(conn.try_clone()?);
        let mut line = String::new();
        rd.read_line(&mut line)?;
        let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
        let mut range_from = None;
        loop {
            let mut h = String::new();
            rd.read_line(&mut h)?;
            if h == "\r\n" || h.is_empty() {
                break;
            }
            if let Some(v) = h.to_ascii_lowercase().strip_prefix("range: bytes=") {
                range_from = v.trim().trim_end_matches('-').parse().ok();
            }
        }
        let req = Req { path, range_from };
        match handler(&req) {
            Reply::Body(b) => match req.range_from {
                Some(from) if (from as usize) < b.len() => {
                    let tail = &b[from as usize..];
                    write!(conn, "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n", tail.len(), from, b.len() - 1, b.len())?;
                    conn.write_all(tail)?;
                }
                Some(_) => write!(conn, "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?,
                None => {
                    write!(conn, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", b.len())?;
                    conn.write_all(&b)?;
                }
            },
            Reply::IgnoreRange(b) => {
                write!(conn, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", b.len())?;
                conn.write_all(&b)?;
            }
            Reply::Truncated { body, declared } => {
                write!(conn, "HTTP/1.1 200 OK\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n")?;
                conn.write_all(&body)?;
                conn.flush()?;
            }
            Reply::Status(code) => write!(conn, "HTTP/1.1 {code} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?,
            Reply::Redirect(to) => write!(conn, "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?,
            Reply::Hang { body, declared } => {
                write!(conn, "HTTP/1.1 200 OK\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n")?;
                conn.write_all(&body)?;
                conn.flush()?;
                std::thread::sleep(std::time::Duration::from_secs(120));
            }
        }
        conn.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::testserver::*;
    use super::*;
    use crate::net::build_client;
    use std::sync::Arc;

    fn payload(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 31 % 251) as u8).collect()
    }
    fn sha(b: &[u8]) -> String {
        hex::encode(Sha256::digest(b))
    }
    fn dev() -> (reqwest::Client, UrlPolicy) {
        let pol = UrlPolicy::new(vec![], true);
        (build_client(pol.clone()).unwrap(), pol)
    }
    fn spec<'a>(url: &'a str, body: &[u8], hash: &'a str, dir: &'a Path) -> DownloadSpec<'a> {
        DownloadSpec { url, size: body.len() as u64, sha256: hash, kind: PackageKind::Msi, dest_dir: dir }
    }

    #[tokio::test]
    async fn downloads_verifies_and_reports_real_progress() {
        let body = payload(600_000);
        let b2 = body.clone();
        let srv = Server::start(move |_| Reply::Body(b2.clone()));
        let (c, pol) = dev();
        let d = tempfile::tempdir().unwrap();
        let hash = sha(&body);
        let url = srv.url("/a.msi");
        let seen = std::sync::Mutex::new(Vec::new());
        let path = download_verified(&c, &pol, &spec(&url, &body, &hash, d.path()), &CancellationToken::new(), |a, b| seen.lock().unwrap().push((a, b))).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(path.file_name().unwrap().to_string_lossy(), format!("{}.msi", &hash[..32]));
        let seen = seen.into_inner().unwrap();
        assert_eq!(*seen.last().unwrap(), (600_000, 600_000));
        assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "progress never goes backwards");
        assert!(!d.path().join(format!("{}.msi.part", &hash[..32])).exists());
    }

    #[tokio::test]
    async fn a_hash_mismatch_deletes_the_bytes_and_fails() {
        let body = payload(10_000);
        let b2 = body.clone();
        let srv = Server::start(move |_| Reply::Body(b2.clone()));
        let (c, pol) = dev();
        let d = tempfile::tempdir().unwrap();
        let wrong = sha(b"something else");
        let url = srv.url("/a.msi");
        let err = download_verified(&c, &pol, &spec(&url, &body, &wrong, d.path()), &CancellationToken::new(), |_, _| {}).await.unwrap_err();
        assert_eq!(err.code(), "hash_mismatch");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0, "nothing left behind");
    }

    #[tokio::test]
    async fn a_body_longer_than_declared_is_refused() {
        let body = payload(10_000);
        let long = payload(20_000);
        let srv = Server::start(move |_| Reply::IgnoreRange(long.clone()));
        let (c, pol) = dev();
        let d = tempfile::tempdir().unwrap();
        let hash = sha(&body);
        let url = srv.url("/a.msi");
        let err = download_verified(&c, &pol, &spec(&url, &body, &hash, d.path()), &CancellationToken::new(), |_, _| {}).await.unwrap_err();
        assert_eq!(err.code(), "size_mismatch");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn an_interrupted_download_keeps_the_partial_file_and_the_retry_resumes_it() {
        let body = payload(500_000);
        let (b_full, b_cut) = (body.clone(), body[..200_000].to_vec());
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c2 = calls.clone();
        let srv = Server::start(move |req| {
            if c2.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Reply::Truncated { body: b_cut.clone(), declared: 500_000 }
            } else {
                assert_eq!(req.range_from, Some(200_000), "the retry asks only for what is missing");
                Reply::Body(b_full.clone())
            }
        });
        let (c, pol) = dev();
        let d = tempfile::tempdir().unwrap();
        let hash = sha(&body);
        let url = srv.url("/a.msi");
        let s = spec(&url, &body, &hash, d.path());
        let err = download_verified(&c, &pol, &s, &CancellationToken::new(), |_, _| {}).await.unwrap_err();
        assert_eq!(err.code(), "download_interrupted");
        assert!(err.retryable());
        let part = d.path().join(format!("{}.msi.part", &hash[..32]));
        assert_eq!(std::fs::metadata(&part).unwrap().len(), 200_000);
        let path = download_verified(&c, &pol, &s, &CancellationToken::new(), |_, _| {}).await.unwrap();
        assert_eq!(std::fs::read(path).unwrap(), body);
        assert!(!part.exists());
    }

    #[tokio::test]
    async fn a_server_that_ignores_range_restarts_cleanly() {
        let body = payload(300_000);
        let b2 = body.clone();
        let srv = Server::start(move |_| Reply::IgnoreRange(b2.clone()));
        let (c, pol) = dev();
        let d = tempfile::tempdir().unwrap();
        let hash = sha(&body);
        // A bogus partial file from an earlier run:
        std::fs::write(d.path().join(format!("{}.msi.part", &hash[..32])), &body[..1000]).unwrap();
        let url = srv.url("/a.msi");
        let path = download_verified(&c, &pol, &spec(&url, &body, &hash, d.path()), &CancellationToken::new(), |_, _| {}).await.unwrap();
        assert_eq!(std::fs::read(path).unwrap(), body);
    }

    #[tokio::test]
    async fn a_corrupt_partial_file_is_caught_by_the_final_hash_and_not_reused() {
        let body = payload(100_000);
        let b2 = body.clone();
        let srv = Server::start(move |_| Reply::Body(b2.clone()));
        let (c, pol) = dev();
        let d = tempfile::tempdir().unwrap();
        let hash = sha(&body);
        let mut junk = body[..40_000].to_vec();
        junk[5] ^= 0xff;
        let part = d.path().join(format!("{}.msi.part", &hash[..32]));
        std::fs::write(&part, junk).unwrap();
        let url = srv.url("/a.msi");
        let s = spec(&url, &body, &hash, d.path());
        let err = download_verified(&c, &pol, &s, &CancellationToken::new(), |_, _| {}).await.unwrap_err();
        assert_eq!(err.code(), "hash_mismatch");
        assert!(!part.exists(), "poisoned partial removed");
        // The next attempt starts from zero and succeeds.
        assert!(download_verified(&c, &pol, &s, &CancellationToken::new(), |_, _| {}).await.is_ok());
    }

    #[tokio::test]
    async fn http_errors_and_dead_servers_are_reported() {
        let body = payload(1000);
        let hash = sha(&body);
        let (c, pol) = dev();
        let d = tempfile::tempdir().unwrap();
        let srv = Server::start(|_| Reply::Status(404));
        let url = srv.url("/a.msi");
        let err = download_verified(&c, &pol, &spec(&url, &body, &hash, d.path()), &CancellationToken::new(), |_, _| {}).await.unwrap_err();
        assert!(matches!(err, HubError::HttpStatus { status: 404, .. }));
        // Nothing listens on this port.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = format!("http://{}/a.msi", l.local_addr().unwrap());
        drop(l);
        let err = download_verified(&c, &pol, &spec(&dead, &body, &hash, d.path()), &CancellationToken::new(), |_, _| {}).await.unwrap_err();
        assert_eq!(err.code(), "offline");
        assert!(err.retryable());
    }

    #[tokio::test]
    async fn urls_outside_the_policy_never_leave_the_machine() {
        let body = payload(1000);
        let hash = sha(&body);
        let c = build_client(UrlPolicy::new(vec!["releases.example.com".into()], false)).unwrap();
        let pol = UrlPolicy::new(vec!["releases.example.com".into()], false);
        let d = tempfile::tempdir().unwrap();
        for bad in ["http://releases.example.com/a.msi", "https://evil.example.org/a.msi", "https://u:p@releases.example.com/a.msi", "not a url"] {
            let err = download_verified(&c, &pol, &spec(bad, &body, &hash, d.path()), &CancellationToken::new(), |_, _| {}).await.unwrap_err();
            assert_eq!(err.code(), "url_refused", "{bad}");
        }
    }

    #[tokio::test]
    async fn a_redirect_to_a_host_outside_the_policy_is_refused() {
        let body = payload(1000);
        let hash = sha(&body);
        let srv = Server::start(|_| Reply::Redirect("https://evil.example.org/a.msi".into()));
        let (c, pol) = dev();
        let d = tempfile::tempdir().unwrap();
        let url = srv.url("/a.msi");
        let err = download_verified(&c, &pol, &spec(&url, &body, &hash, d.path()), &CancellationToken::new(), |_, _| {}).await.unwrap_err();
        assert_eq!(err.code(), "url_refused");
    }

    #[tokio::test]
    async fn cancelling_stops_the_download_and_keeps_the_partial_file() {
        let body = payload(400_000);
        let srv = Server::start(move |_| Reply::Hang { body: payload(150_000), declared: 400_000 });
        let (c, pol) = dev();
        let d = tempfile::tempdir().unwrap();
        let hash = sha(&body);
        let url = srv.url("/a.msi");
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        let task = tokio::spawn({
            let (c, pol) = (c.clone(), pol.clone());
            let dir = d.path().to_path_buf();
            let (url, hash, n) = (url.clone(), hash.clone(), body.len() as u64);
            async move {
                let s = DownloadSpec { url: &url, size: n, sha256: &hash, kind: PackageKind::Msi, dest_dir: &dir };
                download_verified(&c, &pol, &s, &c2, |_, _| {}).await
            }
        });
        tokio::time::sleep(Duration::from_millis(400)).await;
        cancel.cancel();
        let err = task.await.unwrap().unwrap_err();
        assert_eq!(err.code(), "cancelled");
        assert!(!d.path().join(format!("{}.msi", &hash[..32])).exists());
    }

    #[tokio::test]
    async fn a_complete_verified_file_is_reused_without_touching_the_network() {
        let body = payload(50_000);
        let b2 = body.clone();
        let srv = Server::start(move |_| Reply::Body(b2.clone()));
        let (c, pol) = dev();
        let d = tempfile::tempdir().unwrap();
        let hash = sha(&body);
        let url = srv.url("/a.msi");
        let s = spec(&url, &body, &hash, d.path());
        download_verified(&c, &pol, &s, &CancellationToken::new(), |_, _| {}).await.unwrap();
        let before = srv.hits.load(std::sync::atomic::Ordering::SeqCst);
        download_verified(&c, &pol, &s, &CancellationToken::new(), |_, _| {}).await.unwrap();
        assert_eq!(srv.hits.load(std::sync::atomic::Ordering::SeqCst), before);
        // A file replaced on disk after verification is detected by the pre-install re-check.
        let fp = final_path(&s);
        std::fs::write(&fp, b"swapped").unwrap();
        assert_eq!(verify_file(&fp, s.size, s.sha256).await.unwrap_err().code(), "size_mismatch");
    }
}



