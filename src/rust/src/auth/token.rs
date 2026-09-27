type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "io.github.darkplayoff.yayma";
#[cfg(target_os = "macos")]
const KEYCHAIN_ACCOUNT: &str = "yandex-music-oauth";
#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

fn serialize_token_payload(token: &str, user_id: u64) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&(token, user_id))?)
}

fn deserialize_token_payload(payload: &[u8]) -> Result<(String, u64)> {
    Ok(serde_json::from_slice(payload)?)
}

#[cfg(target_os = "macos")]
fn load_keychain_token() -> Result<Option<(String, u64)>> {
    use security_framework::passwords::get_generic_password;

    match get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        Ok(payload) => Ok(Some(deserialize_token_payload(&payload)?)),
        Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
        Err(error) => Err(Box::new(error)),
    }
}

#[cfg(target_os = "macos")]
fn store_keychain_token(token: &str, user_id: u64) -> Result<()> {
    use security_framework::passwords::set_generic_password;

    let payload = serialize_token_payload(token, user_id)?;
    set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, &payload)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn delete_keychain_token() -> Result<()> {
    use security_framework::passwords::delete_generic_password;

    match delete_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
        Err(error) => Err(Box::new(error)),
    }
}

pub struct TokenProvider;

impl TokenProvider {
    pub async fn resolve() -> Option<(String, u64)> {
        #[cfg(target_os = "macos")]
        {
            match load_keychain_token() {
                Ok(Some(saved)) => return Some(saved),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!("Failed to read auth token from macOS Keychain: {:?}", error);
                }
            }

            // One-time migration from the legacy SQLite setting. Keep the legacy
            // token if Keychain is unavailable, so existing users are not logged out.
            let database = crate::app::get_database().await.ok()?;
            let mut db = database.lock().await;
            let legacy = db.load_auth_token().await.ok().flatten();

            if let Some((token, user_id)) = legacy.as_ref() {
                match store_keychain_token(token, *user_id) {
                    Ok(()) => {
                        if let Err(error) = db.delete_auth_token().await {
                            tracing::warn!(
                                "OAuth token migrated to Keychain but legacy SQLite token could not be removed: {:?}",
                                error
                            );
                        } else {
                            tracing::info!("Migrated OAuth token from SQLite to macOS Keychain");
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            "Failed to migrate OAuth token to macOS Keychain; keeping legacy SQLite token: {:?}",
                            error
                        );
                    }
                }
            }

            return legacy;
        }

        #[cfg(not(target_os = "macos"))]
        {
            let database = crate::app::get_database().await.ok()?;
            let mut db = database.lock().await;
            db.load_auth_token().await.ok().flatten()
        }
    }

    pub async fn store(token: &str, user_id: u64) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            store_keychain_token(token, user_id)?;

            // Remove a pre-Keychain token if one is still present after an upgrade.
            let database = crate::app::get_database().await?;
            let mut db = database.lock().await;
            db.delete_auth_token().await?;
            return Ok(());
        }

        #[cfg(not(target_os = "macos"))]
        {
            let database = crate::app::get_database().await?;
            let mut db = database.lock().await;
            db.save_auth_token(token, user_id).await?;
            Ok(())
        }
    }

    pub async fn delete() -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            // Always attempt both stores. This prevents an old plaintext token from
            // surviving logout if a previous migration was only partially completed.
            let keychain_result = delete_keychain_token();

            let database = crate::app::get_database().await?;
            let mut db = database.lock().await;
            db.delete_auth_token().await?;

            return keychain_result;
        }

        #[cfg(not(target_os = "macos"))]
        {
            let database = crate::app::get_database().await?;
            let mut db = database.lock().await;
            db.delete_auth_token().await?;
            Ok(())
        }
    }

    pub async fn validate(token: String) -> Result<u64> {
        let client = crate::util::tls::yandex_music_client(token)?;
        let status = client.get_account_status().await?;

        status.account.uid.ok_or("No user id found".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{deserialize_token_payload, serialize_token_payload};

    #[test]
    fn token_payload_round_trip() {
        let encoded = serialize_token_payload("oauth-secret", 42).expect("serialize");
        let decoded = deserialize_token_payload(&encoded).expect("deserialize");

        assert_eq!(decoded, ("oauth-secret".to_string(), 42));
    }

    #[test]
    fn malformed_token_payload_is_rejected() {
        assert!(deserialize_token_payload(b"not-json").is_err());
    }
}
