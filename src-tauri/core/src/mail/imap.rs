use std::sync::Arc;

use async_imap::types::{Fetch, Name, NameAttribute};
use async_imap::{Client, Session};
use async_trait::async_trait;
use chrono::NaiveDate;
use futures::TryStreamExt;
use mailparse::MailHeaderMap;
use rustls::pki_types::ServerName;
use tokio::net::TcpStream;
use tokio_rustls::{client::TlsStream, TlsConnector};

use super::account::MailAccount;

const UID_CHUNK_SIZE: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailHeader {
    pub uid: u32,
    pub message_id: String,
    pub subject: String,
    pub sender: String,
    pub received_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("Gmail authentication failed")]
    Auth(String),
    #[error("Gmail connection failed: {0}")]
    Connection(String),
    #[error("Gmail IMAP protocol error: {0}")]
    Protocol(String),
}

#[async_trait]
pub trait MailSource: Send + Sync {
    async fn check(&self) -> Result<(), MailError>;
    async fn recent_headers(&self, since: NaiveDate) -> Result<Vec<MailHeader>, MailError>;
    async fn body_text(&self, uid: u32) -> Result<String, MailError>;
}

pub struct ImapSource {
    account: MailAccount,
    folder: String,
}

impl ImapSource {
    pub fn new(account: MailAccount, folder: String) -> Self {
        Self { account, folder }
    }

    /// Mailboxes the account can open, in server order. `\Noselect` folders are skipped.
    pub async fn list_mailboxes(&self) -> Result<Vec<String>, MailError> {
        let mut session = self.open_session().await?;
        let names: Vec<Name> = session
            .list(Some(""), Some("*"))
            .await
            .map_err(|error| MailError::Protocol(error.to_string()))?
            .try_collect()
            .await
            .map_err(|error| MailError::Protocol(error.to_string()))?;
        let mailboxes = names
            .iter()
            .filter(|name| !name.attributes().iter().any(|a| matches!(a, NameAttribute::NoSelect)))
            .map(|name| name.name().to_string())
            .collect();
        session
            .logout()
            .await
            .map_err(|error| MailError::Protocol(error.to_string()))?;
        Ok(mailboxes)
    }

    async fn open_session(&self) -> Result<Session<TlsStream<TcpStream>>, MailError> {
        let tcp = TcpStream::connect((self.account.host.as_str(), self.account.port))
            .await
            .map_err(|error| MailError::Connection(error.to_string()))?;

        let config = build_tls_config();
        let connector = TlsConnector::from(Arc::new(config));
        let server_name = ServerName::try_from(self.account.host.clone())
            .map_err(|error| MailError::Connection(error.to_string()))?;
        let tls = connector
            .connect(server_name, tcp)
            .await
            .map_err(|error| MailError::Connection(error.to_string()))?;

        let client = Client::new(tls);
        match client
            .login(&self.account.address, &self.account.password)
            .await
        {
            Ok(session) => Ok(session),
            Err((_error, _client)) => Err(MailError::Auth("Gmail authentication failed".into())),
        }
    }

    async fn examine_folder(
        &self,
        session: &mut Session<TlsStream<TcpStream>>,
    ) -> Result<(), MailError> {
        session
            .examine(self.folder.as_str())
            .await
            .map(|_| ())
            .map_err(|error| MailError::Protocol(error.to_string()))
    }
}

pub(super) fn build_tls_config() -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("AWS-LC supports rustls default protocol versions")
    .with_root_certificates(roots)
    .with_no_client_auth()
}

#[async_trait]
impl MailSource for ImapSource {
    async fn check(&self) -> Result<(), MailError> {
        let mut session = self.open_session().await?;
        self.examine_folder(&mut session).await?;
        session
            .logout()
            .await
            .map_err(|error| MailError::Protocol(error.to_string()))
    }

