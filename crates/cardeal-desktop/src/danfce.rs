//! O DANFCE de uma NFC-e autorizada: monta o [`cardeal_pdf::DanfcePdf`] a partir da nota
//! guardada no documento fiscal (emitente, itens, pagamentos) e do XML autorizado (QR Code e
//! endereço de consulta), grava numa pasta temporária e abre no visualizador do sistema — de
//! onde se imprime na bobina.

use cardeal_cliente::{Motor, Sessao};
use cardeal_fiscal::{DocumentoConsumidor, MeioPagamento};
use cardeal_kernel::{Dinheiro, Fuso, Id};
use cardeal_pdf::{DanfcePdf, ItemDanfce};
use mod_fiscal::{AmbienteFiscal, DetalheDocumento, DocumentoFiscal, EstadoDocumento};

/// O conteúdo de `<tag>…</tag>` no XML (o `sefaz-api` grava o `infNFeSupl` em texto puro).
fn tag(xml: &str, nome: &str) -> Option<String> {
    let abre = format!("<{nome}>");
    let i = xml.find(&abre)? + abre.len();
    let f = xml[i..].find(&format!("</{nome}>"))? + i;
    let conteudo = xml[i..f].trim();
    let conteudo = conteudo
        .strip_prefix("<![CDATA[")
        .and_then(|c| c.strip_suffix("]]>"))
        .unwrap_or(conteudo);
    Some(conteudo.replace("&amp;", "&")).filter(|c| !c.is_empty())
}

fn meio(m: MeioPagamento) -> &'static str {
    match m {
        MeioPagamento::Dinheiro => "Dinheiro",
        MeioPagamento::CartaoCredito => "Cartão de crédito",
        MeioPagamento::CreditoLoja => "Crédito loja",
        MeioPagamento::CartaoDebito => "Cartão de débito",
        MeioPagamento::Pix => "Pix",
        MeioPagamento::Outros => "Outros",
    }
}

/// O DANFCE de um documento autorizado — ou emitido em contingência, com o aviso e o QR Code
/// da nota assinada.
///
/// # Errors
/// O documento não está autorizado nem em contingência, ou não tem a nota guardada.
pub fn montar(d: &DetalheDocumento) -> Result<DanfcePdf, String> {
    let em_contingencia = d.resumo.estado == EstadoDocumento::Contingencia;
    if d.resumo.estado != EstadoDocumento::Autorizada && !em_contingencia {
        return Err(format!(
            "Só a NFC-e autorizada (ou emitida em contingência) tem cupom fiscal (esta está {})",
            d.resumo.estado.rotulo()
        ));
    }
    let nota = d
        .nota
        .as_ref()
        .ok_or_else(|| "A nota guardada não está legível".to_owned())?;
    let chave = d
        .resumo
        .chave
        .clone()
        .ok_or_else(|| "A nota autorizada está sem a chave de acesso".to_owned())?;
    let e = &nota.emitente;
    let en = &e.endereco;
    let complemento = if en.complemento.is_empty() {
        String::new()
    } else {
        format!(" {}", en.complemento)
    };
    // Em contingência, o QR Code está na nota assinada (o `nfeProc` ainda não veio).
    let xml = if em_contingencia {
        d.xml_assinado.as_deref()
    } else {
        d.xml.as_deref()
    }
    .unwrap_or_default();
    Ok(DanfcePdf {
        razao_social: e.razao_social.clone(),
        cnpj: e.cnpj.formatar(),
        inscricao_estadual: e.inscricao_estadual.clone(),
        endereco: format!(
            "{}, {}{complemento} - {}, {}/{}",
            en.logradouro,
            en.numero,
            en.bairro,
            en.municipio,
            en.uf.sigla()
        ),
        itens: nota
            .itens
            .iter()
            .map(|i| ItemDanfce {
                codigo: i.codigo.clone(),
                descricao: i.descricao.clone(),
                quantidade: i.quantidade,
                unidade: i.unidade.clone(),
                valor_unitario: i.valor_unitario,
                valor_total: i.valor_bruto,
            })
            .collect(),
        valor_total: nota.itens.iter().map(|i| i.valor_bruto).sum(),
        desconto: nota.itens.iter().map(|i| i.desconto).sum::<Dinheiro>(),
        valor_a_pagar: nota.total(),
        pagamentos: nota
            .pagamentos
            .iter()
            .map(|p| (meio(p.meio).to_owned(), p.valor))
            .collect(),
        troco: nota.troco,
        consumidor: nota.consumidor.as_ref().map(|c| match c.documento {
            DocumentoConsumidor::Cpf(cpf) => format!("CPF {}", cpf.formatar()),
            DocumentoConsumidor::Cnpj(cnpj) => format!("CNPJ {}", cnpj.formatar()),
        }),
        numero: d.resumo.numero,
        serie: d.resumo.serie,
        emissao: d.resumo.criado_em,
        chave,
        protocolo: d.resumo.protocolo.clone().filter(|_| !em_contingencia),
        autorizacao: d.resumo.autorizado_em.filter(|_| !em_contingencia),
        url_consulta: tag(xml, "urlChave")
            .unwrap_or_else(|| "o portal da NFC-e da SEFAZ do estado".to_owned()),
        qr_code: tag(xml, "qrCode"),
        homologacao: d.resumo.ambiente == AmbienteFiscal::Homologacao,
        contingencia: d.resumo.contingencia_em.is_some(),
        fuso: Fuso::BRASILIA,
    })
}

