//! O plano de contas gerencial padrão.
//!
//! Semeado na instalação de toda empresa — inclusive MEI, que nunca liga a contabilidade
//! fiscal e mesmo assim precisa de DRE, fluxo de caixa e margem. Ver
//! `docs/05-nucleo-financeiro.md` §3.1.

use crate::conta::{CodigoConta, GrupoFluxo, Natureza, PapelConta, TipoConta};

/// Uma linha do plano padrão, antes de ganhar `Id`. `Contas::semear` (em `cardeal-storage`,
/// quando existir) percorre esta lista, cria cada [`crate::Conta`] e resolve `pai` pelo
/// código. Aqui só o formato de dados — nenhum acesso a banco.
#[derive(Debug, Clone)]
pub struct ContaSemente {
    /// O código hierárquico, ex.: `"1.1.01"`.
    pub codigo: CodigoConta,
    /// O nome exibido.
    pub nome: &'static str,
    /// A natureza contábil.
    pub natureza: Natureza,
    /// Sintética ou analítica.
    pub tipo: TipoConta,
    /// Classificação de fluxo de caixa, se a conta representa disponibilidade.
    pub grupo_fluxo: Option<GrupoFluxo>,
    /// O papel semântico, se algum módulo resolve esta conta por papel.
    pub papel: Option<PapelConta>,
}

const fn linha(
    codigo: &'static str,
    nome: &'static str,
    natureza: Natureza,
    tipo: TipoConta,
) -> Semente {
    Semente {
        codigo,
        nome,
        natureza,
        tipo,
        grupo_fluxo: None,
        papel: None,
    }
}

/// Forma literal, mais compacta que `ContaSemente`, usada só para declarar a tabela
/// abaixo — convertida para `ContaSemente` (que usa `CodigoConta`, não `&str`) em
/// [`plano_padrao`].
struct Semente {
    codigo: &'static str,
    nome: &'static str,
    natureza: Natureza,
    tipo: TipoConta,
    grupo_fluxo: Option<GrupoFluxo>,
    papel: Option<PapelConta>,
}

impl Semente {
    const fn fluxo(mut self, g: GrupoFluxo) -> Self {
        self.grupo_fluxo = Some(g);
        self
    }
    const fn papel(mut self, p: PapelConta) -> Self {
        self.papel = Some(p);
        self
    }
}

