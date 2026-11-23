//! A **DPS** (Declaração de Prestação de Serviço) e o **pedido de registro de evento** de
//! cancelamento, no leiaute 1.01 do Sistema Nacional NFS-e — na ordem exata do XSD oficial
//! (`DPS_v1.01.xsd`, `pedRegEvento_v1.01.xsd`).
//!
//! Há dois leiautes 1.01 em circulação: o **vigente** em produção (esquemas de 09/02/2026) e o da
//! **NT 009** (esquemas de 20/10/2026, já na produção restrita). Para uma nota de serviço comum a
//! diferença é o `finNFSe`, que a NT 009 tornou obrigatório dentro do `infDPS` (e que o leiaute
//! vigente não aceita ali), e a ordem do `cNBS` (antes da descrição na NT 009, depois no vigente). Os testes validam a DPS contra os dois conjuntos de XSD.
//!
//! O grupo `IBSCBS` (reforma tributária) é opcional nos dois leiautes e não é enviado.

use cardeal_kernel::{Dinheiro, Fuso, Instante, Percentual};

use super::xml::{digitos, texto_xsd, Xml};
use crate::emissao::{Crt, DocumentoConsumidor, EnderecoEmitente};
use crate::nfse::{MotivoCancelamentoNfse, Nfse, OpcaoSimples};
use crate::porta::AmbienteFiscal;

/// O namespace dos documentos do Sistema Nacional NFS-e.
pub const NS_NFSE: &str = "http://www.sped.fazenda.gov.br/nfse";
/// A versão do leiaute.
pub const VERSAO: &str = "1.01";
/// O `verAplic` (até 20 caracteres).
const VER_APLIC: &str = concat!("Cardeal-", env!("CARGO_PKG_VERSION"));

/// Qual leiaute 1.01 escrever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeiauteDps {
    /// O vigente em produção (esquemas de 09/02/2026).
    Vigente,
    /// O da NT 009 (esquemas de 20/10/2026): `finNFSe` no `infDPS`.
    Nt009,
}

impl LeiauteDps {
    /// O outro leiaute (a troca quando a Sefin recusa pelo esquema).
    #[must_use]
    pub const fn outro(self) -> Self {
        match self {
            Self::Vigente => Self::Nt009,
            Self::Nt009 => Self::Vigente,
        }
    }
}

/// A DPS montada (ainda sem assinatura).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpsMontada {
    /// O `Id` (`DPS` + 42 dígitos).
    pub id: String,
    /// O `<infDPS …>…</infDPS>` canônico, sem `xmlns` (herdado do `<DPS>`).
    pub inf_dps: String,
}

/// O identificador da DPS: `DPS` + município emissor (7) + tipo de inscrição (`2` = CNPJ) +
/// CNPJ (14) + série (5) + número (15). É também a chave da consulta `GET /dps/{id}`.
#[must_use]
pub fn id_dps(codigo_municipio: &str, cnpj: &str, serie: u32, numero: u64) -> String {
    format!(
        "DPS{:0>7}2{:0>14}{serie:05}{numero:015}",
        digitos(codigo_municipio),
        cnpj.chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
    )
}

/// `1234.50` (o `TSDec15V2`: ponto e duas casas).
fn valor(d: Dinheiro) -> String {
    let c = d.em_centavos();
    format!("{}.{:02}", c / 100, (c % 100).abs())
}

/// `5.00` (duas casas).
fn percentual(p: Percentual) -> String {
    p.formatar(2).replace(',', ".").replace([' ', '%'], "")
}

/// `2026-11-23T10:15:00-03:00` no fuso de Brasília.
pub(crate) fn data_hora(instante: Instante) -> String {
    let fuso = Fuso::BRASILIA;
    let (d, h) = (instante.data(fuso), instante.hora(fuso));
    let min = fuso.em_minutos();
    format!(
        "{}T{:02}:{:02}:{:02}{}{:02}:{:02}",
        d.formatar_iso(),
        h.hora(),
        h.minuto(),
        h.segundo(),
        if min < 0 { '-' } else { '+' },
        min.abs() / 60,
        min.abs() % 60
    )
}

/// A opção pelo Simples (`opSimpNac`): a da nota, ou a do regime do prestador.
pub(crate) fn opcao_simples(n: &Nfse) -> OpcaoSimples {
    n.opcao_simples.unwrap_or(match n.prestador.crt {
        Crt::RegimeNormal => OpcaoSimples::NaoOptante,
        Crt::SimplesNacional | Crt::SimplesExcessoSublimite => OpcaoSimples::MeEpp,
    })
}