    async fn recent_headers(&self, since: NaiveDate) -> Result<Vec<MailHeader>, MailError> {
        let mut session = self.open_session().await?;
        self.examine_folder(&mut session).await?;
        let date = since.format("%-d-%b-%Y").to_string();
        let mut uids: Vec<u32> = session
            .uid_search(format!("SINCE {date}"))
            .await
            .map_err(|error| MailError::Protocol(error.to_string()))?
            .into_iter()
            .collect();
        uids.sort_unstable();

        let mut headers = Vec::with_capacity(uids.len());
        for chunk in chunk_uids(&uids) {
            let uid_set = chunk
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let fetches = session
                .uid_fetch(
                    uid_set,
                    "BODY.PEEK[HEADER.FIELDS (MESSAGE-ID SUBJECT FROM DATE)]",
                )
                .await
                .map_err(|error| MailError::Protocol(error.to_string()))?;
            let fetches: Vec<Fetch> = fetches
                .try_collect()
                .await
                .map_err(|error| MailError::Protocol(error.to_string()))?;
            headers.extend(fetches.into_iter().filter_map(|fetch| {
                fetch
                    .header()
                    .map(|raw| parse_header(raw, fetch.uid.unwrap_or_default()))
            }));
        }

        session
            .logout()
            .await
            .map_err(|error| MailError::Protocol(error.to_string()))?;
        Ok(headers)
    }

    async fn body_text(&self, uid: u32) -> Result<String, MailError> {
        let mut session = self.open_session().await?;
        self.examine_folder(&mut session).await?;
        let fetches = session
            .uid_fetch(uid.to_string(), "BODY.PEEK[]")
            .await
            .map_err(|error| MailError::Protocol(error.to_string()))?;
        let fetches: Vec<Fetch> = fetches
            .try_collect()
            .await
            .map_err(|error| MailError::Protocol(error.to_string()))?;
        let raw = fetches
            .into_iter()
            .find_map(|fetch| fetch.body().map(<[u8]>::to_vec))
            .ok_or_else(|| MailError::Protocol(format!("message UID {uid} was not found")))?;
        let text = extract_text(&raw);
        session
            .logout()
            .await
            .map_err(|error| MailError::Protocol(error.to_string()))?;
        Ok(text)
    }
}

pub fn parse_header(raw: &[u8], uid: u32) -> MailHeader {
    let parsed = mailparse::parse_mail(raw).ok();
    let value = |name: &str| {
        parsed
            .as_ref()
            .and_then(|mail| mail.headers.get_first_value(name))
            .unwrap_or_default()
    };
    MailHeader {
        uid,
        message_id: value("Message-ID"),
        subject: value("Subject"),
        sender: value("From"),
        received_at: value("Date"),
    }
}

pub fn extract_text(raw: &[u8]) -> String {
    let Ok(parsed) = mailparse::parse_mail(raw) else {
        return String::from_utf8_lossy(raw).into_owned();
    };
    if let Some(plain) = find_mime_body(&parsed, "text/plain") {
        return plain;
    }
    if let Some(html) = find_mime_body(&parsed, "text/html") {
        return html2text::from_read(html.as_bytes(), 100).unwrap_or_default();
    }
    String::new()
}

fn find_mime_body(mail: &mailparse::ParsedMail<'_>, wanted: &str) -> Option<String> {
    if mail.ctype.mimetype.eq_ignore_ascii_case(wanted) {
        return mail.get_body().ok();
    }
    mail.subparts
        .iter()
        .find_map(|part| find_mime_body(part, wanted))
}

pub fn chunk_uids(uids: &[u32]) -> Vec<Vec<u32>> {
    uids.chunks(UID_CHUNK_SIZE).map(<[u32]>::to_vec).collect()
}

pub fn gmail_link(message_id: &str) -> String {
    let id = message_id
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>');
    let encoded = id
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect::<String>();
    format!("https://mail.google.com/mail/u/0/#search/rfc822msgid%3A{encoded}")
}
