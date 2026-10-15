//! O comprovante de venda (não fiscal) do PDV: monta o PDF na bobina configurada (Configurações
//! › Impressão) e abre no visualizador do sistema, de onde vai à impressora — como o DANFCE.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Dinheiro, Fuso, Id};
use mod_pdv::{
    ComprovanteDaVenda, DadosDoComprovante, EstadoCupom, FormaPagamentoPdv, ImpressaoDoPdv,
    ImpressaoPdv, LarguraBobina,
};

/// A bobina do `cardeal-pdf` para a largura configurada no PDV.
#[must_use]
pub fn bobina(l: LarguraBobina) -> cardeal_pdf::Bobina {
    match l {
        LarguraBobina::Mm80 => cardeal_pdf::Bobina::Mm80,
        LarguraBobina::Mm58 => cardeal_pdf::Bobina::Mm58,
    }
}

/// A bobina configurada — 80 mm quando não dá para saber (sem PDV ou sem permissão).
#[must_use]
pub fn bobina_configurada(motor: &Motor, sessao: &Sessao) -> cardeal_pdf::Bobina {
    motor
        .consultar(sessao, "pdv.impressao.v1", &ImpressaoDoPdv)
        .map(|i: ImpressaoPdv| bobina(i.bobina))
        .unwrap_or_default()
}

const fn forma(f: FormaPagamentoPdv) -> &'static str {
    match f {
        FormaPagamentoPdv::Dinheiro => "Dinheiro",
        FormaPagamentoPdv::Pix => "Pix",
        FormaPagamentoPdv::Debito => "Cartão de débito",
        FormaPagamentoPdv::Credito => "Cartão de crédito",
    }
}

/// O que vai impresso. `troco` só se conhece na hora da venda; a reimpressão manda zero.
#[must_use]
pub fn montar(
    d: &DadosDoComprovante,
    troco: Dinheiro,
    segunda_via: bool,
) -> cardeal_pdf::ComprovantePdf {
    cardeal_pdf::ComprovantePdf {
        nome: d.nome.clone(),
        razao_social: (d.razao_social != d.nome).then(|| d.razao_social.clone()),
        cnpj: d.cnpj.clone(),
        endereco: d.endereco.clone(),
        numero: d.numero,
        emissao: d.abertura,
        cliente: d.cliente.clone(),
        itens: d
            .itens
            .iter()
            .map(|i| cardeal_pdf::ItemComprovante {
                descricao: i.descricao.clone(),
                unidade: i.unidade.clone(),
                quantidade: i.quantidade,
                valor_unitario: i.preco_unitario,
                desconto: i.desconto,
                total: i.total,
            })
            .collect(),
        subtotal: d.subtotal,
        desconto: d.desconto,
        total: d.total,
        pagamentos: d
            .pagamentos
            .iter()
            .map(|p| (forma(p.forma).to_owned(), p.valor))
            .collect(),
        troco,
        rodape: d.impressao.rodape.clone(),
        segunda_via,
        estornada: (d.estado == EstadoCupom::Estornado).then(|| {
            d.estorno
                .as_ref()
                .map_or_else(String::new, |e| e.motivo.clone())
        }),
        fuso: Fuso::BRASILIA,
    }
}

/// Monta o comprovante da venda, grava em `%TEMP%\Cardeal\cupons` e abre para imprimir.
///
/// # Errors
/// A mensagem do motor (venda não finalizada, sem permissão) ou de E/S.
pub fn imprimir(
    motor: &Motor,
    sessao: &Sessao,
    cupom: Id,
    troco: Dinheiro,
    segunda_via: bool,
) -> Result<std::path::PathBuf, String> {
    let dados: DadosDoComprovante = motor
        .consultar(
            sessao,
            "pdv.comprovante_da_venda.v1",
            &ComprovanteDaVenda { cupom },
        )
        .map_err(|e| e.mensagem)?;
    let pdf = cardeal_pdf::gerar_comprovante(
        &montar(&dados, troco, segunda_via),
        bobina(dados.impressao.bobina),
    );
    let pasta = std::env::temp_dir().join("Cardeal").join("cupons");
    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
    let caminho = pasta.join(format!("Comprovante-{}.pdf", dados.numero));
    std::fs::write(&caminho, pdf).map_err(|e| e.to_string())?;
    if !cfg!(test) {
        open::that_detached(&caminho).map_err(|e| e.to_string())?;
    }
    Ok(caminho)
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Instante, Preco, Quantidade};

    use super::*;

    fn dados() -> DadosDoComprovante {
        DadosDoComprovante {
            nome: "Loja do Zé".into(),
            razao_social: "JOSE DA SILVA".into(),
            cnpj: "11.222.333/0001-81".into(),
            endereco: None,
            numero: 7,
            abertura: Instante::agora(),
            cliente: None,
            itens: vec![mod_pdv::ItemDoComprovante {
                descricao: "Capinha".into(),
                unidade: "UN".into(),
                quantidade: Quantidade::unidades(1),
                preco_unitario: Preco::reais(25),
                desconto: Dinheiro::ZERO,
                total: Dinheiro::reais(25),
            }],
            subtotal: Dinheiro::reais(25),
            desconto: Dinheiro::ZERO,
            total: Dinheiro::reais(25),
            pagamentos: vec![mod_pdv::PagamentoCupom {
                id: Id::novo(),
                forma: FormaPagamentoPdv::Pix,
                valor: Dinheiro::reais(25),
            }],
            estado: EstadoCupom::Estornado,
            estorno: Some(mod_pdv::EstornoDaVenda {
                motivo: "Cliente desistiu".into(),
                autorizado_por: Id::novo(),
                em: Instante::agora(),
            }),
            impressao: ImpressaoPdv::default(),
        }
    }

    #[test]
    fn monta_razao_so_quando_difere_e_marca_o_estorno() {
        let c = montar(&dados(), Dinheiro::ZERO, true);
        assert_eq!(c.razao_social.as_deref(), Some("JOSE DA SILVA"));
        assert_eq!(c.pagamentos, vec![("Pix".to_owned(), Dinheiro::reais(25))]);
        assert_eq!(c.estornada.as_deref(), Some("Cliente desistiu"));
        assert!(c.segunda_via);
        let mut igual = dados();
        igual.razao_social = igual.nome.clone();
        igual.estado = EstadoCupom::Finalizado;
        let c = montar(&igual, Dinheiro::reais(5), false);
        assert_eq!(
            (c.razao_social, c.estornada, c.troco),
            (None, None, Dinheiro::reais(5))
        );
    }
}
