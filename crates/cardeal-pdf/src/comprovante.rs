//! O comprovante de venda — o recibo **não fiscal** do PDV, para quem não emite NFC-e (o MEI
//! vendendo para pessoa física, por exemplo) ou quer um recibo além do DANFCE. Bobina de 80 ou
//! 58 mm, altura do tamanho da venda; traz "NÃO É DOCUMENTO FISCAL" logo abaixo do título, como
//! a lei exige de todo documento que não seja o fiscal.

use cardeal_kernel::{Dinheiro, Fuso, Instante, Preco, Quantidade};

use crate::bobina::{imprimir, Bobina, Cursor};
use crate::fonte::Fonte;

/// Uma linha do comprovante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemComprovante {
    /// Descrição.
    pub descricao: String,
    /// Unidade ("UN", "KG").
    pub unidade: String,
    /// Quantidade.
    pub quantidade: Quantidade,
    /// Preço unitário.
    pub valor_unitario: Preco,
    /// Desconto do item.
    pub desconto: Dinheiro,
    /// Total do item (já com o desconto).
    pub total: Dinheiro,
}

/// Tudo o que vai impresso no comprovante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComprovantePdf {
    /// O nome no topo (o fantasia, ou a razão social).
    pub nome: String,
    /// A razão social, quando é diferente do nome.
    pub razao_social: Option<String>,
    /// O CNPJ, formatado.
    pub cnpj: String,
    /// O endereço numa linha, se houver.
    pub endereco: Option<String>,
    /// O número da venda.
    pub numero: i64,
    /// Quando.
    pub emissao: Instante,
    /// O cliente identificado.
    pub cliente: Option<String>,
    /// Os itens.
    pub itens: Vec<ItemComprovante>,
    /// Soma bruta.
    pub subtotal: Dinheiro,
    /// Descontos.
    pub desconto: Dinheiro,
    /// Total.
    pub total: Dinheiro,
    /// Formas de pagamento e valores.
    pub pagamentos: Vec<(String, Dinheiro)>,
    /// Troco (só na primeira impressão — a reimpressão não sabe quanto o cliente deu).
    pub troco: Dinheiro,
    /// O texto livre do rodapé ("Trocas em até 7 dias com este comprovante").
    pub rodape: Option<String>,
    /// Reimpressão.
    pub segunda_via: bool,
    /// A venda foi estornada — o motivo.
    pub estornada: Option<String>,
    /// Venda a prazo: a linha para o cliente assinar (reconhece a dívida).
    pub assinatura: bool,
    /// O fuso das datas.
    pub fuso: Fuso,
}

fn compor(c: &mut Cursor, d: &ComprovantePdf) {
    let (corpo, pequeno) = (c.b.corpo(), c.b.pequeno());
    c.centro(&d.nome, Fonte::Negrito, c.b.titulo());
    if let Some(r) = &d.razao_social {
        c.centro(r, Fonte::Regular, pequeno);
    }
    c.centro(&format!("CNPJ {}", d.cnpj), Fonte::Regular, pequeno);
    if let Some(e) = &d.endereco {
        c.centro(e, Fonte::Regular, pequeno);
    }
    c.tracejado();
    c.centro("COMPROVANTE DE VENDA", Fonte::Negrito, corpo);
    c.centro("NÃO É DOCUMENTO FISCAL", Fonte::Negrito, corpo);
    c.centro(
        &format!(
            "Venda nº {} · {} {}",
            d.numero,
            d.emissao.data(d.fuso).formatar(),
            d.emissao.hora(d.fuso).formatar()
        ),
        Fonte::Regular,
        pequeno,
    );
    if d.segunda_via {
        c.centro("2ª VIA", Fonte::Negrito, pequeno);
    }
    if let Some(motivo) = &d.estornada {
        c.espaco(0.8);
        c.centro("VENDA ESTORNADA", Fonte::Negrito, corpo);
        c.centro(motivo, Fonte::Regular, pequeno);
    }
    if let Some(cliente) = &d.cliente {
        c.espaco(0.6);
        c.esquerda(&format!("Cliente: {cliente}"), Fonte::Regular, pequeno);
    }
    c.tracejado();

    for item in &d.itens {
        c.esquerda(&item.descricao, Fonte::Regular, corpo);
        c.par(
            &format!(
                "   {} {} x {}",
                item.quantidade.formatar(0),
                item.unidade,
                item.valor_unitario.formatar()
            ),
            &(item.total + item.desconto).formatar(),
            Fonte::Regular,
        );
        if !item.desconto.e_zero() {
            c.par(
                "   desconto",
                &format!("-{}", item.desconto.formatar()),
                Fonte::Regular,
            );
        }
    }
    c.tracejado();

    if !d.desconto.e_zero() {
        c.par("Subtotal", &d.subtotal.formatar(), Fonte::Regular);
        c.par(
            "Descontos",
            &format!("-{}", d.desconto.formatar()),
            Fonte::Regular,
        );
    }
    c.par("TOTAL R$", &d.total.formatar(), Fonte::Negrito);
    c.espaco(0.8);
    for (forma, valor) in &d.pagamentos {
        c.par(forma, &valor.formatar(), Fonte::Regular);
    }
    if !d.troco.e_zero() {
        c.par("Troco", &d.troco.formatar(), Fonte::Regular);
    }
    c.tracejado();
    if d.assinatura {
        c.espaco(8.0);
        c.centro("_______________________________", Fonte::Regular, corpo);
        c.centro(
            d.cliente.as_deref().unwrap_or("Assinatura do cliente"),
            Fonte::Regular,
            pequeno,
        );
        c.centro("Reconheço a dívida acima", Fonte::Regular, pequeno);
        c.espaco(1.5);
    }
    if let Some(r) = d.rodape.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        c.centro(r, Fonte::Regular, pequeno);
    }
    c.centro("Obrigado pela preferência!", Fonte::Negrito, pequeno);
}