/// Gera o DANFCE do documento, grava na pasta temporária e abre no visualizador. Devolve o
/// caminho.
///
/// # Errors
/// Documento inexistente, não autorizado, ou falha ao gerar/gravar.
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
    let pdf = cardeal_pdf::gerar_danfce(
        &montar(&detalhe)?,
        crate::comprovante::bobina_configurada(motor, sessao),
    )
    .map_err(|e| e.to_string())?;
    let pasta = std::env::temp_dir().join("Cardeal").join("cupons");
    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
    let caminho = pasta.join(format!(
        "NFCe-{}-{}.pdf",
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

    fn detalhe(estado: EstadoDocumento) -> DetalheDocumento {
        use cardeal_fiscal::{
            ContribuicaoItem, Crt, Emitente, EnderecoEmitente, IcmsItem, ImpostosItem,
            ItemDocumento, Nfce, PagamentoDocumento,
        };
        use cardeal_kernel::{Cnpj, Instante, Percentual, Preco, Quantidade, Uf};
        let zero = || ContribuicaoItem {
            cst: "49".into(),
            base: Some(Dinheiro::ZERO),
            aliquota: Some(Percentual::ZERO),
            valor: Some(Dinheiro::ZERO),
        };
        let nota = Nfce {
            emitente: Emitente {
                cnpj: Cnpj::novo("11222333000181").unwrap(),
                razao_social: "EMPRESA TESTE LTDA".into(),
                nome_fantasia: "Teste".into(),
                inscricao_estadual: "0960000000".into(),
                inscricao_municipal: None,
                crt: Crt::SimplesNacional,
                endereco: EnderecoEmitente {
                    logradouro: "Rua A".into(),
                    numero: "1".into(),
                    complemento: "Sala 2".into(),
                    bairro: "Centro".into(),
                    codigo_municipio: "4314902".into(),
                    municipio: "Porto Alegre".into(),
                    uf: Uf::Rs,
                    cep: "90010000".into(),
                    telefone: None,
                },
            },
            itens: vec![ItemDocumento {
                codigo: "ABC".into(),
                gtin: None,
                descricao: "Capinha".into(),
                ncm: "39269090".into(),
                cest: None,
                cfop: "5102".into(),
                unidade: "UN".into(),
                quantidade: Quantidade::unidades(2),
                valor_unitario: Preco::reais(25),
                valor_bruto: Dinheiro::reais(50),
                desconto: Dinheiro::reais(5),
                impostos: ImpostosItem {
                    icms: IcmsItem {
                        origem: 0,
                        cst: None,
                        csosn: Some("102".into()),
                        base: None,
                        aliquota: None,
                        valor: None,
                        reducao_base: None,
                    },
                    pis: zero(),
                    cofins: zero(),
                },
            }],
            pagamentos: vec![PagamentoDocumento {
                meio: MeioPagamento::Dinheiro,
                valor: Dinheiro::reais(50),
            }],
            troco: Dinheiro::reais(5),
            consumidor: None,
            serie: 1,
            numero: 7,
            informacoes: None,
        };
        let chave = "4".repeat(44);
        DetalheDocumento {
            resumo: mod_fiscal::DocumentoResumo {
                id: Id::novo(),
                modelo: mod_fiscal::ModeloFiscal::Nfce,
                serie: 1,
                numero: 7,
                ambiente: AmbienteFiscal::Homologacao,
                estado,
                total: Dinheiro::reais(45),
                chave: Some(chave.clone()),
                protocolo: Some("143".into()),
                codigo: Some("100".into()),
                motivo: None,
                origem: Id::novo(),
                tentativas: 1,
                criado_em: Instante::agora(),
                autorizado_em: Some(Instante::agora()),
                pedido: None,
                anulada: false,
                contingencia_em: None,
            },
            nota: Some(nota),
            xml: Some(format!(
                "<nfeProc><infNFeSupl><qrCode>https://x/q?p={chave}|3|2</qrCode><urlChave>www.sefaz.rs.gov.br/nfce/consulta</urlChave></infNFeSupl></nfeProc>"
            )),
            xml_assinado: None,
        }
    }

    #[test]
    fn o_cupom_da_contingencia_tira_o_qr_da_nota_assinada_e_avisa() {
        let mut d = detalhe(EstadoDocumento::Contingencia);
        d.resumo.protocolo = None;
        d.resumo.contingencia_em = Some(cardeal_kernel::Instante::agora());
        d.xml = None;
        d.xml_assinado = Some(
            "<NFe><infNFeSupl><qrCode>https://x/q?p=4|3|2|9|assinado</qrCode>\
             <urlChave>www.sefaz.rs.gov.br/nfce/consulta</urlChave></infNFeSupl></NFe>"
                .into(),
        );
        let pdf = montar(&d).unwrap();
        assert!(pdf.contingencia);
        assert!(pdf.qr_code.unwrap().ends_with("assinado"));
        assert_eq!((pdf.protocolo, pdf.autorizacao), (None, None));
    }

    #[test]
    fn monta_o_cupom_da_nota_autorizada() {
        let d = montar(&detalhe(EstadoDocumento::Autorizada)).unwrap();
        assert_eq!(d.cnpj, "11.222.333/0001-81");
        assert_eq!(d.endereco, "Rua A, 1 Sala 2 - Centro, Porto Alegre/RS");
        assert_eq!(
            (d.valor_total, d.desconto, d.valor_a_pagar),
            (Dinheiro::reais(50), Dinheiro::reais(5), Dinheiro::reais(45))
        );
        assert_eq!(
            d.pagamentos,
            vec![("Dinheiro".to_owned(), Dinheiro::reais(50))]
        );
        assert_eq!(d.troco, Dinheiro::reais(5));
        assert!(d.qr_code.unwrap().contains("|3|2"));
        assert_eq!(d.url_consulta, "www.sefaz.rs.gov.br/nfce/consulta");
        assert!(d.homologacao);
        assert!(cardeal_pdf::gerar_danfce(
            &montar(&detalhe(EstadoDocumento::Autorizada)).unwrap(),
            cardeal_pdf::Bobina::Mm58
        )
        .is_ok());
        assert!(montar(&detalhe(EstadoDocumento::Pendente))
            .unwrap_err()
            .contains("autorizada"));
    }

    #[test]
    fn le_o_qr_e_a_url_do_xml() {
        let xml =
            "<nfeProc><NFe><infNFeSupl><qrCode><![CDATA[https://x.gov.br/q?p=1|3|2]]></qrCode>\
                   <urlChave>www.x.gov.br/consulta</urlChave></infNFeSupl></NFe></nfeProc>";
        assert_eq!(
            tag(xml, "qrCode").as_deref(),
            Some("https://x.gov.br/q?p=1|3|2")
        );
        assert_eq!(
            tag(xml, "urlChave").as_deref(),
            Some("www.x.gov.br/consulta")
        );
        assert_eq!(tag(xml, "nada"), None);
    }
}
