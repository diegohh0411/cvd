//! Supabase Auth: verifies access tokens against the project's JWKS.
use std::time::{Duration, Instant};

use cvd_core::store::UserId;
use jsonwebtoken::{jwk::JwkSet, Algorithm, DecodingKey, Validation};
use tokio::sync::RwLock;

/// Supabase signs user sessions for this audience.
const AUDIENCE: &str = "authenticated";
/// Unknown `kid`s refetch the key set (rotation), at most this often.
const REFETCH_AFTER: Duration = Duration::from_secs(60);
/// Even known keys are refetched this often, so a revoked key stops working. Supabase's
/// own edge caches the JWKS for about as long.
const MAX_AGE: Duration = Duration::from_secs(10 * 60);
/// If the refetch fails, cached keys keep working this long before sign-ins fail closed.
const STALE_LIMIT: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum AuthError {
    #[error("Sign in to continue.")]
    Missing,
    #[error("Your session is invalid or expired; sign in again.")]
    Invalid,
    #[error("Couldn't reach the sign-in service: {0}")]
    Keys(String),
}

#[derive(serde::Deserialize)]
struct Claims {
    sub: uuid::Uuid,
    exp: u64,
}

/// A verified token: whose it is and until when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Session {
    pub user: UserId,
    pub expires_at: std::time::SystemTime,
}

enum Source {
    Remote { url: String, http: reqwest::Client },
    Fixed,
}

struct Cache {
    keys: JwkSet,
    fetched: Option<Instant>,
    /// Last fetch attempt, successful or not; bounds retries while Supabase is down.
    attempted: Option<Instant>,
}

pub struct Verifier {
    issuer: String,
    source: Source,
    cache: RwLock<Cache>,
}

impl Verifier {
    /// `project_url` is the Supabase project URL, e.g. `https://abc.supabase.co`.
    pub fn supabase(project_url: &str) -> Self {
        let base = format!("{}/auth/v1", project_url.trim_end_matches('/'));
        Self {
            source: Source::Remote {
                url: format!("{base}/.well-known/jwks.json"),
                http: reqwest::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                    .expect("static client config"),
            },
            issuer: base,
            cache: RwLock::new(Cache {
                keys: JwkSet { keys: vec![] },
                fetched: None,
                attempted: None,
            }),
        }
    }

    /// Never refetches; for tests and fixed-key deployments.
    pub fn with_keys(issuer: &str, keys: JwkSet) -> Self {
        Self {
            issuer: issuer.to_owned(),
            source: Source::Fixed,
            cache: RwLock::new(Cache {
                keys,
                fetched: Some(Instant::now()),
                attempted: None,
            }),
        }
    }

    /// Checks signature, `exp`, `aud` and `iss`; the subject is the tenant.
    pub async fn verify(&self, token: &str) -> Result<Session, AuthError> {
        let header = jsonwebtoken::decode_header(token).map_err(|_| AuthError::Invalid)?;
        let kid = header.kid.ok_or(AuthError::Invalid)?;
        self.expire_old_keys().await?;
        let (key, alg) = match self.key(&kid).await? {
            Some(k) => k,
            None => {
                self.refresh().await?;
                self.key(&kid).await?.ok_or(AuthError::Invalid)?
            }
        };
        // The key pins the algorithm so a token can't pick a weaker one.
        if header.alg != alg {
            return Err(AuthError::Invalid);
        }
        let mut validation = Validation::new(alg);
        validation.set_audience(&[AUDIENCE]);
        validation.set_issuer(&[&self.issuer]);
        validation.set_required_spec_claims(&["exp", "aud", "iss", "sub"]);
        let data = jsonwebtoken::decode::<Claims>(token, &key, &validation)
            .map_err(|_| AuthError::Invalid)?;
        Ok(Session {
            user: UserId(data.claims.sub),
            expires_at: std::time::UNIX_EPOCH + Duration::from_secs(data.claims.exp),
        })
    }

