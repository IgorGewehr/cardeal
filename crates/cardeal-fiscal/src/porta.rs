//! A porta fiscal — o único ponto de contato com o mundo externo da SEFAZ
//! (`docs/10-modulo-fiscal.md` §2 e §9).
//!
//! **Decisão desta fase (fiscal ainda não implementado de verdade):** o doc 10 desenha
//! `PortaFiscal` como `#[async_trait]`, porque o adaptador real (`ApiFiscalHttp`) fala HTTP.
//! Mas todo `Comando` do despacho roda **síncrono**, dentro do fecho do `Escritor`
//! (`docs/contratos-internos.md` §4) — e `Ctx::porta()` (a forma de injetar uma porta dentro
//! de um comando) ainda está listado como "adiado" lá. Por isso esta versão da trait é
//! **síncrona**: o adaptador real, quando existir, resolve a ponte com
//! `tokio::runtime::Handle::block_on` internamente — a mesma técnica que bibliotecas
//! síncronas usam para embrulhar clientes HTTP assíncronos, sem vazar `async` para quem
//! chama. Isso é reavaliado quando `Ctx::porta()` for implementado.
//!
//! O que esta versão cobre é só o suficiente para `mod-compras` puxar notas de entrada da
//! distribuição `DFe` — o restante da trait completa do doc 10 (emitir, cancelar, SPED…) fica
//! para quando `mod-fiscal` for implementado de verdade.

use cardeal_kernel::{Cnpj, Resultado};

use crate::chave::ChaveAcesso;

/// O "próximo a buscar" na distribuição `DFe` — um cursor opaco que a SEFAZ define. `0` pede
/// desde o início.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Nsu(pub u64);

/// Um documento fiscal resumido, listado pela distribuição `DFe` — ainda sem o XML completo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoDfe {
    /// A chave de acesso do documento.
    pub chave_acesso: ChaveAcesso,
    /// O CNPJ de quem emitiu.
    pub cnpj_emitente: Cnpj,
    /// O NSU deste resumo — usado para pedir a próxima página a partir daqui.
    pub nsu: Nsu,
}

/// O XML de uma nota, ainda cru — quem interpreta é `crate::nfe::interpretar`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlNota(pub String);

/// A porta fiscal mínima que `mod-compras` precisa nesta fase: descobrir documentos novos
/// endereçados ao CNPJ da empresa e baixar o XML de um deles.
pub trait PortaFiscal: Send + Sync {
    /// Lista os resumos de documentos fiscais emitidos contra `cnpj` desde `desde`
    /// (exclusive). A chamada real pagina pelo NSU; o chamador guarda o maior NSU visto e
    /// o usa como `desde` na próxima varredura.
    ///
    /// # Errors
    /// [`crate::ErroFiscal::ServicoIndisponivel`] (só no adaptador real).
    fn distribuicao_dfe(&self, cnpj: &Cnpj, desde: Nsu) -> Resultado<Vec<ResumoDfe>>;

    /// Baixa o XML completo de um documento pela chave de acesso.
    ///
    /// # Errors
    /// [`crate::ErroFiscal::ServicoIndisponivel`] (só no adaptador real).
    fn baixar_xml(&self, chave: &ChaveAcesso) -> Resultado<XmlNota>;
}
