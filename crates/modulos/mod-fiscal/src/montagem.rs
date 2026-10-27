//! Monta a [`Nfce`] de uma venda do PDV e a [`Nfe`] de um pedido de vendas: itens com o produto
//! e a tributação (os mesmos nas duas), pagamentos, o consumidor ou o destinatário. Funções puras
//! — quem busca os dados é `nfce`/`nfe` (dentro da transação).
//!
//! Tudo o que impede a nota vira uma frase para o usuário (com o item e o que corrigir); a nota
//! não sai pela metade.

use std::collections::HashMap;

use cardeal_fiscal::{
    Consumidor, Destinatario, DocumentoConsumidor, Duplicata, Emitente, EnderecoEmitente,
    FinalidadeNfe, IndicadorIe, ItemDocumento, MeioPagamento, Nfce, Nfe, PagamentoDocumento,
};
use cardeal_kernel::{Data, Dinheiro, Id, Preco, Quantidade};
use mod_estoque::ProdutoParaNota;

use crate::tributacao::tributar;

/// Um item da venda, como o PDV o congelou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDaVenda {
    /// O produto.
    pub produto: Id,
    /// A quantidade.
    pub quantidade: Quantidade,
    /// O preço unitário.
    pub preco_unitario: Preco,
    /// O valor bruto (`quantidade × preço`, arredondado).
    pub valor_bruto: Dinheiro,
    /// O desconto do item.
    pub desconto: Dinheiro,
}

/// A venda que vira nota.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DadosDaVenda {
    /// Os itens (os cancelados já fora).
    pub itens: Vec<ItemDaVenda>,
    /// Os pagamentos.
    pub pagamentos: Vec<(MeioPagamento, Dinheiro)>,
    /// O consumidor, se identificado.
    pub consumidor: Option<Consumidor>,
}

/// O código do produto na nota: os 12 últimos dígitos hexadecimais do id (a parte aleatória do
/// `UUIDv7` — o começo é o instante da criação, igual para produtos importados juntos).
fn codigo_do_produto(id: Id) -> String {
    let hex: String = id
        .to_string()
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect();
    hex[hex.len().saturating_sub(12)..].to_ascii_uppercase()
}

/// O que a SEFAZ exige na descrição do primeiro item de uma NFC-e de homologação.
pub const DESCRICAO_HOMOLOGACAO: &str =
    "NOTA FISCAL EMITIDA EM AMBIENTE DE HOMOLOGACAO - SEM VALOR FISCAL";

/// Marca a nota como de homologação (rejeição 373 sem isso): o primeiro item ganha a descrição
/// obrigatória.
pub fn marcar_homologacao(nota: &mut Nfce) {
    if let Some(i) = nota.itens.first_mut() {
        i.descricao = DESCRICAO_HOMOLOGACAO.to_string();
    }
}

/// Monta a nota.
///
/// # Errors
/// As frases de tudo o que impede a nota (todas de uma vez).
pub fn montar_nfce(
    emitente: Emitente,
    venda: &DadosDaVenda,
    produtos: &HashMap<Id, ProdutoParaNota>,
    serie: u32,
    numero: u64,
) -> Result<Nfce, Vec<String>> {
    let (itens, mut problemas) = itens_da_nota(&emitente, &venda.itens, produtos, false);
    if venda.itens.is_empty() {
        problemas.insert(0, "a venda não tem itens".to_string());
    }
    let total: Dinheiro = venda.itens.iter().map(|i| i.valor_bruto - i.desconto).sum();
    let pago: Dinheiro = venda.pagamentos.iter().map(|(_, v)| *v).sum();
    if venda.pagamentos.is_empty() {
        problemas.push("a venda não tem pagamento".to_string());
    } else if pago < total {
        problemas.push(format!(
            "os pagamentos ({}) não cobrem o total ({})",
            pago.formatar_com_simbolo(),
            total.formatar_com_simbolo()
        ));
    }
    if !problemas.is_empty() {
        return Err(problemas);
    }
    Ok(Nfce {
        emitente,
        itens,
        pagamentos: venda
            .pagamentos
            .iter()
            .map(|(meio, valor)| PagamentoDocumento {
                meio: *meio,
                valor: *valor,
            })
            .collect(),
        troco: pago - total,
        consumidor: venda.consumidor.clone(),
        serie,
        numero,
        informacoes: None,
    })
}

