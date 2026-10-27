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
//! O que esta versão cobre: a distribuição `DFe` e a Manifestação do Destinatário que
//! `mod-compras` usa, o status do serviço da SEFAZ e a NFC-e (emitir, consultar, cancelar,
//! inutilizar). O adaptador real é [`crate::ApiFiscalHttp`], cliente do `sefaz-api`.

use cardeal_kernel::{Cnpj, Resultado, Uf};
use serde::{Deserialize, Serialize};

use crate::chave::ChaveAcesso;
use crate::dfe::{Manifestacao, PaginaDfe};
use crate::emissao::{
    Csc, EntradaEmContingencia, Nfce, NotaEmContingencia, PedidoInutilizacao, RetornoSefaz,
};

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

    /// O prazo nacional para cancelar depois da autorização, em segundos: 30 minutos para a
    /// NFC-e, 24 horas para a NF-e. Fora dele a SEFAZ recusa (501) — algumas UFs aceitam, e
    /// homologam fora do prazo (155).
    #[must_use]
    pub const fn prazo_de_cancelamento_segundos(self) -> i64 {
        match self {
            Self::Nfe => 24 * 60 * 60,
            Self::Nfce => 30 * 60,
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Nsu(pub u64);

/// A porta fiscal: a SEFAZ, por trás do provedor.
pub trait PortaFiscal: Send + Sync {
    /// Uma página da distribuição `DFe` do `cnpj` (o da empresa, dono do certificado) a partir
    /// de `desde` (exclusive). Quem chama guarda o `ultimo_nsu` e para quando ele alcança o
    /// `maior_nsu` — e só pede de novo depois de uma hora (a SEFAZ barra: 656).
    ///
    /// # Errors
    /// [`crate::ErroFiscal::ServicoIndisponivel`] quando a resposta não chegou ou não serve
    /// (nunca avançar o NSU com ela); [`crate::ErroFiscal::ApiFiscalRecusou`] quando a SEFAZ
    /// recusou o pedido (CNPJ diferente do certificado, por exemplo).
    fn distribuicao_dfe(&self, cnpj: &Cnpj, uf: Uf, desde: Nsu) -> Resultado<PaginaDfe>;

    /// Os documentos `DFe` de uma chave: a nota completa, se a empresa já deu ciência; senão o
    /// resumo.
    ///
    /// # Errors
    /// Como [`Self::distribuicao_dfe`].
    fn consultar_dfe_por_chave(
        &self,
        cnpj: &Cnpj,
        uf: Uf,
        chave: &ChaveAcesso,
    ) -> Resultado<PaginaDfe>;

    /// A Manifestação do Destinatário sobre uma nota recebida. Registrada vem como
    /// `Autorizado` (135), com o XML do evento; já registrada antes, `Rejeitado` com 573.
    ///
    /// # Errors
    /// Como [`Self::emitir_nfce`].
    fn manifestar(
        &self,
        cnpj: &Cnpj,
        chave: &ChaveAcesso,
        tipo: Manifestacao,
        justificativa: Option<&str>,
    ) -> Resultado<RetornoSefaz>;

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
    /// uma nota cujo resultado não chegou. Com o `xml_assinado` (o que a emissão incerta
    /// devolveu), a nota autorizada ou denegada volta com o `nfeProc`; a cancelada volta
    /// `Cancelado` (101/151/155) e com o XML do cancelamento.
    ///
    /// # Errors
    /// Como [`Self::emitir_nfce`].
    fn consultar_documento(
        &self,
        chave: &ChaveAcesso,
        uf: Uf,
        xml_assinado: Option<&str>,
    ) -> Resultado<RetornoSefaz>;

    /// Cancela um documento autorizado (evento 110111). Aceito vem como `Autorizado` (135,
    /// 136, 155); recusado (prazo, por exemplo) como `Rejeitado`.
    ///
    /// # Errors
    /// Como [`Self::emitir_nfce`].
    fn cancelar(
        &self,
        chave: &ChaveAcesso,
        protocolo: &str,
        justificativa: &str,
        uf: Uf,
    ) -> Resultado<RetornoSefaz>;

    /// Inutiliza uma faixa de números (102 = homologada, vem como `Autorizado`).
    ///
    /// # Errors
    /// Como [`Self::emitir_nfce`].
    fn inutilizar(&self, pedido: &PedidoInutilizacao) -> Resultado<RetornoSefaz>;

    /// Assina a NFC-e em **contingência off-line** (`tpEmis = 9`), sem transmitir: a chave e o
    /// XML com o QR Code, que vai impresso no DANFCE e é transmitido depois, sem mudar
    /// ([`Self::transmitir_nfce_em_contingencia`]). Para quando a SEFAZ está fora e a API não.
    ///
    /// # Errors
    /// Como [`Self::emitir_nfce`] — a recusa dos dados vem como
    /// [`crate::ErroFiscal::ApiFiscalRecusou`].
    fn preparar_nfce_em_contingencia(
        &self,
        nota: &Nfce,
        csc: &Csc,
        entrada: &EntradaEmContingencia,
    ) -> Resultado<NotaEmContingencia>;

    /// Transmite a NFC-e assinada em contingência — o mesmo XML, quantas vezes for preciso: a
    /// chave não muda, e se uma transmissão anterior chegou a SEFAZ responde duplicidade (204).
    ///
    /// # Errors
    /// Como [`Self::emitir_nfce`].
    fn transmitir_nfce_em_contingencia(
        &self,
        xml_assinado: &str,
        uf: Uf,
    ) -> Resultado<RetornoSefaz>;
}

/// Uma porta compartilhada é uma porta (o simulado de um teste, servindo várias empresas).
impl<T: PortaFiscal + ?Sized> PortaFiscal for std::sync::Arc<T> {
    fn distribuicao_dfe(&self, cnpj: &Cnpj, uf: Uf, desde: Nsu) -> Resultado<PaginaDfe> {
        (**self).distribuicao_dfe(cnpj, uf, desde)
    }

    fn consultar_dfe_por_chave(
        &self,
        cnpj: &Cnpj,
        uf: Uf,
        chave: &ChaveAcesso,
    ) -> Resultado<PaginaDfe> {
        (**self).consultar_dfe_por_chave(cnpj, uf, chave)
    }

    fn manifestar(
        &self,
        cnpj: &Cnpj,
        chave: &ChaveAcesso,
        tipo: Manifestacao,
        justificativa: Option<&str>,
    ) -> Resultado<RetornoSefaz> {
        (**self).manifestar(cnpj, chave, tipo, justificativa)
    }

    fn status_servico(&self, uf: Uf, modelo: ModeloFiscal) -> Resultado<StatusSefaz> {
        (**self).status_servico(uf, modelo)
    }

    fn emitir_nfce(&self, nota: &Nfce, csc: &Csc) -> Resultado<RetornoSefaz> {
        (**self).emitir_nfce(nota, csc)
    }

    fn consultar_documento(
        &self,
        chave: &ChaveAcesso,
        uf: Uf,
        xml_assinado: Option<&str>,
    ) -> Resultado<RetornoSefaz> {
        (**self).consultar_documento(chave, uf, xml_assinado)
    }

    fn cancelar(
        &self,
        chave: &ChaveAcesso,
        protocolo: &str,
        justificativa: &str,
        uf: Uf,
    ) -> Resultado<RetornoSefaz> {
        (**self).cancelar(chave, protocolo, justificativa, uf)
    }

    fn inutilizar(&self, pedido: &PedidoInutilizacao) -> Resultado<RetornoSefaz> {
        (**self).inutilizar(pedido)
    }

    fn preparar_nfce_em_contingencia(
        &self,
        nota: &Nfce,
        csc: &Csc,
        entrada: &EntradaEmContingencia,
    ) -> Resultado<NotaEmContingencia> {
        (**self).preparar_nfce_em_contingencia(nota, csc, entrada)
    }

    fn transmitir_nfce_em_contingencia(
        &self,
        xml_assinado: &str,
        uf: Uf,
    ) -> Resultado<RetornoSefaz> {
        (**self).transmitir_nfce_em_contingencia(xml_assinado, uf)
    }
}
