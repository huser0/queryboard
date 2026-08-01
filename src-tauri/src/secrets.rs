//! Keyring do SO. Único lugar do repositório que toca senha de connection
//! — CLAUDE.md §7: "Senhas nunca em texto plano, nunca versionadas...
//! senha nunca volta pelo IPC, nem em log, nem em mensagem de erro."
//!
//! A chave no keyring é `(service = "queryboard", account = connection.id)`
//! — nunca o slug (que é exportável) nem o nome (que o usuário edita).

use keyring::Entry;
use thiserror::Error;

use crate::db::driver::SecretRef;

const SERVICE: &str = "queryboard";

#[derive(Debug, Error)]
pub enum SecretsError {
    #[error("falha ao acessar o keyring do sistema")]
    Backend,
    #[error("nenhuma senha cadastrada para esta connection")]
    NotFound,
}

impl From<keyring::Error> for SecretsError {
    fn from(err: keyring::Error) -> Self {
        match err {
            keyring::Error::NoEntry => SecretsError::NotFound,
            // Deliberado: o `keyring::Error` original pode conter detalhes
            // de backend (ex.: caminho de socket D-Bus) que não têm nada a
            // ver com SQL, mas por princípio nenhum detalhe interno de
            // infraestrutura atravessa esta borda sem necessidade.
            _ => SecretsError::Backend,
        }
    }
}

fn entry(connection_id: &str) -> Result<Entry, SecretsError> {
    Entry::new(SERVICE, connection_id).map_err(SecretsError::from)
}

pub fn store(connection_id: &str, password: &str) -> Result<(), SecretsError> {
    entry(connection_id)?.set_password(password)?;
    Ok(())
}

pub fn resolve(secret: &SecretRef) -> Result<String, SecretsError> {
    entry(secret.connection_id())?
        .get_password()
        .map_err(SecretsError::from)
}

pub fn delete(connection_id: &str) -> Result<(), SecretsError> {
    match entry(connection_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(SecretsError::from(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Precisa de um backend de keyring real (secret-service/keychain/
    // credential manager) rodando — não disponível em todo runner de CI
    // headless. Rodar manualmente com `cargo test secrets:: -- --ignored`.
    #[test]
    #[ignore]
    fn store_resolve_delete_roundtrip() {
        let id = format!("queryboard-test-{}", uuid::Uuid::new_v4());
        store(&id, "s3cr3t-password").unwrap();

        let secret = SecretRef::new(id.clone());
        assert_eq!(resolve(&secret).unwrap(), "s3cr3t-password");

        delete(&id).unwrap();
        assert!(matches!(resolve(&secret), Err(SecretsError::NotFound)));
    }

    #[test]
    #[ignore]
    fn delete_of_nonexistent_entry_is_not_an_error() {
        let id = format!("queryboard-test-never-existed-{}", uuid::Uuid::new_v4());
        assert!(delete(&id).is_ok());
    }
}