/// O CFOP de saída para outra UF: o `5` vira `6` — e a venda de mercadoria com ST já retida
/// (`5405`) vira `6404`, que é o equivalente interestadual.
#[must_use]
pub fn cfop_interestadual(cfop: &str) -> String {
    match cfop {
        "5405" => "6404".to_string(),
        c if c.starts_with('5') => format!("6{}", &c[1..]),
        c => c.to_string(),
    }
}

/// Os itens da nota com a tributação — e os problemas de cada um. `interestadual`: o CFOP vai
/// para o de outra UF.
fn itens_da_nota(
    emitente: &Emitente,
    venda: &[ItemDaVenda],
    produtos: &HashMap<Id, ProdutoParaNota>,
    interestadual: bool,
) -> (Vec<ItemDocumento>, Vec<String>) {
    let mut problemas = Vec::new();
    let mut itens = Vec::with_capacity(venda.len());
    for (i, item) in venda.iter().enumerate() {
        let n = i + 1;
        let Some(p) = produtos.get(&item.produto) else {
            problemas.push(format!("Item {n}: o produto não existe mais no cadastro"));
            continue;
        };
        let rotulo = format!("Item {n} ({})", p.nome);
        let ncm: String = p.ncm.chars().filter(char::is_ascii_digit).collect();
        if ncm.len() != 8 {
            problemas.push(format!(
                "{rotulo}: o produto está sem NCM — preencha no cadastro"
            ));
        }
        let cest = p
            .cest
            .as_ref()
            .map(|c| c.chars().filter(char::is_ascii_digit).collect::<String>());
        if cest.as_ref().is_some_and(|c| c.len() != 7) {
            problemas.push(format!("{rotulo}: o CEST precisa ter 7 dígitos"));
        }
        let liquido = item.valor_bruto - item.desconto;
        match tributar(emitente.crt, &p.fiscal, liquido) {
            Ok(t) => itens.push(ItemDocumento {
                codigo: codigo_do_produto(p.id),
                gtin: p.gtin.clone(),
                descricao: p.nome.trim().chars().take(120).collect(),
                ncm,
                cest,
                cfop: if interestadual {
                    cfop_interestadual(&t.cfop)
                } else {
                    t.cfop
                },
                unidade: p.unidade.trim().chars().take(6).collect(),
                quantidade: item.quantidade,
                valor_unitario: item.preco_unitario,
                valor_bruto: item.valor_bruto,
                desconto: item.desconto,
                impostos: t.impostos,
            }),
            Err(e) => problemas.push(format!("{rotulo}: {e}")),
        }
    }
    (itens, problemas)
}

/// O que a nota de uma ME ou EPP do Simples Nacional precisa dizer (Resolução CGSN 140/2018,
/// art. 59): vai nas informações complementares da NF-e.
pub const TEXTO_DO_SIMPLES: &str =
    "DOCUMENTO EMITIDO POR ME OU EPP OPTANTE PELO SIMPLES NACIONAL. NAO GERA DIREITO A CREDITO FISCAL DE IPI.";

/// Um pedido de vendas que vira NF-e.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DadosDoPedido {
    /// Os itens.
    pub itens: Vec<ItemDaVenda>,
    /// As parcelas a receber, pelo vencimento — vazio quando foi à vista.
    pub parcelas: Vec<(Data, Dinheiro)>,
}