fn endereco(x: &mut Xml, e: &EnderecoEmitente) -> bool {
    let (cmun, cep) = (digitos(&e.codigo_municipio), digitos(&e.cep));
    let (Some(lgr), Some(bairro)) = (
        texto_xsd(&e.logradouro, 255, false),
        texto_xsd(&e.bairro, 60, false),
    ) else {
        return false;
    };
    if cmun.len() != 7 || cep.len() != 8 {
        return false;
    }
    let nro = texto_xsd(&e.numero, 60, false).unwrap_or_else(|| "S/N".into());
    x.abrir("end", &[]);
    x.abrir("endNac", &[])
        .folha("cMun", &cmun)
        .folha("CEP", &cep)
        .fechar("endNac");
    x.folha("xLgr", &lgr).folha("nro", &nro);
    x.talvez("xCpl", texto_xsd(&e.complemento, 156, false).as_deref());
    x.folha("xBairro", &bairro).fechar("end");
    true
}

/// Monta a DPS — ou diz, em frases, o que impede. (Longa de propósito: segue o XSD na ordem.)
///
/// # Errors
/// O que falta ou está fora do que o XSD aceita (para a tela, antes de gastar uma chamada).
#[allow(clippy::too_many_lines)]
pub fn montar_dps(
    n: &Nfse,
    ambiente: AmbienteFiscal,
    leiaute: LeiauteDps,
    emitida_em: Instante,
) -> Result<DpsMontada, Vec<String>> {
    let mut problemas = Vec::new();
    let p = &n.prestador;
    let cnpj = p.cnpj.sem_mascara();
    let cmun = digitos(&p.endereco.codigo_municipio);
    if cmun.len() != 7 {
        problemas.push("o código IBGE do município da empresa precisa de 7 dígitos".to_string());
    }
    if n.codigo_tributacao_nacional.len() != 6
        || !n
            .codigo_tributacao_nacional
            .bytes()
            .all(|b| b.is_ascii_digit())
    {
        problemas.push("o código de tributação nacional precisa de 6 dígitos".into());
    }
    let ctribmun = n
        .codigo_tributacao_municipal
        .as_deref()
        .map(digitos)
        .filter(|c| !c.is_empty());
    if ctribmun.as_ref().is_some_and(|c| c.len() != 3) {
        problemas.push("o código de tributação municipal tem 3 dígitos no padrão nacional".into());
    }
    let nbs = n.nbs.as_deref().map(digitos).filter(|c| !c.is_empty());
    if nbs.as_ref().is_some_and(|c| c.len() != 9) {
        problemas.push("o código NBS precisa de 9 dígitos".into());
    }
    if !n.valor_servicos.e_positivo() {
        problemas.push("o valor do serviço precisa ser maior que zero".into());
    }
    if n.desconto.e_negativo() || n.desconto >= n.valor_servicos {
        problemas.push("o desconto precisa ser menor que o valor do serviço".into());
    }
    let Some(descricao) = texto_xsd(&n.discriminacao, 2000, true) else {
        problemas.push("a descrição do serviço está vazia".into());
        return Err(problemas);
    };
    if n.numero == 0 || n.numero > 999_999_999_999_999 {
        problemas.push("o número do DPS precisa ter de 1 a 15 dígitos".into());
    }
    if n.serie > 89_999 {
        problemas
            .push("a série do DPS vai até 89999 (90000 em diante é do emissor nacional)".into());
    }
    let opcao = opcao_simples(n);
    let pa = (opcao == OpcaoSimples::NaoOptante).then(|| percentual(n.aliquota));
    if pa
        .as_ref()
        .is_some_and(|a| !a.starts_with(|c: char| c.is_ascii_digit()) || a.len() != 4)
    {
        problemas.push("a alíquota do ISS vai de 0,00% a 9,99%".into());
    }
    if !problemas.is_empty() {
        return Err(problemas);
    }

    let id = id_dps(&cmun, &cnpj, n.serie, n.numero);
    let mut x = Xml::novo();
    x.abrir("infDPS", &[("Id", &id)]);
    x.folha(
        "tpAmb",
        if ambiente == AmbienteFiscal::Producao {
            "1"
        } else {
            "2"
        },
    );
    x.folha("dhEmi", &data_hora(emitida_em));
    x.folha("verAplic", VER_APLIC);
    x.folha("serie", &n.serie.to_string());
    x.folha("nDPS", &n.numero.to_string());
    x.folha("dCompet", &n.competencia.formatar_iso());
    x.folha("tpEmit", "1");
    if leiaute == LeiauteDps::Nt009 {
        x.folha("finNFSe", "0");
    }
    x.folha("cLocEmi", &cmun);

    // Prestador: quem emite é ele, então o nome e o endereço vêm do cadastro nacional.
    x.abrir("prest", &[]).folha("CNPJ", &cnpj);
    x.talvez(
        "IM",
        p.inscricao_municipal
            .as_deref()
            .and_then(|im| texto_xsd(im, 15, false))
            .as_deref(),
    );
    x.abrir("regTrib", &[]).folha("opSimpNac", opcao.codigo());
    if opcao == OpcaoSimples::MeEpp {
        let apuracao = if p.crt == Crt::SimplesExcessoSublimite {
            "2"
        } else {
            "1"
        };
        x.folha("regApTribSN", apuracao);
    }
    x.folha("regEspTrib", "0").fechar("regTrib").fechar("prest");

    // Tomador: sem documento, não há grupo (o consumidor que não se identifica).
    if let Some(doc) = &n.tomador.documento {
        x.abrir("toma", &[]);
        match doc {
            DocumentoConsumidor::Cpf(c) => x.folha("CPF", &c.sem_mascara()),
            DocumentoConsumidor::Cnpj(c) => x.folha("CNPJ", &c.sem_mascara()),
        };
        let nome = texto_xsd(&n.tomador.nome, 300, false).unwrap_or_else(|| "CONSUMIDOR".into());
        x.folha("xNome", &nome);
        if let Some(e) = &n.tomador.endereco {
            endereco(&mut x, e);
        }
        let email = n
            .tomador
            .email
            .as_deref()
            .map(str::trim)
            .filter(|e| e.contains('@') && e.len() <= 80);
        x.talvez("email", email);
        x.fechar("toma");
    }

    x.abrir("serv", &[]);
    x.abrir("locPrest", &[])
        .folha("cLocPrestacao", &cmun)
        .fechar("locPrest");
    // A NT 009 mudou a ordem: o NBS antes da descrição.
    x.abrir("cServ", &[])
        .folha("cTribNac", &n.codigo_tributacao_nacional);
    x.talvez("cTribMun", ctribmun.as_deref());
    match leiaute {
        LeiauteDps::Vigente => x
            .folha("xDescServ", &descricao)
            .talvez("cNBS", nbs.as_deref()),
        LeiauteDps::Nt009 => x
            .talvez("cNBS", nbs.as_deref())
            .folha("xDescServ", &descricao),
    };
    x.fechar("cServ");
    if let Some(info) = n
        .informacoes
        .as_deref()
        .and_then(|i| texto_xsd(i, 2000, false))
    {
        x.abrir("infoCompl", &[])
            .folha("xInfComp", &info)
            .fechar("infoCompl");
    }
    x.fechar("serv");

    x.abrir("valores", &[]);
    x.abrir("vServPrest", &[])
        .folha("vServ", &valor(n.valor_servicos))
        .fechar("vServPrest");
    if n.desconto.e_positivo() {
        x.abrir("vDescCondIncond", &[])
            .folha("vDescIncond", &valor(n.desconto))
            .fechar("vDescCondIncond");
    }
    x.abrir("trib", &[]).abrir("tribMun", &[]);
    x.folha("tribISSQN", "1");
    x.folha("tpRetISSQN", if n.iss_retido { "2" } else { "1" });
    x.talvez("pAliq", pa.as_deref());
    x.fechar("tribMun").abrir("totTrib", &[]);
    if opcao == OpcaoSimples::MeEpp && !n.aliquota.e_zero() {
        x.folha("pTotTribSN", &percentual(n.aliquota));
    } else {
        x.folha("indTotTrib", "0");
    }
    x.fechar("totTrib").fechar("trib").fechar("valores");
    x.fechar("infDPS");
    Ok(DpsMontada {
        id,
        inf_dps: x.texto(),
    })
}

