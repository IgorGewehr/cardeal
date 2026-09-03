//! Utilitários finos de mapeamento entre os tipos do kernel e o SQLite.

use cardeal_kernel::Id;

/// Um [`Id`] como blob de 16 bytes, para parâmetros de consulta.
pub(crate) fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

/// Um [`Id`] opcional como blob, ou `NULL`.
pub(crate) fn blob_opt(id: Option<Id>) -> Option<Vec<u8>> {
    id.map(blob)
}