/// O destinatário da NF-e a partir do cadastro do cliente: o documento (CNPJ antes do CPF), a
/// inscrição estadual (contribuinte; "ISENTO" = isento; sem ela, não contribuinte), o endereço
/// principal — com o código IBGE do município achado pelo nome — e o e-mail.
///
/// # Errors
/// O que falta no cadastro, em frases.
pub fn destinatario_do_cliente(
    nome: &str,
    documentos: &[mod_clientes::DocumentoPessoa],
    enderecos: &[mod_clientes::Endereco],
    email: Option<String>,
) -> Result<Destinatario, Vec<String>> {
    use mod_clientes::TipoDocumento;
    let mut problemas = Vec::new();
    let documento = documentos
        .iter()
        .find_map(|d| match d.tipo {
            TipoDocumento::Cnpj => cardeal_kernel::Cnpj::novo(&d.numero)
                .ok()
                .map(DocumentoConsumidor::Cnpj),
            _ => None,
        })
        .or_else(|| {
            documentos.iter().find_map(|d| match d.tipo {
                TipoDocumento::Cpf => cardeal_kernel::Cpf::novo(&d.numero)
                    .ok()
                    .map(DocumentoConsumidor::Cpf),
                _ => None,
            })
        });
    if documento.is_none() {
        problemas.push("o cliente está sem CPF/CNPJ — a NF-e precisa dele".to_string());
    }
    let ie = documentos
        .iter()
        .find(|d| d.tipo == TipoDocumento::Ie)
        .map(|d| d.numero.trim().to_uppercase())
        .filter(|n| !n.is_empty());
    let (indicador_ie, inscricao_estadual) = match ie {
        Some(n) if n == "ISENTO" => (IndicadorIe::Isento, None),
        Some(n) => (
            IndicadorIe::Contribuinte,
            Some(n.chars().filter(char::is_ascii_alphanumeric).collect()),
        ),
        None => (IndicadorIe::NaoContribuinte, None),
    };
    let endereco = enderecos
        .iter()
        .find(|e| e.principal)
        .or_else(|| enderecos.first());
    let endereco = match endereco {
        None => {
            problemas.push("o cliente está sem endereço — a NF-e precisa dele".to_string());
            None
        }
        Some(e) => {
            let codigo = cardeal_kernel::municipio::codigo_do_municipio(e.uf, &e.cidade);
            if codigo.is_none() {
                problemas.push(format!(
                    "a cidade \"{}/{}\" do endereço do cliente não está na tabela do IBGE — \
                     confira o nome",
                    e.cidade.trim(),
                    e.uf.sigla()
                ));
            }
            if e.logradouro.trim().is_empty() || e.bairro.trim().is_empty() {
                problemas.push("o endereço do cliente está sem rua ou bairro".to_string());
            }
            codigo.map(|codigo| EnderecoEmitente {
                logradouro: e.logradouro.trim().chars().take(60).collect(),
                numero: Some(e.numero.trim())
                    .filter(|n| !n.is_empty())
                    .unwrap_or("S/N")
                    .chars()
                    .take(60)
                    .collect(),
                complemento: e
                    .complemento
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or_default()
                    .chars()
                    .take(60)
                    .collect(),
                bairro: e.bairro.trim().chars().take(60).collect(),
                codigo_municipio: codigo.to_string(),
                municipio: cardeal_kernel::municipio::municipio(codigo)
                    .map_or_else(|| e.cidade.trim().to_string(), |(_, n)| n.to_string()),
                uf: e.uf,
                cep: e.cep.chars().filter(char::is_ascii_digit).collect(),
                telefone: None,
            })
        }
    };
    let nome: String = nome.trim().chars().take(60).collect();
    if nome.chars().count() < 2 {
        problemas.push("o cliente está sem nome".to_string());
    }
    match (documento, endereco) {
        (Some(documento), Some(endereco)) if problemas.is_empty() => Ok(Destinatario {
            documento,
            nome,
            indicador_ie,
            inscricao_estadual,
            endereco,
            email: email.filter(|e| e.contains('@')),
        }),
        _ => Err(problemas),
    }
}

