//! O DANFE de uma NF-e autorizada: monta o [`cardeal_pdf::DanfePdf`] a partir da NF-e guardada no
//! documento fiscal (emitente, destinatário, itens, duplicatas) e do XML autorizado (a data de
//! emissão), grava numa pasta temporária e abre no visualizador — de onde vai para a impressora
//! (A4) ou para o e-mail do cliente.

use cardeal_cliente::{Motor, Sessao};
use cardeal_fiscal::{DocumentoConsumidor, EnderecoEmitente, ItemDocumento, Nfe};
use cardeal_kernel::{Data, Dinheiro, Fuso, Hora, Id, Instante};
use cardeal_pdf::{DanfePdf, DestinatarioDanfe, DuplicataDanfe, ItemDanfe, TotaisDanfe};
use mod_fiscal::{
    AmbienteFiscal, DetalheDocumento, DocumentoFiscal, EstadoDocumento, ModeloFiscal,
};

const FUSO: Fuso = Fuso::BRASILIA;

/// O CEP com o traço: `95020-000`.
fn cep(c: &str) -> String {
    let d: String = c.chars().filter(char::is_ascii_digit).collect();
    if d.len() == 8 {
        format!("{}-{}", &d[..5], &d[5..])
    } else {
        d
    }
}

/// A data e a hora de emissão do XML (`<dhEmi>2026-10-27T12:53:00-03:00</dhEmi>`), no fuso do
/// emitente.
fn emissao_do_xml(xml: &str) -> Option<Instante> {
    let i = xml.find("<dhEmi>")? + "<dhEmi>".len();
    let t = xml.get(i..i + 19)?;
    let n = |a: usize, b: usize| t.get(a..b)?.parse::<u32>().ok();
    let data = Data::de_ymd(i32::try_from(n(0, 4)?).ok()?, n(5, 7)?, n(8, 10)?).ok()?;
    let hora = Hora::de_hms(n(11, 13)?, n(14, 16)?, n(17, 19)?).ok()?;
    Some(Instante::de_data_hora(data, hora, FUSO))
}

fn endereco(e: &EnderecoEmitente) -> String {
    let complemento = if e.complemento.is_empty() {
        String::new()
    } else {
        format!(" {}", e.complemento)
    };
    format!("{}, {}{complemento}", e.logradouro, e.numero)
}

fn item(i: &ItemDocumento) -> ItemDanfe {
    let icms = &i.impostos.icms;
    let situacao = icms
        .cst
        .clone()
        .or_else(|| icms.csosn.clone())
        .unwrap_or_default();
    ItemDanfe {
        codigo: i.codigo.clone(),
        descricao: i.descricao.clone(),
        ncm: i.ncm.clone(),
        cst: format!("{}{situacao}", icms.origem),
        cfop: i.cfop.clone(),
        unidade: i.unidade.clone(),
        quantidade: i.quantidade,
        valor_unitario: i.valor_unitario,
        valor_total: i.valor_bruto,
        base_icms: icms.base.unwrap_or(Dinheiro::ZERO),
        valor_icms: icms.valor.unwrap_or(Dinheiro::ZERO),
        aliquota_icms: icms.aliquota,
    }
}

fn totais(n: &Nfe) -> TotaisDanfe {
    let soma = |f: &dyn Fn(&ItemDocumento) -> Dinheiro| n.itens.iter().map(f).sum::<Dinheiro>();
    // Na devolução: o ST, o IPI, o frete, o seguro e as outras despesas que voltam.
    let devolvido = |f: &dyn Fn(&cardeal_fiscal::DevolucaoDoItem) -> Dinheiro| {
        n.devolvidos.iter().map(f).sum::<Dinheiro>()
    };
    TotaisDanfe {
        base_icms: soma(&|i| i.impostos.icms.base.unwrap_or(Dinheiro::ZERO)),
        valor_icms: soma(&|i| i.impostos.icms.valor.unwrap_or(Dinheiro::ZERO)),
        base_icms_st: devolvido(&|d| d.base_st),
        valor_icms_st: devolvido(&|d| d.valor_st + d.valor_fcp_st),
        produtos: soma(&|i| i.valor_bruto),
        frete: devolvido(&|d| d.frete),
        seguro: devolvido(&|d| d.seguro),
        desconto: soma(&|i| i.desconto),
        outras: devolvido(&|d| d.outras),
        ipi: devolvido(&|d| d.ipi_devolvido),
        total: n.total(),
    }
}

