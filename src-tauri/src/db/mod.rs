//! Camada de acesso a banco. `driver` define o contrato; cada dialeto
//! implementa atrás dele — ver CLAUDE.md §3.

pub mod driver;
pub mod error;
pub mod mysql;
pub mod oracle;
pub mod postgres;
pub mod value;