/// Monta a NF-e de venda de um pedido.
///
/// Venda para outra UF: o CFOP vai para o interestadual (`6xxx`). No **regime normal** ela ainda
/// não sai — a alíquota interestadual e o DIFAL não estão no cálculo; no Simples Nacional, sim (o
/// ICMS vai no DAS, sem destaque).
///
/// # Errors
/// As frases de tudo o que impede a nota (todas de uma vez).
pub fn montar_nfe(
    emitente: Emitente,
    destinatario: Destinatario,
    pedido: &DadosDoPedido,
    produtos: &HashMap<Id, ProdutoParaNota>,
    serie: u32,
    numero: u64,
) -> Result<Nfe, Vec<String>> {
    let interestadual = destinatario.endereco.uf != emitente.endereco.uf;
    let (itens, mut problemas) = itens_da_nota(&emitente, &pedido.itens, produtos, interestadual);
    if pedido.itens.is_empty() {
        problemas.insert(0, "o pedido não tem itens".to_string());
    }
    if interestadual && !emitente.crt.e_simples() {
        problemas.push(format!(
            "venda para outra UF ({}) no regime normal ainda não sai em NF-e — a alíquota \
             interestadual e o DIFAL não estão no cálculo",
            destinatario.endereco.uf.sigla()
        ));
    }
    let total: Dinheiro = pedido
        .itens
        .iter()
        .map(|i| i.valor_bruto - i.desconto)
        .sum();
    let a_prazo = !pedido.parcelas.is_empty();
    let parcelado: Dinheiro = pedido.parcelas.iter().map(|(_, v)| *v).sum();
    if a_prazo && parcelado != total {
        problemas.push(format!(
            "as parcelas a receber ({}) não fecham com o total da nota ({})",
            parcelado.formatar_com_simbolo(),
            total.formatar_com_simbolo()
        ));
    }
    if !problemas.is_empty() {
        return Err(problemas);
    }
    let consumidor_final = destinatario.indicador_ie != IndicadorIe::Contribuinte;
    let informacoes = emitente
        .crt
        .e_simples()
        .then(|| TEXTO_DO_SIMPLES.to_string());
    Ok(Nfe {
        emitente,
        destinatario,
        itens,
        natureza: "Venda de mercadoria".into(),
        saida: true,
        finalidade: FinalidadeNfe::Normal,
        referencias: Vec::new(),
        consumidor_final,
        presencial: false,
        pagamentos: vec![PagamentoDocumento {
            meio: MeioPagamento::Outros,
            valor: total,
        }],
        a_prazo,
        duplicatas: pedido
            .parcelas
            .iter()
            .enumerate()
            .map(|(i, (vencimento, valor))| Duplicata {
                numero: format!("{:03}", i + 1),
                vencimento: *vencimento,
                valor: *valor,
            })
            .collect(),
        serie,
        numero,
        informacoes,
    })
}

#[cfg(test)]
mod testes {
    use cardeal_fiscal::{Crt, EnderecoEmitente};
    use cardeal_kernel::{Cnpj, Uf};
    use mod_estoque::fiscal::CamposFiscais;

    use super::*;

    fn emitente() -> Emitente {
        Emitente {
            cnpj: Cnpj::novo("11222333000181").unwrap(),
            razao_social: "EMPRESA".into(),
            nome_fantasia: "E".into(),
            inscricao_estadual: "0960000000".into(),
            inscricao_municipal: None,
            crt: Crt::SimplesNacional,
            endereco: EnderecoEmitente {
                logradouro: "Rua A".into(),
                numero: "1".into(),
                complemento: String::new(),
                bairro: "Centro".into(),
                codigo_municipio: "4314902".into(),
                municipio: "Porto Alegre".into(),
                uf: Uf::Rs,
                cep: "90010000".into(),
                telefone: None,
            },
        }
    }

    fn produto(id: Id, nome: &str, ncm: &str) -> ProdutoParaNota {
        ProdutoParaNota {
            id,
            nome: nome.into(),
            ncm: ncm.into(),
            cest: None,
            gtin: None,
            unidade: "UN".into(),
            fiscal: CamposFiscais::default(),
        }
    }