    /// Refetches keys older than [`MAX_AGE`]; on failure, keeps them up to [`STALE_LIMIT`].
    async fn expire_old_keys(&self) -> Result<(), AuthError> {
        let age = |c: &Cache| c.fetched.map_or(Duration::MAX, |t| t.elapsed());
        if matches!(self.source, Source::Fixed) || age(&*self.cache.read().await) < MAX_AGE {
            return Ok(());
        }
        match self.refresh().await {
            Ok(()) => Ok(()),
            Err(e) if age(&*self.cache.read().await) < STALE_LIMIT => {
                tracing::warn!(error = %e, "couldn't refresh signing keys; using cached ones");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    async fn key(&self, kid: &str) -> Result<Option<(DecodingKey, Algorithm)>, AuthError> {
        let cache = self.cache.read().await;
        let Some(jwk) = cache.keys.find(kid) else {
            return Ok(None);
        };
        let alg = jwk
            .common
            .key_algorithm
            .and_then(|a| Algorithm::try_from(a).ok())
            .ok_or(AuthError::Invalid)?;
        let key = DecodingKey::from_jwk(jwk).map_err(|_| AuthError::Invalid)?;
        Ok(Some((key, alg)))
    }

    async fn refresh(&self) -> Result<(), AuthError> {
        let Source::Remote { url, http } = &self.source else {
            return Ok(());
        };
        let mut cache = self.cache.write().await;
        // Another request may have refetched (or failed to) while we waited for the lock.
        if cache.attempted.is_some_and(|t| t.elapsed() < REFETCH_AFTER) {
            return Ok(());
        }
        cache.attempted = Some(Instant::now());
        let keys = async {
            http.get(url)
                .send()
                .await?
                .error_for_status()?
                .json::<JwkSet>()
                .await
        }
        .await
        .map_err(|e| AuthError::Keys(e.to_string()))?;
        cache.keys = keys;
        cache.fetched = cache.attempted;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;

    const ISSUER: &str = "https://test.supabase.co/auth/v1";
    const SECRET: &[u8] = b"test-secret-that-is-long-enough!";

    fn verifier() -> Verifier {
        let keys = serde_json::from_value(json!({ "keys": [{
            "kty": "oct", "kid": "k1", "alg": "HS256",
            "k": base64url(SECRET),
        }]}))
        .unwrap();
        Verifier::with_keys(ISSUER, keys)
    }

    fn base64url(bytes: &[u8]) -> String {
        use base64::Engine;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    }

    fn token(claims: serde_json::Value, kid: &str) -> String {
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some(kid.into());
        encode(&header, &claims, &EncodingKey::from_secret(SECRET)).unwrap()
    }

    fn claims(user: uuid::Uuid) -> serde_json::Value {
        let exp = chrono::Utc::now().timestamp() + 600;
        json!({ "sub": user, "aud": AUDIENCE, "iss": ISSUER, "exp": exp })
    }

    #[tokio::test]
    async fn accepts_a_valid_token_and_maps_sub() {
        let user = uuid::Uuid::new_v4();
        let got = verifier().verify(&token(claims(user), "k1")).await.unwrap();
        assert_eq!(got.user, UserId(user));
        assert!(got.expires_at > std::time::SystemTime::now());
    }

    #[tokio::test]
    async fn rejects_bad_audience_issuer_expiry_and_keys() {
        let user = uuid::Uuid::new_v4();
        let v = verifier();
        let mut c = claims(user);
        c["aud"] = json!("anon");
        assert_eq!(v.verify(&token(c, "k1")).await, Err(AuthError::Invalid));
        let mut c = claims(user);
        c["iss"] = json!("https://other.supabase.co/auth/v1");
        assert_eq!(v.verify(&token(c, "k1")).await, Err(AuthError::Invalid));
        let mut c = claims(user);
        c["exp"] = json!(chrono::Utc::now().timestamp() - 3600);
        assert_eq!(v.verify(&token(c, "k1")).await, Err(AuthError::Invalid));
        assert_eq!(
            v.verify(&token(claims(user), "unknown")).await,
            Err(AuthError::Invalid)
        );
        assert_eq!(v.verify("not-a-jwt").await, Err(AuthError::Invalid));
    }
}