/// O DANFE de uma NF-e autorizada.
///
/// # Errors
/// O documento não é uma NF-e autorizada, ou não tem a nota guardada.
pub fn montar(d: &DetalheDocumento) -> Result<DanfePdf, String> {
    if d.resumo.modelo != ModeloFiscal::Nfe {
        return Err("O DANFE é da NF-e; a NFC-e tem o cupom fiscal".into());
    }
    if d.resumo.estado != EstadoDocumento::Autorizada {
        return Err(format!(
            "Só a NF-e autorizada tem DANFE (esta está {})",
            d.resumo.estado.rotulo()
        ));
    }
    let n = d
        .nfe
        .as_ref()
        .ok_or_else(|| "A NF-e guardada não está legível".to_owned())?;
    let chave = d
        .resumo
        .chave
        .clone()
        .ok_or_else(|| "A NF-e autorizada está sem a chave de acesso".to_owned())?;
    let e = &n.emitente;
    let de = &n.destinatario;
    let mut endereco_emitente = vec![
        format!("{} - {}", endereco(&e.endereco), e.endereco.bairro),
        format!(
            "{}/{} - CEP {}",
            e.endereco.municipio,
            e.endereco.uf.sigla(),
            cep(&e.endereco.cep)
        ),
    ];
    if let Some(t) = &e.endereco.telefone {
        endereco_emitente.push(format!("Fone: {t}"));
    }
    Ok(DanfePdf {
        razao_social: e.razao_social.clone(),
        endereco_emitente,
        cnpj: e.cnpj.formatar(),
        inscricao_estadual: e.inscricao_estadual.clone(),
        saida: n.saida,
        numero: d.resumo.numero,
        serie: d.resumo.serie,
        chave,
        protocolo: d.resumo.protocolo.clone(),
        autorizacao: d.resumo.autorizado_em,
        natureza: n.natureza.clone(),
        emissao: d
            .xml
            .as_deref()
            .and_then(emissao_do_xml)
            .unwrap_or(d.resumo.criado_em),
        destinatario: DestinatarioDanfe {
            nome: de.nome.clone(),
            documento: match &de.documento {
                DocumentoConsumidor::Cpf(c) => c.formatar(),
                DocumentoConsumidor::Cnpj(c) => c.formatar(),
            },
            endereco: endereco(&de.endereco),
            bairro: de.endereco.bairro.clone(),
            cep: cep(&de.endereco.cep),
            municipio: de.endereco.municipio.clone(),
            uf: de.endereco.uf.sigla().to_owned(),
            fone: de.endereco.telefone.clone().unwrap_or_default(),
            inscricao_estadual: de.inscricao_estadual.clone().unwrap_or_default(),
        },
        duplicatas: n
            .duplicatas
            .iter()
            .map(|dup| DuplicataDanfe {
                numero: dup.numero.clone(),
                vencimento: dup.vencimento,
                valor: dup.valor,
            })
            .collect(),
        totais: totais(n),
        frete: "9 - Sem Ocorrência de Transporte".into(),
        itens: n.itens.iter().map(item).collect(),
        informacoes: n.informacoes.clone(),
        homologacao: d.resumo.ambiente == AmbienteFiscal::Homologacao,
        fuso: FUSO,
    })
}

