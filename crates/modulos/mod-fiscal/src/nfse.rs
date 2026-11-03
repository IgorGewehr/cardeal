//! A **NFS-e da ordem de serviço** (`docs/modulos/fiscal.md` §7, "NFS-e"): a mão de obra da OS
//! faturada vira uma NFS-e (DPS do Padrão Nacional), pela mesma fila das outras notas — sem
//! internet, espera; sem resposta, **consulta pelo DPS antes de reenviar** (o mesmo número não
//! gera duas notas); e o cancelamento é um pedido que a fila leva.
//!
//! As peças da OS são mercadoria: não entram na NFS-e (saem em NF-e/NFC-e, à parte). Tabela
//! própria (`fiscal_nfse`): a NFS-e tem número do município, chave de 50 dígitos e eventos
//! diferentes dos da NF-e, e não cabe na série/chave de 44 dígitos de `fiscal_documento`.

use cardeal_fiscal::{
    Emitente, MotivoCancelamentoNfse, Nfse, RetornoNfse, SituacaoNfse, TomadorNfse,
};
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Data, Dinheiro, Id, Instante, Percentual};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::AmbienteFiscal;

/// A origem da NFS-e da OS (o id é o da ordem de serviço).
pub const ORIGEM_OS: &str = "os";

/// O mínimo de caracteres da justificativa do cancelamento.
pub const JUSTIFICATIVA_MINIMA: usize = 15;

/// Como a empresa emite a NFS-e.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfiguracaoNfse {
    /// O código de tributação nacional (`cTribNac`, 6 dígitos).
    pub codigo_tributacao_nacional: String,
    /// O item da LC 116 (`14.01`).
    pub item_lc116: String,
    /// O código de tributação do município, quando ele pede.
    pub codigo_tributacao_municipal: Option<String>,
    /// O código NBS (9 dígitos), quando informado.
    pub nbs: Option<String>,
    /// No Simples: o percentual aproximado dos tributos; no regime normal: a alíquota do ISS.
    pub aliquota: Percentual,
    /// A série do DPS.
    pub serie: u32,
    /// O próximo número do DPS no ambiente atual.
    pub proximo_numero: u64,
    /// O ambiente atual.
    pub ambiente: AmbienteFiscal,
}

/// O código de tributação nacional: 6 dígitos (o ponto e o traço do anexo saem).
fn codigo_nacional(texto: &str) -> Option<String> {
    let d: String = texto.chars().filter(char::is_ascii_digit).collect();
    (d.len() == 6).then_some(d)
}

/// O item da LC 116: `dd.dd` (aceita `1401` e `14.01`).
fn item_lc116(texto: &str) -> Option<String> {
    let d: String = texto.chars().filter(char::is_ascii_digit).collect();
    (d.len() == 4).then(|| format!("{}.{}", &d[..2], &d[2..]))
}

/// Define a NFS-e da empresa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigurarNfse {
    /// O código de tributação nacional.
    pub codigo_tributacao_nacional: String,
    /// O item da LC 116.
    pub item_lc116: String,
    /// O código de tributação municipal (opcional).
    pub codigo_tributacao_municipal: Option<String>,
    /// O NBS (opcional).
    pub nbs: Option<String>,
    /// O percentual (Simples) ou a alíquota do ISS (regime normal).
    pub aliquota: Percentual,
    /// A série do DPS.
    pub serie: u32,
    /// O próximo número no ambiente atual (`None`: não mexe).
    pub proximo_numero: Option<u64>,
}

impl Comando for ConfigurarNfse {
    type Saida = ConfiguracaoNfse;
    const PERMISSAO: &'static str = "fiscal.configurar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<ConfiguracaoNfse> {
        sqlite::configurar(&self, ctx, uow)
    }
}

/// A configuração da NFS-e (`None`: ainda não configurada).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConfiguracaoDaNfse;

impl Consulta for ConfiguracaoDaNfse {
    type Saida = Option<ConfiguracaoNfse>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Option<ConfiguracaoNfse>> {
        sqlite::configuracao(c, ctx.empresa)
    }
}

/// Em que pé está a NFS-e.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoNfse {
    /// Na fila.
    Pendente,
    /// Enviada sem resposta: a fila consulta pelo DPS antes de reenviar.
    Incerta,
    /// Autorizada.
    Autorizada,
    /// Recusada (o motivo diz o que corrigir; emitir de novo remonta no mesmo número).
    Rejeitada,
    /// Cancelada.
    Cancelada,
}

