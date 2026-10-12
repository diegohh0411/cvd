use crate::store::Store;

use super::account::{self, MailAccount};
use super::imap::{chunk_uids, extract_text, gmail_link, parse_header, ImapSource, MailSource};

async fn store() -> Store {
    Store::open("sqlite::memory:").await.unwrap()
}

fn account() -> MailAccount {
    MailAccount {
        host: "imap.gmail.com".into(),
        port: 993,
        address: "user@gmail.com".into(),
        password: "abcdefghijklmnop".into(),
    }
}

#[tokio::test]
async fn save_then_load_round_trips() {
    let store = store().await;
    let expected = account();
    account::save(&store, &expected).await.unwrap();
    assert!(account::load(&store).await.unwrap() == Some(expected));
}

#[tokio::test]
async fn password_spaces_are_stripped_on_save() {
    let store = store().await;
    let mut account = account();
    account.password = "abcd efgh ijkl mnop".into();
    account::save(&store, &account).await.unwrap();
    assert_eq!(
        account::load(&store).await.unwrap().unwrap().password,
        "abcdefghijklmnop"
    );
}

#[tokio::test]
async fn load_is_none_when_incomplete() {
    let store = store().await;
    store
        .set_setting("mail.address", "user@gmail.com")
        .await
        .unwrap();
    assert!(account::load(&store).await.unwrap().is_none());
}

#[tokio::test]
async fn remove_clears_everything() {
    let store = store().await;
    account::save(&store, &account()).await.unwrap();
    account::remove(&store).await.unwrap();
    assert!(account::load(&store).await.unwrap().is_none());
    assert_eq!(store.get_setting("mail.host").await.unwrap(), None);
    assert_eq!(store.get_setting("mail.port").await.unwrap(), None);
    assert_eq!(store.get_setting("mail.address").await.unwrap(), None);
    assert_eq!(store.get_secret("gmail").await.unwrap(), None);
}

#[test]
fn validate_rejects_bad_fields() {
    assert!(account::validate("", 993, "user@gmail.com").is_err());
    assert!(account::validate("imap.gmail.com", 0, "user@gmail.com").is_err());
    assert!(account::validate("imap.gmail.com", 993, "invalid-address").is_err());
}

#[test]
fn html_only_email_yields_readable_text() {
    let raw = b"From: Recruiter <recruiter@example.com>\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body><h1>Interview invitation</h1><p>Please choose a time.</p></body></html>";
    let text = extract_text(raw);
    assert!(text.contains("Interview invitation"));
    assert!(text.contains("Please choose a time."));
    assert!(!text.contains("<h1>"));
}

#[test]
fn plain_part_preferred_over_html() {
    let raw = b"From: Recruiter <recruiter@example.com>\r\nContent-Type: multipart/alternative; boundary=alt\r\n\r\n--alt\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nPlain version\r\n--alt\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<b>HTML version</b>\r\n--alt--\r\n";
    let text = extract_text(raw);
    assert!(text.contains("Plain version"));
    assert!(!text.contains("HTML version"));
}

#[test]
fn rfc2047_subject_is_decoded() {
    let raw = b"Message-ID: <message@example.com>\r\nSubject: =?UTF-8?Q?Interview_invitation_=E2=9C=85?=\r\nFrom: Recruiter <recruiter@example.com>\r\nDate: Mon, 12 Oct 2026 10:00:00 +0000\r\n\r\nBody";
    let header = parse_header(raw, 17);
    assert!(header.subject.contains("Interview invitation"));
    assert!(header.subject.contains('✅'));
    assert_eq!(header.uid, 17);
    assert_eq!(header.message_id, "<message@example.com>");
}

#[test]
fn chunk_uids_splits_at_200() {
    let uids: Vec<u32> = (1..=450).collect();
    let chunks = chunk_uids(&uids);
    assert_eq!(
        chunks.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![200, 200, 50]
    );
}

#[test]
fn gmail_link_strips_brackets_and_encodes() {
    assert_eq!(
        gmail_link("<message+tag@example.com>"),
        "https://mail.google.com/mail/u/0/#search/rfc822msgid%3Amessage%2Btag%40example.com"
    );
}

#[test]
fn tls_config_builds_with_workspace_rustls_features() {
    super::imap::build_tls_config();
}

#[tokio::test]
#[ignore = "requires CVD_TEST_GMAIL_ADDRESS and CVD_TEST_GMAIL_APP_PASSWORD"]
async fn live_gmail_headers() {
    let account = MailAccount {
        host: "imap.gmail.com".into(),
        port: 993,
        address: std::env::var("CVD_TEST_GMAIL_ADDRESS").expect("test Gmail address is required"),
        password: std::env::var("CVD_TEST_GMAIL_APP_PASSWORD")
            .expect("test Gmail app password is required"),
    };
    let source = ImapSource::new(account);
    source.check().await.unwrap();
    let since = chrono::Utc::now().date_naive() - chrono::Duration::days(30);
    let headers = source.recent_headers(since).await.unwrap();
    assert!(!headers.is_empty());
}
