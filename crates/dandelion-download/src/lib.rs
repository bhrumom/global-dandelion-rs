//! Public resource downloads: bounded streaming, pinned DNS and no script execution.
use dandelion_core::{Error, Result};
use reqwest::{Url, blocking::Response};
use scraper::{Html, Selector};
use serde::Serialize;
use std::{
    collections::{BTreeSet, VecDeque},
    io::{Read, Write},
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    time::Duration,
};
pub const MAX_FILE: u64 = 256 * 1024 * 1024;
pub const MAX_TOTAL: u64 = 512 * 1024 * 1024;
pub const MAX_HTML: usize = 1024 * 1024;
fn fail(message: &str) -> Error {
    Error::Config(format!("link download: {message}"))
}
pub fn parse(value: &str) -> Result<Url> {
    if value.len() > 8192 {
        return Err(fail("URL limit"));
    }
    let mut url = Url::parse(value).map_err(|_| fail("invalid URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err(fail("public HTTP(S) URL required"));
    }
    url.set_fragment(None);
    Ok(url)
}
fn public(address: SocketAddr) -> bool {
    use dandelion_core::targets::{TargetPool, TargetPoolEntry};
    let ip = address.ip();
    let special = match ip {
        IpAddr::V4(ip) => {
            let n = ip.octets();
            (n[0] == 192 && n[1] == 0)
                || (n[0] == 198 && n[1] == 51 && n[2] == 100)
                || (n[0] == 203 && n[1] == 0 && n[2] == 113)
        }
        IpAddr::V6(ip) => {
            let n = ip.segments();
            n[0] & 0xe000 != 0x2000
                || n[0] == 0x2002
                || (n[0] == 0x2001 && (n[1] == 0xdb8 || n[1] < 0x200))
        }
    };
    !special
        && TargetPool::new(&[TargetPoolEntry {
            cidr: format!("{}/{}", ip, if ip.is_ipv4() { 32 } else { 128 }),
            port: address.port(),
            server_name: "download.invalid".into(),
        }])
        .is_ok()
}
fn addresses(url: &Url) -> Result<Vec<SocketAddr>> {
    let host = url
        .host_str()
        .ok_or_else(|| fail("missing host"))?
        .trim_matches(['[', ']']);
    let port = url
        .port_or_known_default()
        .ok_or_else(|| fail("invalid port"))?;
    #[cfg(test)]
    if host == "fixture.invalid" {
        return Ok(vec![SocketAddr::from(([127, 0, 0, 1], port))]);
    }
    let values: Vec<_> = (host, port)
        .to_socket_addrs()
        .map_err(|_| fail("DNS lookup failed"))?
        .take(17)
        .collect();
    if values.is_empty() || values.len() > 16 || values.iter().any(|value| !public(*value)) {
        return Err(fail("non-public destination blocked"));
    }
    Ok(values)
}
fn request(mut url: Url) -> Result<Response> {
    for hop in 0..=5 {
        let values = addresses(&url)?;
        let host = url.host_str().ok_or_else(|| fail("missing host"))?;
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(host, &values)
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .user_agent("Global-Dandelion/0.4 link-download")
            .build()
            .map_err(|_| fail("HTTP initialization failed"))?;
        let response = client
            .get(url.clone())
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .map_err(|_| fail("request failed or timed out"))?;
        if response.status().is_redirection() {
            if hop == 5 {
                return Err(fail("redirect limit"));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|h| h.to_str().ok())
                .ok_or_else(|| fail("invalid redirect"))?;
            let next = parse(
                url.join(location)
                    .map_err(|_| fail("invalid redirect"))?
                    .as_str(),
            )?;
            if url.scheme() == "https" && next.scheme() != "https" {
                return Err(fail("insecure redirect blocked"));
            }
            url = next;
            continue;
        }
        if !response.status().is_success() {
            return Err(fail("resource unavailable or requires login"));
        }
        if response
            .headers()
            .get(reqwest::header::CONTENT_ENCODING)
            .is_some_and(|h| h != "identity")
        {
            return Err(fail("unsupported content encoding"));
        }
        return Ok(response);
    }
    Err(fail("redirect limit"))
}
fn downloadable(value: &str) -> bool {
    let path = value
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    [
        ".pdf", ".epub", ".zip", ".7z", ".txt", ".docx", ".xlsx", ".pptx", ".mp3", ".m4a", ".wav",
        ".mp4", ".webm", ".jpg", ".jpeg", ".png", ".webp", ".gif", ".svg",
    ]
    .iter()
    .any(|ext| path.ends_with(ext))
}
pub fn resources(html: &str, base: &Url) -> Vec<Url> {
    let document = Html::parse_document(html);
    let Ok(selector) = Selector::parse(
        "img[src],video[src],audio[src],source[src],a[href],meta[property],link[rel='enclosure']",
    ) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for element in document.select(&selector).take(4096) {
        let node = element.value();
        let raw = match node.name() {
            "a" => node
                .attr("href")
                .filter(|v| node.attr("download").is_some() || downloadable(v)),
            "meta" => node
                .attr("property")
                .filter(|p| {
                    matches!(
                        *p,
                        "og:image" | "og:video" | "og:audio" | "og:video:url" | "og:audio:url"
                    )
                })
                .and_then(|_| node.attr("content")),
            "link" => node.attr("href"),
            _ => node.attr("src"),
        };
        if let Some(url) = raw
            .and_then(|v| base.join(v).ok())
            .and_then(|v| parse(v.as_str()).ok())
        {
            if seen.insert(url.to_string()) {
                result.push(url);
                if result.len() == 31 {
                    break;
                }
            }
        }
    }
    result
}
#[derive(Serialize)]
pub struct Entry {
    pub name: String,
    pub file: String,
    pub size: u64,
}
#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub entries: Vec<Entry>,
    pub warnings: Vec<String>,
}
/// The caller owns private storage, cancellation (process lifetime) and progress.
pub trait Sink {
    fn create(&mut self, name: &str) -> Result<Box<dyn Write>>;
    fn reserve(&self, bytes: u64) -> Result<()>;
    fn progress(&mut self, bytes: u64, files: usize) -> Result<()>;
}
pub fn download(urls: &[String], sink: &mut impl Sink) -> Result<Report> {
    if urls.is_empty() || urls.len() > 8 {
        return Err(fail("choose 1..8 links"));
    }
    let mut pending = VecDeque::new();
    let mut seen = BTreeSet::new();
    for raw in urls {
        let url = parse(raw)?;
        if seen.insert(url.to_string()) {
            pending.push_back((url, true));
        }
    }
    let mut report = Report {
        schema_version: 1,
        entries: Vec::new(),
        warnings: Vec::new(),
    };
    let mut total = 0;
    let mut attempted = 0;
    let started = std::time::Instant::now();
    while let Some((url, expand)) = pending.pop_front() {
        if started.elapsed() > Duration::from_secs(120) {
            return Err(fail("job deadline exceeded"));
        }
        attempted += 1;
        if report.entries.len() >= 32 {
            report
                .warnings
                .push("Resource limit reached (32 files)".into());
            break;
        }
        let outcome = (|| -> Result<Entry> {
            let mut response = request(url.clone())?;
            let is_playlist = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.contains("mpegurl") || v.contains("dash+xml"))
                || response.url().path().ends_with(".m3u8")
                || response.url().path().ends_with(".mpd");
            if is_playlist {
                return Err(fail("streaming playlist requires a direct media file"));
            }
            let is_html = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| {
                    v.starts_with("text/html") || v.starts_with("application/xhtml+xml")
                });
            let limit = if is_html { MAX_HTML as u64 } else { MAX_FILE }.min(MAX_TOTAL - total);
            if response.content_length().is_some_and(|n| n > limit) {
                return Err(fail("resource exceeds download limit"));
            }
            let mut name = response
                .url()
                .path_segments()
                .and_then(|mut p| p.next_back())
                .filter(|s| !s.is_empty())
                .unwrap_or(if is_html { "page.html" } else { "resource.bin" })
                .chars()
                .filter(|c| !c.is_control() && !matches!(c, '/' | '\\'))
                .take(160)
                .collect::<String>();
            if name.is_empty() {
                name = "resource.bin".into();
            }
            let file = format!(
                "resource-{:03}{}",
                attempted,
                if is_html { ".html" } else { ".bin" }
            );
            sink.reserve(response.content_length().unwrap_or(limit) + 65536)?;
            let mut writer = sink.create(&file)?;
            let mut size = 0;
            let mut html = Vec::new();
            let mut buffer = [0_u8; 65536];
            loop {
                let count = response
                    .read(&mut buffer)
                    .map_err(|_| fail("download interrupted"))?;
                if count == 0 {
                    break;
                }
                if size + count as u64 > limit {
                    return Err(fail("resource exceeds download limit"));
                }
                sink.reserve(count as u64 + 65536)?;
                writer.write_all(&buffer[..count])?;
                if is_html {
                    html.extend_from_slice(&buffer[..count]);
                }
                size += count as u64;
                total += count as u64;
                sink.progress(total, report.entries.len())?;
            }
            writer.flush()?;
            if is_html && expand {
                let text =
                    std::str::from_utf8(&html).map_err(|_| fail("page requires UTF-8 encoding"))?;
                for child in resources(text, response.url()) {
                    if seen.len() < 40 && seen.insert(child.to_string()) {
                        pending.push_back((child, false));
                    }
                }
            }
            Ok(Entry { name, file, size })
        })();
        match outcome {
            Ok(entry) => report.entries.push(entry),
            Err(error) => {
                if report.warnings.len() < 32 {
                    report.warnings.push(error.to_string());
                }
            }
        }
    }
    if report.entries.is_empty() {
        return Err(fail(
            "no resources downloaded; check access and size limits",
        ));
    }
    sink.progress(total, report.entries.len())?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Files {
        root: tempfile::TempDir,
    }
    impl Sink for Files {
        fn create(&mut self, name: &str) -> Result<Box<dyn Write>> {
            Ok(Box::new(
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(self.root.path().join(name))?,
            ))
        }
        fn reserve(&self, _: u64) -> Result<()> {
            Ok(())
        }
        fn progress(&mut self, _: u64, _: usize) -> Result<()> {
            Ok(())
        }
    }
    fn server(responses: Vec<Vec<u8>>) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let task = std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut buffer = [0; 4096];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(&response);
            }
        });
        (
            format!("http://fixture.invalid:{}/page", address.port()),
            task,
        )
    }
    fn response(kind: &str, bytes: &[u8]) -> Vec<u8> {
        [format!("HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).as_bytes(), bytes].concat()
    }
    #[test]
    fn extracts_only_bounded_downloadable_resources() {
        let base = parse("https://example.invalid/path/index.html").unwrap();
        let html = r#"<img src='../x.png?a=1&amp;b=2'><img src='../x.png?a=1&amp;b=2'><script src='bad.js'></script><a href='/next'>next</a><a download href='/file'>file</a><video src='/movie.mp4'></video><a href='file:///etc/passwd'>bad</a>"#;
        let urls = resources(html, &base);
        assert_eq!(urls.len(), 3);
        assert_eq!(urls[0].as_str(), "https://example.invalid/x.png?a=1&b=2");
        assert!(resources(&"<img src='/a.png'>".repeat(10000), &base).len() <= 31);
    }
    #[test]
    fn streams_actual_file_and_page_attachment_bytes() {
        let html = b"<a download href='/document'>download</a><script>fetch('/never')</script>";
        let (url, task) = server(vec![
            response("text/html", html),
            response("application/pdf", b"actual PDF bytes"),
        ]);
        let mut files = Files {
            root: tempfile::tempdir().unwrap(),
        };
        let report = download(&[url], &mut files).unwrap();
        task.join().unwrap();
        assert_eq!(report.entries.len(), 2);
        assert_eq!(
            std::fs::read(files.root.path().join(&report.entries[1].file)).unwrap(),
            b"actual PDF bytes"
        );
        assert!(report.warnings.is_empty());
    }
    #[test]
    fn blocks_private_urls_and_private_redirects() {
        for value in [
            "file:///etc/passwd",
            "https://name:secret@example.invalid/a",
            "javascript:evil()",
        ] {
            assert!(parse(value).is_err());
        }
        for value in [
            "127.0.0.1:80",
            "10.0.0.1:80",
            "169.254.169.254:80",
            "[::1]:80",
            "[::ffff:127.0.0.1]:80",
            "[2002:7f00::1]:80",
        ] {
            assert!(!public(value.parse().unwrap()));
        }
        let (url, task) = server(vec![b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()]);
        let mut files = Files {
            root: tempfile::tempdir().unwrap(),
        };
        assert!(download(&[url], &mut files).is_err());
        task.join().unwrap();
        assert_eq!(std::fs::read_dir(files.root.path()).unwrap().count(), 0);
    }
    #[test]
    fn rejects_oversized_headers_and_streamed_html() {
        for bytes in [
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_FILE + 1
            )
            .into_bytes(),
            [
                b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n"
                    .as_slice(),
                &vec![b'x'; MAX_HTML + 1],
            ]
            .concat(),
        ] {
            let (url, task) = server(vec![bytes]);
            let mut files = Files {
                root: tempfile::tempdir().unwrap(),
            };
            assert!(download(&[url], &mut files).is_err());
            task.join().unwrap();
        }
    }
}
