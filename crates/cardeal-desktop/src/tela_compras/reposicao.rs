//! O **radar de reposição** que o pedido de compra puxa: o que falta para o estoque voltar ao
//! ponto de pedido **mais** o que as OS estão esperando, menos o disponível e o que já vem a
//! caminho. As peças das OS entram ligadas à linha (`PecaDaOsNoPedido`): a nota do pedido as
//! guarda para cada OS.

use std::collections::HashMap;

use cardeal_kernel::{Id, Quantidade};
use mod_compras::PecaDaOsNoPedido;

/// Um produto abaixo do ponto de pedido.
#[derive(Debug, Clone, Copy)]
pub(super) struct AbaixoDoPonto {
    pub(super) produto: Id,
    pub(super) ponto_pedido: Quantidade,
    pub(super) disponivel: Quantidade,
}

/// Uma peça de OS esperando compra (sem encomenda).
#[derive(Debug, Clone, Copy)]
pub(super) struct PecaEsperando {
    pub(super) item_peca: Id,
    pub(super) produto: Id,
    /// Quanto ainda falta para ela (pedida menos o que o estoque já guarda para ela).
    pub(super) falta: Quantidade,
    /// O disponível do produto agora (soma dos locais).
    pub(super) disponivel: Quantidade,
}

/// Uma linha sugerida: o produto, quanto pedir e as peças de OS que ela atende.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Sugestao {
    pub(super) produto: Id,
    pub(super) quantidade: Quantidade,
    pub(super) para_os: Vec<PecaDaOsNoPedido>,
}

/// Junta o estoque abaixo do ponto e as peças das OS por produto. Para cada produto:
/// `falta = ponto de pedido (se abaixo) + o que as OS esperam − disponível − a caminho`. As peças
/// entram ligadas enquanto couberem na quantidade sugerida (o que o disponível cobre não precisa de
/// compra). Ordem: a dos produtos do radar, depois os que só as OS pedem.
pub(super) fn sugerir(
    abaixo: &[AbaixoDoPonto],
    pecas: &[PecaEsperando],
    a_caminho: &HashMap<Id, Quantidade>,
) -> Vec<Sugestao> {
    let mut ordem: Vec<Id> = abaixo.iter().map(|a| a.produto).collect();
    for p in pecas {
        if !ordem.contains(&p.produto) {
            ordem.push(p.produto);
        }
    }
    ordem
        .into_iter()
        .filter_map(|produto| {
            let radar = abaixo.iter().find(|a| a.produto == produto);
            let das_os: Vec<&PecaEsperando> = pecas
                .iter()
                .filter(|p| p.produto == produto && p.falta.e_positiva())
                .collect();
            let disponivel = radar.map_or_else(
                || das_os.first().map_or(Quantidade::ZERO, |p| p.disponivel),
                |r| r.disponivel,
            );
            let ponto = radar.map_or(Quantidade::ZERO, |r| r.ponto_pedido);
            let os: Quantidade = das_os.iter().map(|p| p.falta).sum();
            let vem = a_caminho.get(&produto).copied().unwrap_or(Quantidade::ZERO);
            let quantidade = ponto + os - disponivel - vem;
            if !quantidade.e_positiva() {
                return None;
            }
            let mut ligadas = Quantidade::ZERO;
            let para_os = das_os
                .into_iter()
                .filter(|p| {
                    let cabe = ligadas + p.falta <= quantidade;
                    if cabe {
                        ligadas += p.falta;
                    }
                    cabe
                })
                .map(|p| PecaDaOsNoPedido {
                    item_peca: p.item_peca,
                    quantidade: p.falta,
                })
                .collect();
            Some(Sugestao {
                produto,
                quantidade,
                para_os,
            })
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn q(n: i64) -> Quantidade {
        Quantidade::unidades(n)
    }

    #[test]
    fn soma_o_ponto_de_pedido_e_as_os_e_liga_as_pecas() {
        let (tela, item1, item2) = (Id::novo(), Id::novo(), Id::novo());
        let s = sugerir(
            &[AbaixoDoPonto {
                produto: tela,
                ponto_pedido: q(5),
                disponivel: q(2),
            }],
            &[
                PecaEsperando {
                    item_peca: item1,
                    produto: tela,
                    falta: q(1),
                    disponivel: q(2),
                },
                PecaEsperando {
                    item_peca: item2,
                    produto: tela,
                    falta: q(1),
                    disponivel: q(2),
                },
            ],
            &HashMap::new(),
        );
        assert_eq!(s.len(), 1);
        assert_eq!(
            s[0].quantidade,
            q(5),
            "5 do ponto + 2 das OS − 2 disponíveis"
        );
        assert_eq!(s[0].para_os.len(), 2);
    }

    #[test]
    fn produto_so_das_os_entra_e_o_a_caminho_desconta() {
        let (cabo, bateria, item, item_b) = (Id::novo(), Id::novo(), Id::novo(), Id::novo());
        let a_caminho = HashMap::from([(bateria, q(1))]);
        let s = sugerir(
            &[],
            &[
                PecaEsperando {
                    item_peca: item,
                    produto: cabo,
                    falta: q(2),
                    disponivel: q(0),
                },
                PecaEsperando {
                    item_peca: item_b,
                    produto: bateria,
                    falta: q(1),
                    disponivel: q(0),
                },
            ],
            &a_caminho,
        );
        assert_eq!(s.len(), 1, "a bateria já vem a caminho");
        assert_eq!(s[0].produto, cabo);
        assert_eq!(s[0].quantidade, q(2));
        assert_eq!(s[0].para_os[0].item_peca, item);
    }

    #[test]
    fn a_peca_que_nao_cabe_na_quantidade_nao_e_ligada() {
        // Disponível 1 cobre uma das duas peças: pede 1 e liga só a primeira.
        let (p, a, b) = (Id::novo(), Id::novo(), Id::novo());
        let s = sugerir(
            &[],
            &[
                PecaEsperando {
                    item_peca: a,
                    produto: p,
                    falta: q(1),
                    disponivel: q(1),
                },
                PecaEsperando {
                    item_peca: b,
                    produto: p,
                    falta: q(1),
                    disponivel: q(1),
                },
            ],
            &HashMap::new(),
        );
        assert_eq!(s[0].quantidade, q(1));
        assert_eq!(s[0].para_os.len(), 1);
        let ligada: Quantidade = s[0].para_os.iter().map(|o| o.quantidade).sum();
        assert!(ligada <= s[0].quantidade, "nunca liga mais que a linha");
    }
}