    fn item(produto: Id, centavos: i64, desconto: i64) -> ItemDaVenda {
        ItemDaVenda {
            produto,
            quantidade: Quantidade::unidades(1),
            preco_unitario: Preco::centavos(centavos),
            valor_bruto: Dinheiro::centavos(centavos),
            desconto: Dinheiro::centavos(desconto),
        }
    }

    #[test]
    fn monta_com_troco_e_codigo_estavel() {
        let (a, b) = (Id::novo(), Id::novo());
        let produtos = HashMap::from([
            (a, produto(a, "Cabo", "8544.42.00")),
            (b, produto(b, "Película", "39199090")),
        ]);
        let venda = DadosDaVenda {
            itens: vec![item(a, 3_000, 500), item(b, 1_500, 0)],
            pagamentos: vec![(MeioPagamento::Dinheiro, Dinheiro::reais(50))],
            consumidor: None,
        };
        let nota = montar_nfce(emitente(), &venda, &produtos, 1, 7).unwrap();
        assert_eq!(nota.total(), Dinheiro::centavos(4_000));
        assert_eq!(nota.troco, Dinheiro::reais(10));
        assert_eq!(nota.itens[0].ncm, "85444200");
        assert_eq!(nota.itens[0].codigo.len(), 12);
        assert_ne!(nota.itens[0].codigo, nota.itens[1].codigo);
        assert_eq!(nota.itens[0].cfop, "5102");
        assert_eq!(nota.numero, 7);
    }

    fn documento(tipo: mod_clientes::TipoDocumento, numero: &str) -> mod_clientes::DocumentoPessoa {
        mod_clientes::DocumentoPessoa {
            id: Id::novo(),
            empresa: Id::novo(),
            pessoa: Id::novo(),
            tipo,
            numero: numero.into(),
            orgao_emissor: None,
            validado_sefaz_em: None,
        }
    }

    fn endereco(cidade: &str, uf: Uf) -> mod_clientes::Endereco {
        mod_clientes::Endereco {
            id: Id::novo(),
            pessoa: Id::novo(),
            tipo: mod_clientes::TipoEndereco::Comercial,
            logradouro: "Av. Brasil".into(),
            numero: String::new(),
            complemento: None,
            bairro: "Centro".into(),
            cidade: cidade.into(),
            uf,
            cep: "88010-000".into(),
            principal: true,
        }
    }

    #[test]
    fn o_destinatario_sai_do_cadastro_do_cliente() {
        use mod_clientes::TipoDocumento::{Cnpj, Cpf, Ie};
        let d = destinatario_do_cliente(
            "  CLIENTE LTDA ",
            &[
                documento(Cpf, "52998224725"),
                documento(Cnpj, "11.222.333/0001-81"),
                documento(Ie, "255.123.456"),
            ],
            &[endereco("Florianópolis", Uf::Sc)],
            Some("compras@cliente.com.br".into()),
        )
        .unwrap();
        assert!(
            matches!(d.documento, DocumentoConsumidor::Cnpj(_)),
            "CNPJ antes do CPF"
        );
        assert_eq!(d.nome, "CLIENTE LTDA");
        assert_eq!(d.indicador_ie, IndicadorIe::Contribuinte);
        assert_eq!(d.inscricao_estadual.as_deref(), Some("255123456"));
        assert_eq!(d.endereco.codigo_municipio, "4205407");
        assert_eq!(d.endereco.numero, "S/N");
        assert_eq!(d.endereco.cep, "88010000");
        let isento = destinatario_do_cliente(
            "Fulano",
            &[documento(Cpf, "52998224725"), documento(Ie, "isento")],
            &[endereco("Porto Alegre", Uf::Rs)],
            Some("sem arroba".into()),
        )
        .unwrap();
        assert_eq!(isento.indicador_ie, IndicadorIe::Isento);
        assert_eq!(isento.email, None);
        // O que falta, tudo de uma vez.
        let problemas =
            destinatario_do_cliente("Fulano", &[], &[endereco("Cidade Inventada", Uf::Rs)], None)
                .unwrap_err()
                .join(" | ");
        assert!(problemas.contains("sem CPF/CNPJ"), "{problemas}");
        assert!(problemas.contains("\"Cidade Inventada/RS\""), "{problemas}");
    }

