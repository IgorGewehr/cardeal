//! Monta a [`Nfce`] de uma venda: itens com o produto e a tributação, pagamentos, troco e o
//! consumidor. Função pura — quem busca os dados é `nfce` (dentro da transação).
//!
//! Tudo o que impede a nota vira uma frase para o usuário (com o item e o que corrigir); a nota
//! não sai pela metade.

use std::collections::HashMap;

use cardeal_fiscal::{
    Consumidor, Emitente, ItemDocumento, MeioPagamento, Nfce, PagamentoDocumento,
};
use cardeal_kernel::{Dinheiro, Id, Preco, Quantidade};
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
    let mut problemas = Vec::new();
    if venda.itens.is_empty() {
        problemas.push("a venda não tem itens".to_string());
    }
    let mut itens = Vec::with_capacity(venda.itens.len());
    for (i, item) in venda.itens.iter().enumerate() {
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
                cfop: t.cfop,
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