impl EstadoNfse {
    /// O texto da base.
    #[must_use]
    pub const fn como_str(self) -> &'static str {
        match self {
            Self::Pendente => "Pendente",
            Self::Incerta => "Incerta",
            Self::Autorizada => "Autorizada",
            Self::Rejeitada => "Rejeitada",
            Self::Cancelada => "Cancelada",
        }
    }

    /// O contrário de [`Self::como_str`].
    #[must_use]
    pub fn de_str(s: &str) -> Self {
        match s {
            "Incerta" => Self::Incerta,
            "Autorizada" => Self::Autorizada,
            "Rejeitada" => Self::Rejeitada,
            "Cancelada" => Self::Cancelada,
            _ => Self::Pendente,
        }
    }

    /// Para a tela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Pendente => "Na fila",
            Self::Incerta => "Conferindo",
            Self::Autorizada => "Autorizada",
            Self::Rejeitada => "Rejeitada",
            Self::Cancelada => "Cancelada",
        }
    }
}

/// Uma NFS-e.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NfseResumo {
    /// Identidade.
    pub id: Id,
    /// A ordem de serviço.
    pub origem: Id,
    /// O ambiente.
    pub ambiente: AmbienteFiscal,
    /// A série do DPS.
    pub serie: u32,
    /// O número do DPS.
    pub numero: u64,
    /// Em que pé está.
    pub estado: EstadoNfse,
    /// O valor dos serviços.
    pub total: Dinheiro,
    /// O número da NFS-e (do município).
    pub numero_nfse: Option<u64>,
    /// A chave de acesso (50 dígitos).
    pub chave: Option<String>,
    /// O código de verificação.
    pub codigo_verificacao: Option<String>,
    /// O que o município disse (ou o que impede).
    pub motivo: Option<String>,
    /// O cancelamento foi pedido e espera a fila.
    pub cancelamento_pedido: bool,
    /// Quando foi criada.
    pub criado_em: Instante,
    /// Quando foi autorizada.
    pub autorizado_em: Option<Instante>,
}

/// Cria (ou, rejeitada, remonta) a NFS-e da OS faturada e a põe na fila.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EmitirNfseDaOs {
    /// A ordem de serviço.
    pub ordem: Id,
}

impl Comando for EmitirNfseDaOs {
    type Saida = NfseResumo;
    const PERMISSAO: &'static str = "fiscal.emitir";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<NfseResumo> {
        sqlite::emitir_da_os(self.ordem, ctx, uow)
    }
}

/// A NFS-e (viva) da OS, se há.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NfseDaOs {
    /// A ordem de serviço.
    pub ordem: Id,
}

impl Consulta for NfseDaOs {
    type Saida = Option<NfseResumo>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Option<NfseResumo>> {
        sqlite::da_origem(c, ctx.empresa, ORIGEM_OS, self.ordem)
    }
}

/// Pede o cancelamento de uma NFS-e autorizada; a fila o leva ao município.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelarNfse {
    /// A NFS-e.
    pub nfse: Id,
    /// Por quê (o código do evento).
    pub motivo: MotivoCancelamentoNfse,
    /// A justificativa (ao menos [`JUSTIFICATIVA_MINIMA`] caracteres).
    pub justificativa: String,
}

impl Comando for CancelarNfse {
    type Saida = NfseResumo;
    const PERMISSAO: &'static str = "fiscal.cancelar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<NfseResumo> {
        sqlite::pedir_cancelamento(&self, ctx, uow)
    }
}

/// A OS que vira NFS-e: o número, o aparelho e as mãos de obra cobradas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsParaNota {
    /// O número da OS.
    pub numero: u64,
    /// O aparelho.
    pub equipamento: String,
    /// (descrição, valor) de cada mão de obra.
    pub servicos: Vec<(String, Dinheiro)>,
}

/// O tomador a partir do cadastro: o documento, o endereço com o código do município e o
/// e-mail, cada um quando há (a NFS-e aceita o tomador sem eles).
#[must_use]
pub fn tomador_do_cliente(
    nome: &str,
    documentos: &[mod_clientes::DocumentoPessoa],
    enderecos: &[mod_clientes::Endereco],
    email: Option<String>,
) -> TomadorNfse {
    if let Ok(d) =
        crate::montagem::destinatario_do_cliente(nome, documentos, enderecos, email.clone())
    {
        return TomadorNfse {
            documento: Some(d.documento),
            nome: d.nome,
            endereco: Some(d.endereco),
            email: d.email,
        };
    }
    let documento = documentos.iter().find_map(|d| match d.tipo {
        mod_clientes::TipoDocumento::Cnpj => cardeal_kernel::Cnpj::novo(&d.numero)
            .ok()
            .map(cardeal_fiscal::DocumentoConsumidor::Cnpj),
        mod_clientes::TipoDocumento::Cpf => cardeal_kernel::Cpf::novo(&d.numero)
            .ok()
            .map(cardeal_fiscal::DocumentoConsumidor::Cpf),
        _ => None,
    });
    TomadorNfse {
        documento,
        nome: nome.trim().chars().take(150).collect(),
        endereco: None,
        email,
    }
}