    #[test]
    fn a_venda_para_outra_uf_muda_o_cfop_e_no_regime_normal_nao_sai() {
        assert_eq!(cfop_interestadual("5102"), "6102");
        assert_eq!(cfop_interestadual("5405"), "6404");
        assert_eq!(cfop_interestadual("6102"), "6102");
        let a = Id::novo();
        let produtos = HashMap::from([(a, produto(a, "Cabo", "85444200"))]);
        let pedido = DadosDoPedido {
            itens: vec![item(a, 10_000, 0)],
            parcelas: vec![(
                cardeal_kernel::Data::de_ymd(2026, 12, 1).unwrap(),
                Dinheiro::reais(100),
            )],
        };
        let destinatario = destinatario_do_cliente(
            "CLIENTE LTDA",
            &[documento(
                mod_clientes::TipoDocumento::Cnpj,
                "11222333000181",
            )],
            &[endereco("Florianópolis", Uf::Sc)],
            None,
        )
        .unwrap();
        // Simples, RS → SC: sai, com o CFOP de fora.
        let nota = montar_nfe(emitente(), destinatario.clone(), &pedido, &produtos, 1, 9).unwrap();
        assert_eq!(nota.itens[0].cfop, "6102");
        assert_eq!(nota.informacoes.as_deref(), Some(TEXTO_DO_SIMPLES));
        assert_eq!(nota.duplicatas[0].numero, "001");
        assert!(nota.a_prazo && nota.consumidor_final);
        // Regime normal, para fora: ainda não.
        let mut normal = emitente();
        normal.crt = cardeal_fiscal::Crt::RegimeNormal;
        let problemas = montar_nfe(normal, destinatario.clone(), &pedido, &produtos, 1, 9)
            .unwrap_err()
            .join(" | ");
        assert!(
            problemas.contains("regime normal ainda não sai"),
            "{problemas}"
        );
        // As parcelas que não fecham com o total.
        let errado = DadosDoPedido {
            parcelas: vec![(
                cardeal_kernel::Data::de_ymd(2026, 12, 1).unwrap(),
                Dinheiro::reais(90),
            )],
            ..pedido
        };
        let problemas = montar_nfe(emitente(), destinatario, &errado, &produtos, 1, 9)
            .unwrap_err()
            .join(" | ");
        assert!(problemas.contains("não fecham com o total"), "{problemas}");
    }

    #[test]
    fn junta_todos_os_problemas() {
        let (a, b) = (Id::novo(), Id::novo());
        let mut ruim = produto(b, "Peça", "");
        ruim.fiscal.cst_icms = Some("00".into());
        let produtos = HashMap::from([(a, produto(a, "Cabo", "85444200")), (b, ruim)]);
        let venda = DadosDaVenda {
            itens: vec![
                item(a, 1_000, 0),
                item(b, 1_000, 0),
                item(Id::novo(), 100, 0),
            ],
            pagamentos: vec![(MeioPagamento::Pix, Dinheiro::reais(5))],
            consumidor: None,
        };
        let problemas = montar_nfce(emitente(), &venda, &produtos, 1, 1).unwrap_err();
        let texto = problemas.join(" | ");
        assert!(
            texto.contains("Item 2 (Peça): o produto está sem NCM"),
            "{texto}"
        );
        assert!(
            texto.contains("Item 2 (Peça): o perfil fiscal tem o CST 00"),
            "{texto}"
        );
        assert!(texto.contains("Item 3: o produto não existe"), "{texto}");
        assert!(texto.contains("não cobrem o total"), "{texto}");
    }
}