/// Gera o PDF do comprovante na bobina dada.
#[must_use]
pub fn gerar_comprovante(d: &ComprovantePdf, bobina: Bobina) -> Vec<u8> {
    imprimir(bobina, |c| compor(c, d))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn exemplo() -> ComprovantePdf {
        ComprovantePdf {
            nome: "Loja do Zé".into(),
            razao_social: Some("JOSE DA SILVA 52998224725".into()),
            cnpj: "11.222.333/0001-81".into(),
            endereco: Some("Rua A, 1 - Centro, Porto Alegre/RS".into()),
            numero: 42,
            emissao: Instante::agora(),
            cliente: Some("Ana Lima".into()),
            itens: vec![
                ItemComprovante {
                    descricao: "Cabo USB-C trançado de 2 metros com carga rápida".into(),
                    unidade: "UN".into(),
                    quantidade: Quantidade::unidades(2),
                    valor_unitario: Preco::centavos(2_990),
                    desconto: Dinheiro::centavos(580),
                    total: Dinheiro::centavos(5_400),
                },
                ItemComprovante {
                    descricao: "Película".into(),
                    unidade: "UN".into(),
                    quantidade: Quantidade::unidades(1),
                    valor_unitario: Preco::centavos(1_500),
                    desconto: Dinheiro::ZERO,
                    total: Dinheiro::centavos(1_500),
                },
            ],
            subtotal: Dinheiro::centavos(7_480),
            desconto: Dinheiro::centavos(580),
            total: Dinheiro::centavos(6_900),
            pagamentos: vec![("Dinheiro".into(), Dinheiro::centavos(6_900))],
            troco: Dinheiro::centavos(3_100),
            rodape: Some("Trocas em até 7 dias com este comprovante".into()),
            segunda_via: false,
            estornada: None,
            assinatura: false,
            fuso: Fuso::BRASILIA,
        }
    }

    #[test]
    fn comprovante_diz_que_nao_e_fiscal_nas_duas_bobinas() {
        for bobina in [Bobina::Mm80, Bobina::Mm58] {
            let pdf = gerar_comprovante(&exemplo(), bobina);
            // Para ver: CARDEAL_COMPROVANTE_SAIDA=caminho.pdf cargo test -p cardeal-pdf comprovante
            if let Ok(destino) = std::env::var("CARDEAL_COMPROVANTE_SAIDA") {
                std::fs::write(destino, &pdf).unwrap();
            }
            let texto = String::from_utf8_lossy(&pdf);
            assert!(texto.starts_with("%PDF-1.7"));
            assert!(texto.contains("COMPROVANTE DE VENDA"));
            assert!(texto.contains("DOCUMENTO FISCAL"));
            assert!(texto.contains("Troco"));
            assert!(texto.contains("Trocas em"));
        }
    }

    #[test]
    fn segunda_via_e_venda_estornada_aparecem() {
        let mut d = exemplo();
        d.segunda_via = true;
        d.troco = Dinheiro::ZERO;
        d.estornada = Some("Cliente desistiu da compra".into());
        d.assinatura = true;
        let texto = String::from_utf8_lossy(&gerar_comprovante(&d, Bobina::Mm58)).into_owned();
        assert!(texto.contains("VIA"));
        assert!(texto.contains("VENDA ESTORNADA"));
        assert!(texto.contains("Reconhe"), "a linha de assinatura do fiado");
        assert!(!texto.contains("Troco"));
    }
}