/// Gera o DANFE do documento, grava na pasta temporária e abre no visualizador. Devolve o
/// caminho.
///
/// # Errors
/// Documento inexistente, não autorizado, ou falha ao gravar.
pub fn imprimir(
    motor: &Motor,
    sessao: &Sessao,
    documento: Id,
) -> Result<std::path::PathBuf, String> {
    let detalhe: DetalheDocumento = motor
        .consultar(
            sessao,
            "fiscal.documento.v1",
            &DocumentoFiscal { documento },
        )
        .map_err(|e| e.mensagem)?;
    let pdf = cardeal_pdf::gerar_danfe(&montar(&detalhe)?);
    let pasta = std::env::temp_dir().join("Cardeal").join("notas");
    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
    let caminho = pasta.join(format!(
        "NFe-{}-{}.pdf",
        detalhe.resumo.serie, detalhe.resumo.numero
    ));
    std::fs::write(&caminho, pdf).map_err(|e| e.to_string())?;
    if !cfg!(test) {
        open::that_detached(&caminho).map_err(|e| e.to_string())?;
    }
    Ok(caminho)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn detalhe_da_nfe(estado: EstadoDocumento) -> DetalheDocumento {
        let mut d = crate::danfce::testes::detalhe(estado);
        let nfce = d.nota.take().unwrap();
        let mut endereco = nfce.emitente.endereco.clone();
        endereco.municipio = "Caxias do Sul".into();
        endereco.cep = "95020000".into();
        d.nfe = Some(Nfe {
            emitente: nfce.emitente,
            destinatario: cardeal_fiscal::Destinatario {
                documento: DocumentoConsumidor::Cpf(
                    cardeal_kernel::Cpf::novo("52998224725").unwrap(),
                ),
                nome: "João Silva".into(),
                indicador_ie: cardeal_fiscal::IndicadorIe::NaoContribuinte,
                inscricao_estadual: None,
                endereco,
                email: None,
            },
            itens: nfce.itens,
            natureza: "Venda de mercadoria".into(),
            saida: true,
            finalidade: cardeal_fiscal::FinalidadeNfe::Normal,
            referencias: Vec::new(),
            consumidor_final: true,
            presencial: false,
            pagamentos: nfce.pagamentos,
            a_prazo: true,
            duplicatas: vec![cardeal_fiscal::Duplicata {
                numero: "001".into(),
                vencimento: Data::de_ymd(2026, 11, 26).unwrap(),
                valor: Dinheiro::reais(45),
            }],
            serie: 2,
            numero: 51,
            informacoes: Some("DOCUMENTO EMITIDO POR ME OU EPP".into()),
            devolvidos: Vec::new(),
        });
        d.resumo.modelo = ModeloFiscal::Nfe;
        d.resumo.serie = 2;
        d.resumo.numero = 51;
        d.xml = Some("<nfeProc><dhEmi>2026-10-27T09:15:00-03:00</dhEmi></nfeProc>".into());
        d
    }

    #[test]
    fn monta_o_danfe_da_nfe_autorizada() {
        let d = montar(&detalhe_da_nfe(EstadoDocumento::Autorizada)).unwrap();
        assert_eq!(d.destinatario.documento, "529.982.247-25");
        assert_eq!(d.destinatario.cep, "95020-000");
        assert_eq!(d.destinatario.municipio, "Caxias do Sul");
        assert_eq!((d.numero, d.serie), (51, 2));
        assert_eq!(d.emissao.hora(FUSO).formatar(), "09:15");
        assert_eq!(d.totais.produtos, Dinheiro::reais(50));
        assert_eq!(d.totais.desconto, Dinheiro::reais(5));
        assert_eq!(d.totais.total, Dinheiro::reais(45));
        assert_eq!(d.itens[0].cst, "0102");
        assert_eq!(d.duplicatas.len(), 1);
        assert!(d.homologacao);
        assert!(!cardeal_pdf::gerar_danfe(&d).is_empty());
        // Só a NF-e autorizada; a NFC-e tem o cupom.
        assert!(montar(&detalhe_da_nfe(EstadoDocumento::Pendente))
            .unwrap_err()
            .contains("autorizada"));
        let mut nfce = detalhe_da_nfe(EstadoDocumento::Autorizada);
        nfce.resumo.modelo = ModeloFiscal::Nfce;
        assert!(montar(&nfce).unwrap_err().contains("cupom fiscal"));
    }

    #[test]
    fn a_data_de_emissao_sai_do_xml_e_o_cep_ganha_o_traco() {
        let i = emissao_do_xml("<ide><dhEmi>2026-10-27T12:53:07-03:00</dhEmi></ide>").unwrap();
        assert_eq!(i.data(FUSO), Data::de_ymd(2026, 10, 27).unwrap());
        assert_eq!(i.hora(FUSO).formatar_com_segundos(), "12:53:07");
        assert_eq!(emissao_do_xml("<ide/>"), None);
        assert_eq!(cep("95020000"), "95020-000");
        assert_eq!(cep("123"), "123");
    }
}