/// O pedido de registro do evento de cancelamento (`e101101`), sem assinatura: o `Id` e o
/// `<infPedReg>` canônico.
#[must_use]
pub fn montar_cancelamento(
    chave: &str,
    cnpj_autor: &str,
    motivo: MotivoCancelamentoNfse,
    justificativa: &str,
    ambiente: AmbienteFiscal,
    pedido_em: Instante,
) -> (String, String) {
    let id = format!("PRE{chave}101101");
    let codigo = match motivo {
        MotivoCancelamentoNfse::ErroNaEmissao => "1",
        MotivoCancelamentoNfse::ServicoNaoPrestado => "2",
        // O padrão nacional só tem 1, 2 e 9 ("outros").
        MotivoCancelamentoNfse::Duplicidade | MotivoCancelamentoNfse::ErroDeProcessamento => "9",
    };
    let mut texto = texto_xsd(justificativa, 255, false).unwrap_or_default();
    if texto.chars().count() < 15 {
        texto = format!("{} - {}", motivo.rotulo(), texto)
            .trim_end_matches([' ', '-'])
            .to_string();
        while texto.chars().count() < 15 {
            texto.push('.');
        }
    }
    let mut x = Xml::novo();
    x.abrir("infPedReg", &[("Id", &id)]);
    x.folha(
        "tpAmb",
        if ambiente == AmbienteFiscal::Producao {
            "1"
        } else {
            "2"
        },
    );
    x.folha("verAplic", VER_APLIC);
    x.folha("dhEvento", &data_hora(pedido_em));
    x.folha("CNPJAutor", cnpj_autor);
    x.folha("chNFSe", chave);
    x.abrir("e101101", &[]);
    x.folha("xDesc", "Cancelamento de NFS-e");
    x.folha("cMotivo", codigo);
    x.folha("xMotivo", &texto);
    x.fechar("e101101").fechar("infPedReg");
    (id, x.texto())
}