/// O plano gerencial padrão completo — `docs/05-nucleo-financeiro.md` §3.1, sem alterações.
///
/// Convertido em tempo de execução (não `const`) porque `CodigoConta` aloca uma `String`;
/// o custo é irrelevante — roda uma vez, na instalação de cada empresa.
#[must_use]
pub fn plano_padrao() -> Vec<ContaSemente> {
    use GrupoFluxo::{Financiamento, Investimento, Operacional};
    use Natureza::{Ativo, Despesa, Passivo, PatrimonioLiquido, Receita};
    use PapelConta as P;
    use TipoConta::{Analitica as A, Sintetica as S};

    #[rustfmt::skip]
    let sementes: Vec<Semente> = vec![
        // ── 1. ATIVO ─────────────────────────────────────────────────────────
        linha("1", "Ativo", Ativo, S),
        linha("1.1", "Disponível", Ativo, S),
        linha("1.1.01", "Caixa", Ativo, A).fluxo(Operacional).papel(P::Caixa),
        linha("1.1.02", "Bancos", Ativo, A).fluxo(Operacional).papel(P::Bancos),
        linha("1.1.03", "Aplicações", Ativo, A).fluxo(Investimento).papel(P::Aplicacoes),
        linha("1.1.04", "Valores em trânsito", Ativo, A).fluxo(Operacional).papel(P::ValoresEmTransito),
        linha("1.2", "Créditos", Ativo, S),
        linha("1.2.01", "Clientes a receber", Ativo, A).papel(P::ClientesAReceber),
        linha("1.2.02", "Cartões a receber", Ativo, A).papel(P::CartoesAReceber),
        linha("1.2.03", "Cheques a receber", Ativo, A).papel(P::ChequesAReceber),
        linha("1.2.04", "Adiantamentos a fornecedores", Ativo, A).papel(P::AdiantamentoFornecedor),
        linha("1.2.05", "Impostos a recuperar", Ativo, A).papel(P::ImpostosARecuperar),
        linha("1.3", "Estoques", Ativo, S),
        linha("1.3.01", "Mercadorias para revenda", Ativo, A).papel(P::EstoqueMercadorias),
        linha("1.3.02", "Matéria-prima", Ativo, A).papel(P::EstoqueMateriaPrima),
        linha("1.3.03", "Produtos em processo", Ativo, A).papel(P::EstoqueEmProcesso),
        linha("1.3.04", "Produtos acabados", Ativo, A).papel(P::EstoqueAcabado),
        linha("1.4", "Imobilizado", Ativo, S),
        linha("1.4.01", "Móveis, máquinas e equipamentos", Ativo, A),
        linha("1.4.02", "Depreciação acumulada", Ativo, A),

        // ── 2. PASSIVO ───────────────────────────────────────────────────────
        linha("2", "Passivo", Passivo, S),
        linha("2.1", "Obrigações de curto prazo", Passivo, S),
        linha("2.1.01", "Fornecedores", Passivo, A).papel(P::Fornecedores),
        linha("2.1.02", "Impostos a recolher", Passivo, A).papel(P::ImpostosARecolher),
        linha("2.1.03", "Salários e encargos", Passivo, A).papel(P::SalariosAPagar),
        linha("2.1.04", "Empréstimos", Passivo, A).fluxo(Financiamento).papel(P::Emprestimos),
        linha("2.1.05", "Adiantamentos de clientes", Passivo, A).papel(P::AdiantamentoCliente),
        linha("2.1.06", "Cartões a repassar (taxas)", Passivo, A).papel(P::CartoesARepassar),

        // ── 3. PATRIMÔNIO LÍQUIDO ────────────────────────────────────────────
        linha("3", "Patrimônio Líquido", PatrimonioLiquido, S),
        linha("3.1", "Capital", PatrimonioLiquido, A).fluxo(Financiamento).papel(P::Capital),
        linha("3.2", "Lucros acumulados", PatrimonioLiquido, A).papel(P::LucrosAcumulados),
        linha("3.3", "Retiradas / pró-labore", PatrimonioLiquido, A).fluxo(Financiamento).papel(P::Retiradas),

        // ── 4. RECEITAS ──────────────────────────────────────────────────────
        linha("4", "Receitas", Receita, S),
        linha("4.1", "Receita de vendas de mercadorias", Receita, A).papel(P::ReceitaVendas),
        linha("4.2", "Receita de serviços", Receita, A).papel(P::ReceitaServicos),
        linha("4.3", "Receita de aluguéis", Receita, A).papel(P::ReceitaAlugueis),
        linha("4.3.01", "Receita de hospedagem", Receita, A).papel(P::ReceitaHospedagem),
        linha("4.4", "Descontos concedidos", Receita, A).papel(P::DescontosConcedidos),
        linha("4.5", "Devoluções de venda", Receita, A).papel(P::DevolucoesVenda),
        linha("4.6", "Receitas financeiras", Receita, A).papel(P::ReceitaFinanceira),
        linha("4.7", "Outras receitas", Receita, A).papel(P::OutrasReceitas),

        // ── 5. CUSTOS E DESPESAS ─────────────────────────────────────────────
        linha("5", "Custos e Despesas", Despesa, S),
        linha("5.1", "CMV — Custo da mercadoria vendida", Despesa, A).papel(P::Cmv),
        linha("5.2", "Custo de serviço prestado", Despesa, A).papel(P::CustoServico),
        linha("5.3", "Despesas com pessoal", Despesa, A).papel(P::DespesaPessoal),
        linha("5.4", "Despesas administrativas", Despesa, A).papel(P::DespesaAdministrativa),
        linha("5.5", "Despesas comerciais", Despesa, A).papel(P::DespesaComercial),
        linha("5.6", "Ocupação (aluguel, energia, água)", Despesa, A).fluxo(Operacional).papel(P::Ocupacao),
        linha("5.7", "Taxas de cartão e meios de pagamento", Despesa, A).papel(P::TaxasCartao),
        linha("5.8", "Impostos sobre venda", Despesa, A).papel(P::ImpostosSobreVenda),
        linha("5.9", "Despesas financeiras", Despesa, A).papel(P::DespesaFinanceira),
        linha("5.10", "Perdas", Despesa, A).papel(P::Perdas),
        linha("5.11", "Quebra de caixa", Despesa, A).papel(P::QuebraCaixa),
    ];

    sementes
        .into_iter()
        .map(|s| ContaSemente {
            codigo: CodigoConta::novo(s.codigo),
            nome: s.nome,
            natureza: s.natureza,
            tipo: s.tipo,
            grupo_fluxo: s.grupo_fluxo,
            papel: s.papel,
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use std::collections::{HashMap, HashSet};

    use super::*;

    #[test]
    fn plano_padrao_e_coerente() {
        let plano = plano_padrao();
        assert!(!plano.is_empty());

        let codigos: HashSet<&str> = plano.iter().map(|c| c.codigo.como_str()).collect();
        // Nenhum código duplicado.
        assert_eq!(
            codigos.len(),
            plano.len(),
            "código de conta duplicado no plano padrão"
        );

        for conta in &plano {
            // Toda conta, exceto as de nível 1, tem pai presente no próprio plano.
            if let Some(pai) = conta.codigo.pai() {
                assert!(
                    codigos.contains(pai.como_str()),
                    "conta {} não tem o pai {} no plano",
                    conta.codigo,
                    pai
                );
            }
            // Nível derivado do código bate com a profundidade real.
            assert_eq!(
                conta.codigo.nivel(),
                u8::try_from(conta.codigo.como_str().split('.').count()).unwrap()
            );
        }

        // Nenhuma conta sintética é analítica ao mesmo tempo (checagem trivial do enum,
        // mas documenta a invariante: só analítica aceita lançamento).
        for conta in &plano {
            if matches!(conta.tipo, TipoConta::Sintetica) {
                // Uma sintética não deveria ter papel semântico — papel resolve conta que
                // recebe partida.
                assert!(
                    conta.papel.is_none(),
                    "conta sintética {} não deveria ter papel semântico",
                    conta.codigo
                );
            }
        }

        // Todo papel semântico aparece no máximo uma vez (senão `Contas::papel` seria ambíguo).
        let mut por_papel: HashMap<String, &str> = HashMap::new();
        for conta in &plano {
            if let Some(p) = conta.papel {
                let chave = format!("{p:?}");
                assert!(
                    por_papel
                        .insert(chave.clone(), conta.codigo.como_str())
                        .is_none(),
                    "papel {chave} duplicado: {} e {}",
                    por_papel[&chave],
                    conta.codigo
                );
            }
        }

        // Os papéis centrais do receituário (doc 05 §5) existem todos.
        for p in [
            PapelConta::Caixa,
            PapelConta::ClientesAReceber,
            PapelConta::Fornecedores,
            PapelConta::ReceitaVendas,
            PapelConta::Cmv,
            PapelConta::EstoqueMercadorias,
            PapelConta::DescontosConcedidos,
            PapelConta::ImpostosARecolher,
        ] {
            assert!(
                plano.iter().any(|c| c.papel == Some(p)),
                "papel {p:?} ausente do plano padrão"
            );
        }
    }
}