/// Monta a NFS-e da OS — ou o que impede, em frases.
///
/// # Errors
/// O que impede: sem inscrição municipal, sem serviço cobrado, códigos inválidos.
#[allow(clippy::too_many_arguments)]
pub fn montar_nfse_da_os(
    prestador: Emitente,
    tomador: TomadorNfse,
    os: &OsParaNota,
    cfg: &ConfiguracaoNfse,
    serie: u32,
    numero: u64,
    competencia: Data,
) -> Result<Nfse, Vec<String>> {
    let mut problemas = Vec::new();
    if prestador
        .inscricao_municipal
        .as_deref()
        .is_none_or(|im| im.trim().is_empty())
    {
        problemas.push(
            "a empresa está sem inscrição municipal (Configurações › Fiscal) — a NFS-e precisa dela"
                .to_string(),
        );
    }
    let servicos: Vec<&(String, Dinheiro)> =
        os.servicos.iter().filter(|(_, v)| v.e_positivo()).collect();
    let valor: Dinheiro = servicos.iter().map(|(_, v)| *v).sum();
    if !valor.e_positivo() {
        problemas.push(
            "a OS não tem mão de obra cobrada — sem serviço, não há NFS-e (as peças saem em NF-e)"
                .to_string(),
        );
    }
    let Some(codigo) = codigo_nacional(&cfg.codigo_tributacao_nacional) else {
        problemas.push("o código de tributação nacional precisa de 6 dígitos".into());
        return Err(problemas);
    };
    let Some(item) = item_lc116(&cfg.item_lc116) else {
        problemas.push("o item da LC 116 precisa ser como 14.01".into());
        return Err(problemas);
    };
    if !problemas.is_empty() {
        return Err(problemas);
    }
    let detalhe: Vec<String> = servicos
        .iter()
        .map(|(d, v)| format!("{} ({})", d.trim(), v.formatar_com_simbolo()))
        .collect();
    let discriminacao: String = format!(
        "OS {} - {}: {}",
        os.numero,
        os.equipamento.trim(),
        detalhe.join("; ")
    )
    .chars()
    .take(2000)
    .collect();
    Ok(Nfse {
        prestador,
        tomador,
        codigo_tributacao_nacional: codigo,
        item_lc116: item,
        codigo_tributacao_municipal: cfg.codigo_tributacao_municipal.clone(),
        nbs: cfg.nbs.clone(),
        discriminacao,
        valor_servicos: valor,
        desconto: Dinheiro::ZERO,
        aliquota: cfg.aliquota,
        iss_retido: false,
        serie,
        numero,
        competencia,
        informacoes: None,
    })
}

/// O que fazer com a resposta de um pedido de NFS-e.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestinoNfse {
    /// Autorizada: o que veio.
    Autorizada(RetornoNfse),
    /// Recusada, com o motivo.
    Rejeitada(String),
    /// Sem resposta que decida: consultar pelo DPS na próxima rodada.
    Conferir(String),
    /// A consulta disse que o DPS não chegou: emitir de novo.
    Reenviar,
    /// Cancelada: o XML do evento.
    Cancelada(Option<String>),
    /// O cancelamento foi recusado: a nota segue autorizada.
    CancelamentoRecusado(String),
    /// A API não respondeu: tentar de novo depois, no mesmo estado.
    Esperar(String),
}

