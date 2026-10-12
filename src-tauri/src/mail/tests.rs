use crate::store::Store;

use super::account::{self, MailAccount};

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
