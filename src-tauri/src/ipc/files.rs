//! Leitura de arquivo local .sql escolhido pelo usuário via diálogo
//! nativo (feature "abrir SQL existente"). Comando sem estado — não
//! toca `AppState` nem banco; só valida tamanho e lê texto UTF-8.

use std::fs;

/// 5 MiB — rede de segurança contra escolher sem querer um arquivo
/// enorme/binário, independente do teto de 256 KiB de
/// `sql::lexical::MAX_SQL_BYTES` (que valida SQL pronta pra *rodar*,
/// não o que pode ser *aberto* pra editar antes de rodar).
pub const MAX_SQL_FILE_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum SqlFileError {
    #[error("não foi possível ler o arquivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("arquivo excede o limite de {max} bytes (tamanho: {actual})")]
    TooLarge { max: u64, actual: u64 },
}

fn read_sql_file(path: &str) -> Result<String, SqlFileError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_SQL_FILE_BYTES {
        return Err(SqlFileError::TooLarge {
            max: MAX_SQL_FILE_BYTES,
            actual: metadata.len(),
        });
    }
    // read_to_string já rejeita conteúdo não-UTF-8 (io::ErrorKind::InvalidData).
    Ok(fs::read_to_string(path)?)
}

#[tauri::command]
pub fn sql_file_read(path: String) -> Result<String, String> {
    read_sql_file(&path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_existing_small_file() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "SELECT 1").unwrap();
        let sql = read_sql_file(f.path().to_str().unwrap()).unwrap();
        assert_eq!(sql, "SELECT 1");
    }

    #[test]
    fn errors_on_missing_file() {
        let err = read_sql_file("/definitely/not/here.sql").unwrap_err();
        assert!(matches!(err, SqlFileError::Io(_)));
    }

    #[test]
    fn errors_on_oversized_file() {
        let f = tempfile::NamedTempFile::new().unwrap();
        f.as_file().set_len(MAX_SQL_FILE_BYTES + 1).unwrap();
        let err = read_sql_file(f.path().to_str().unwrap()).unwrap_err();
        assert!(matches!(err, SqlFileError::TooLarge { .. }));
    }
}