/// Decide o que fazer com a resposta. `cancelando`: era um pedido de cancelamento.
#[must_use]
pub fn decidir_nfse(r: Result<RetornoNfse, String>, cancelando: bool) -> DestinoNfse {
    let r = match r {
        Ok(r) => r,
        Err(m) => return DestinoNfse::Esperar(m),
    };
    if cancelando {
        return match r.situacao {
            SituacaoNfse::Cancelada => DestinoNfse::Cancelada(r.xml),
            SituacaoNfse::Rejeitada => DestinoNfse::CancelamentoRecusado(r.motivo()),
            _ => DestinoNfse::Esperar(r.motivo()),
        };
    }
    match r.situacao {
        SituacaoNfse::Autorizada if r.chave.is_some() || r.numero.is_some() => {
            DestinoNfse::Autorizada(r)
        }
        SituacaoNfse::Autorizada | SituacaoNfse::Processando | SituacaoNfse::Erro => {
            DestinoNfse::Conferir(r.motivo())
        }
        SituacaoNfse::Rejeitada if r.nao_encontrada() => DestinoNfse::Reenviar,
        SituacaoNfse::Rejeitada => DestinoNfse::Rejeitada(r.motivo()),
        SituacaoNfse::Cancelada => DestinoNfse::Cancelada(r.xml),
    }
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite;

#[cfg(test)]
mod testes {
    use cardeal_fiscal::{Crt, EnderecoEmitente};
    use cardeal_kernel::{Cnpj, Uf};

    use super::*;

    fn prestador(im: Option<&str>) -> Emitente {
        Emitente {
            cnpj: Cnpj::novo("11222333000181").unwrap(),
            razao_social: "ASSISTENCIA LTDA".into(),
            nome_fantasia: "A".into(),
            inscricao_estadual: "0960000000".into(),
            inscricao_municipal: im.map(Into::into),
            crt: Crt::SimplesNacional,
            endereco: EnderecoEmitente {
                logradouro: "Rua A".into(),
                numero: "1".into(),
                complemento: String::new(),
                bairro: "Centro".into(),
                codigo_municipio: "4305108".into(),
                municipio: "Caxias do Sul".into(),
                uf: Uf::Rs,
                cep: "95020000".into(),
                telefone: None,
            },
        }
    }

    fn cfg() -> ConfiguracaoNfse {
        ConfiguracaoNfse {
            codigo_tributacao_nacional: "14.01.01".into(),
            item_lc116: "1401".into(),
            codigo_tributacao_municipal: None,
            nbs: None,
            aliquota: Percentual::de_str("6").unwrap(),
            serie: 900,
            proximo_numero: 1,
            ambiente: AmbienteFiscal::Homologacao,
        }
    }

    fn os() -> OsParaNota {
        OsParaNota {
            numero: 42,
            equipamento: "Notebook Dell".into(),
            servicos: vec![
                ("Diagnóstico".into(), Dinheiro::reais(50)),
                ("Troca de bateria".into(), Dinheiro::reais(100)),
                ("Limpeza (cortesia)".into(), Dinheiro::ZERO),
            ],
        }
    }

    fn tomador() -> TomadorNfse {
        TomadorNfse {
            documento: None,
            nome: "Consumidor".into(),
            endereco: None,
            email: None,
        }
    }

    #[test]
    fn a_nfse_leva_as_maos_de_obra_cobradas() {
        let n = montar_nfse_da_os(
            prestador(Some("12345")),
            tomador(),
            &os(),
            &cfg(),
            900,
            7,
            Data::de_ymd(2026, 10, 28).unwrap(),
        )
        .unwrap();
        assert_eq!(n.valor_servicos, Dinheiro::reais(150));
        assert_eq!(n.codigo_tributacao_nacional, "140101");
        assert_eq!(n.item_lc116, "14.01");
        assert_eq!(
            n.discriminacao,
            "OS 42 - Notebook Dell: Diagnóstico (R$ 50,00); Troca de bateria (R$ 100,00)"
        );
        assert_eq!((n.serie, n.numero), (900, 7));
    }

    #[test]
    fn sem_inscricao_municipal_e_sem_servico_nao_sai() {
        let mut so_pecas = os();
        so_pecas.servicos.clear();
        let e = montar_nfse_da_os(
            prestador(None),
            tomador(),
            &so_pecas,
            &cfg(),
            900,
            7,
            Data::de_ymd(2026, 10, 28).unwrap(),
        )
        .unwrap_err();
        assert_eq!(e.len(), 2);
        assert!(e[0].contains("inscrição municipal"));
        assert!(e[1].contains("sem serviço"));
    }

    #[test]
    fn a_resposta_do_municipio() {
        let r = |situacao, mensagens: &[&str], chave: Option<&str>| RetornoNfse {
            situacao,
            numero: chave.map(|_| 42),
            chave: chave.map(Into::into),
            codigo_verificacao: None,
            xml: None,
            mensagens: mensagens.iter().map(|m| (*m).to_string()).collect(),
        };
        assert!(matches!(
            decidir_nfse(Ok(r(SituacaoNfse::Autorizada, &[], Some("1"))), false),
            DestinoNfse::Autorizada(_)
        ));
        assert!(matches!(
            decidir_nfse(Ok(r(SituacaoNfse::Erro, &["timeout"], None)), false),
            DestinoNfse::Conferir(_)
        ));
        assert_eq!(
            decidir_nfse(
                Ok(r(SituacaoNfse::Rejeitada, &["404 — não encontrada"], None)),
                false
            ),
            DestinoNfse::Reenviar
        );
        assert!(matches!(
            decidir_nfse(Ok(r(SituacaoNfse::Rejeitada, &["E123 — x"], None)), false),
            DestinoNfse::Rejeitada(_)
        ));
        assert!(matches!(
            decidir_nfse(Err("fora do ar".into()), false),
            DestinoNfse::Esperar(_)
        ));
        assert!(matches!(
            decidir_nfse(Ok(r(SituacaoNfse::Cancelada, &[], None)), true),
            DestinoNfse::Cancelada(_)
        ));
        assert!(matches!(
            decidir_nfse(Ok(r(SituacaoNfse::Rejeitada, &["prazo"], None)), true),
            DestinoNfse::CancelamentoRecusado(_)
        ));
    }
}
