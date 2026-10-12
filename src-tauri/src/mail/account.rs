use crate::store::{Store, StoreError};

const HOST_KEY: &str = "mail.host";
const PORT_KEY: &str = "mail.port";
const ADDRESS_KEY: &str = "mail.address";
const FOLDER_KEY: &str = "mail.folder";
const SECRET_PROVIDER: &str = "gmail";
/// Gmail's All Mail, the one place each message appears once. Name is English-only.
pub const DEFAULT_FOLDER: &str = "[Gmail]/All Mail";

#[derive(Clone, PartialEq, Eq)]
pub struct MailAccount {
    pub host: String,
    pub port: u16,
    pub address: String,
    pub password: String,
}

pub async fn load(store: &Store) -> Result<Option<MailAccount>, StoreError> {
    let Some(host) = store.get_setting(HOST_KEY).await? else {
        return Ok(None);
    };
    let Some(port) = store.get_setting(PORT_KEY).await? else {
        return Ok(None);
    };
    let Some(address) = store.get_setting(ADDRESS_KEY).await? else {
        return Ok(None);
    };
    let Some(password) = store.get_secret(SECRET_PROVIDER).await? else {
        return Ok(None);
    };
    let port = port
        .parse::<u16>()
        .map_err(|_| StoreError::Invalid(format!("mail.port value {port}")))?;

    Ok(Some(MailAccount {
        host,
        port,
        address,
        password,
    }))
}

pub async fn save(store: &Store, account: &MailAccount) -> Result<(), StoreError> {
    validate(&account.host, account.port, &account.address).map_err(StoreError::Invalid)?;
    store.set_setting(HOST_KEY, &account.host).await?;
    store
        .set_setting(PORT_KEY, &account.port.to_string())
        .await?;
    store.set_setting(ADDRESS_KEY, &account.address).await?;
    store
        .set_secret(SECRET_PROVIDER, &normalize_password(&account.password))
        .await?;
    Ok(())
}

/// The mailbox the scan reads; `DEFAULT_FOLDER` until the user picks another.
pub async fn folder(store: &Store) -> Result<String, StoreError> {
    Ok(store
        .get_setting(FOLDER_KEY)
        .await?
        .filter(|folder| !folder.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_FOLDER.to_string()))
}

pub async fn set_folder(store: &Store, folder: &str) -> Result<(), StoreError> {
    let folder = folder.trim();
    if folder.is_empty() {
        return Err(StoreError::Invalid("mailbox must not be empty".into()));
    }
    store.set_setting(FOLDER_KEY, folder).await
}

pub async fn remove(store: &Store) -> Result<(), StoreError> {
    store.delete_setting(HOST_KEY).await?;
    store.delete_setting(PORT_KEY).await?;
    store.delete_setting(ADDRESS_KEY).await?;
    store.delete_setting(FOLDER_KEY).await?;
    store.delete_secret(SECRET_PROVIDER).await?;
    Ok(())
}

pub fn validate(host: &str, port: u16, address: &str) -> Result<(), String> {
    if host.is_empty() {
        return Err("host must not be empty".into());
    }
    if port == 0 {
        return Err("port must not be zero".into());
    }
    if !address.contains('@') {
        return Err("address must contain @".into());
    }
    Ok(())
}

pub fn normalize_password(password: &str) -> String {
    password.chars().filter(|c| !c.is_whitespace()).collect()
}
