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
//! O que esta versão cobre: a distribuição `DFe` que `mod-compras` usa e o status do serviço
//! da SEFAZ; o restante da trait do doc 10 (emitir, cancelar, carta de correção…) entra com
//! cada fatia da onda 7. O adaptador real é [`crate::ApiFiscalHttp`], cliente do `sefaz-api`.

use cardeal_kernel::{Cnpj, Resultado, Uf};
use serde::{Deserialize, Serialize};

use crate::chave::ChaveAcesso;
use crate::emissao::{Csc, Nfce, RetornoSefaz};

/// O modelo do documento fiscal eletrônico.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModeloFiscal {
    /// NF-e, modelo 55 (venda entre empresas, devolução, remessa).
    Nfe,
    /// NFC-e, modelo 65 (cupom do consumidor final).
    Nfce,
}

impl ModeloFiscal {
    /// O código do modelo ("55"/"65").
    #[must_use]
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::Nfe => "55",
            Self::Nfce => "65",
        }
    }
}

/// Homologação (testes, sem valor fiscal) ou produção.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AmbienteFiscal {
    /// Homologação — o ambiente de testes da SEFAZ. Padrão até a empresa ligar a produção.
    #[default]
    Homologacao,
    /// Produção — notas com valor fiscal.
    Producao,
}

/// A resposta da SEFAZ à consulta de status do serviço.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusSefaz {
    /// Verdadeiro quando a SEFAZ respondeu "107 — Serviço em Operação".
    pub em_operacao: bool,
    /// O código de status (`cStat`) devolvido.
    pub codigo: String,
    /// O motivo (`xMotivo`) devolvido.
    pub motivo: String,
}

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

    /// Consulta se o serviço de autorização da SEFAZ da `uf` está no ar para o `modelo` — a
    /// primeira chamada que confere, ponta a ponta, certificado, credencial e rede.
    ///
    /// # Errors
    /// [`crate::ErroFiscal::ServicoIndisponivel`] se a API fiscal não responde; os erros de
    /// credencial/certificado que ela devolver.
    fn status_servico(&self, uf: Uf, modelo: ModeloFiscal) -> Resultado<StatusSefaz>;

    /// Emite uma NFC-e (síncrona na SEFAZ). Rejeição e denegação voltam em `Ok`, no
    /// [`RetornoSefaz`]; `Err` é não ter conseguido falar com a API.
    ///
    /// # Errors
    /// [`crate::ErroFiscal::ServicoIndisponivel`] (tentar de novo depois);
    /// [`crate::ErroFiscal::ApiFiscalRecusou`] (o provedor recusou o pedido — corrigir).
    fn emitir_nfce(&self, nota: &Nfce, csc: &Csc) -> Resultado<RetornoSefaz>;

    /// Consulta a situação de um documento pela chave — o passo obrigatório antes de reenviar
    /// uma nota cujo resultado não chegou.
    ///
    /// # Errors
    /// Como [`Self::emitir_nfce`].
    fn consultar_documento(&self, chave: &ChaveAcesso, uf: Uf) -> Resultado<RetornoSefaz>;
}

/// Uma porta compartilhada é uma porta (o simulado de um teste, servindo várias empresas).
impl<T: PortaFiscal + ?Sized> PortaFiscal for std::sync::Arc<T> {
    fn distribuicao_dfe(&self, cnpj: &Cnpj, desde: Nsu) -> Resultado<Vec<ResumoDfe>> {
        (**self).distribuicao_dfe(cnpj, desde)
    }

    fn baixar_xml(&self, chave: &ChaveAcesso) -> Resultado<XmlNota> {
        (**self).baixar_xml(chave)
    }

    fn status_servico(&self, uf: Uf, modelo: ModeloFiscal) -> Resultado<StatusSefaz> {
        (**self).status_servico(uf, modelo)
    }

    fn emitir_nfce(&self, nota: &Nfce, csc: &Csc) -> Resultado<RetornoSefaz> {
        (**self).emitir_nfce(nota, csc)
    }

    fn consultar_documento(&self, chave: &ChaveAcesso, uf: Uf) -> Resultado<RetornoSefaz> {
        (**self).consultar_documento(chave, uf)
    }
}
