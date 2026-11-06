//! Modelos de plano de contas por segmento: contas **a mais** que se juntam ao plano gerencial
//! padrão (`plano_padrao`, semeado em toda empresa), para quem quer ver aluguel, energia,
//! pró-labore ou serviços de bancada separados na DRE em vez de tudo numa linha só.
//!
//! Aplicar um modelo é idempotente (cria só o que falta, por código) e nunca mexe em conta
//! existente. As contas novas ficam sob as sintéticas `4` (Receitas) e `5` (Custos e Despesas),
//! numeradas a partir de `4.20` e `5.20` para não colidir com o plano base; as de receita levam
//! o papel da receita que representam (vendas ou serviços), para a DRE agrupá-las certo.
//!
//! O modelo **Gestão financeira** é diferente: em vez de contas soltas, a árvore que o dono que
//! controla o caixa numa planilha conhece — agrupadoras ("Despesas administrativas de estrutura",
//! "Despesas com veículos", "Impostos sobre o faturamento"…) com as contas dentro, o mesmo recorte
//! do controle financeiro que serviu de base ao DFC gerencial. A agrupadora leva o papel que as
//! filhas herdam (impostos como dedução, pessoal, despesas financeiras).

use serde::{Deserialize, Serialize};

use crate::conta::{Natureza, PapelConta};

/// O segmento do negócio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModeloDePlano {
    /// Comércio varejista ou atacadista.
    Comercio,
    /// Prestação de serviços.
    Servicos,
    /// Assistência técnica e oficina (peças + serviço de bancada).
    AssistenciaTecnica,
    /// A árvore completa de quem controla o caixa por categoria: agrupadoras com as contas
    /// dentro, como no controle financeiro (receitas, impostos, custos, despesas por natureza,
    /// pessoal e financeiras).
    GestaoFinanceira,
}

impl ModeloDePlano {
    /// Todos, na ordem em que a tela os mostra.
    pub const TODOS: [Self; 4] = [
        Self::GestaoFinanceira,
        Self::Comercio,
        Self::Servicos,
        Self::AssistenciaTecnica,
    ];

    /// O nome para o usuário.
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Comercio => "Comércio",
            Self::Servicos => "Prestação de serviços",
            Self::AssistenciaTecnica => "Assistência técnica / oficina",
            Self::GestaoFinanceira => "Gestão financeira completa",
        }
    }

    /// Uma frase sobre o que o modelo acrescenta.
    #[must_use]
    pub const fn descricao(self) -> &'static str {
        match self {
            Self::Comercio => {
                "Despesas do dia a dia separadas (aluguel, energia, pró-labore, marketing…) \
                 mais fretes, embalagens, comissões e taxas de marketplace."
            }
            Self::Servicos => {
                "As despesas do dia a dia separadas, receitas de serviços recorrentes e avulsos, \
                 subcontratação e viagens."
            }
            Self::AssistenciaTecnica => {
                "As despesas do dia a dia separadas, receitas de bancada e de peças, e custos de \
                 peças, ferramentas, garantia e laboratório terceirizado."
            }
            Self::GestaoFinanceira => {
                "A árvore de quem controla o caixa por categoria: impostos sobre o faturamento, \
                 custos, despesas de estrutura, serviços profissionais, veículos, marketing, \
                 pessoal e financeiras, cada grupo com as suas contas — e as categorias de mesmo \
                 nome, já ligadas. Empréstimos, aportes, retiradas e imobilizado já estão no plano."
            }
        }
    }
}

/// Uma conta de um modelo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContaDeModelo {
    /// O código (`"5.20"`); o pai é o código sem o último segmento.
    pub codigo: &'static str,
    /// O nome.
    pub nome: &'static str,
    /// Receita ou despesa.
    pub natureza: Natureza,
    /// O papel, só nas contas de receita que a DRE deve agrupar (e nas agrupadoras cujo papel as
    /// filhas herdam).
    pub papel: Option<PapelConta>,
    /// Agrupadora (recebe filhas, não lançamento).
    pub agrupadora: bool,
}

const fn despesa(codigo: &'static str, nome: &'static str) -> ContaDeModelo {
    ContaDeModelo {
        codigo,
        nome,
        natureza: Natureza::Despesa,
        papel: None,
        agrupadora: false,
    }
}

const fn receita(codigo: &'static str, nome: &'static str, papel: PapelConta) -> ContaDeModelo {
    ContaDeModelo {
        codigo,
        nome,
        natureza: Natureza::Receita,
        papel: Some(papel),
        agrupadora: false,
    }
}

const fn grupo(
    codigo: &'static str,
    nome: &'static str,
    natureza: Natureza,
    papel: Option<PapelConta>,
) -> ContaDeModelo {
    ContaDeModelo {
        codigo,
        nome,
        natureza,
        papel,
        agrupadora: true,
    }
}

const fn filha_receita(codigo: &'static str, nome: &'static str) -> ContaDeModelo {
    ContaDeModelo {
        codigo,
        nome,
        natureza: Natureza::Receita,
        papel: None,
        agrupadora: false,
    }
}

/// As despesas comuns a todo segmento.
const COMUNS: &[ContaDeModelo] = &[
    despesa("5.20", "Aluguel"),
    despesa("5.21", "Energia elétrica"),
    despesa("5.22", "Água"),
    despesa("5.23", "Internet e telefone"),
    despesa("5.24", "Pró-labore"),
    despesa("5.25", "Salários e encargos"),
    despesa("5.26", "Contador"),
    despesa("5.27", "Marketing"),
    despesa("5.28", "Sistemas e assinaturas"),
    despesa("5.29", "Manutenção"),
    despesa("5.30", "Transporte e combustível"),
    despesa("5.31", "Material de escritório e limpeza"),
    despesa("5.32", "Tarifas bancárias"),
    despesa("5.33", "Seguros"),
    despesa("5.34", "Impostos e taxas"),
];

const COMERCIO: &[ContaDeModelo] = &[
    despesa("5.40", "Fretes sobre compras"),
    despesa("5.41", "Embalagens"),
    despesa("5.42", "Comissões de vendedores"),
    despesa("5.43", "Taxas de marketplace"),
];

const SERVICOS: &[ContaDeModelo] = &[
    receita(
        "4.20",
        "Serviços recorrentes (mensalidades)",
        PapelConta::ReceitaServicos,
    ),
    receita("4.21", "Serviços avulsos", PapelConta::ReceitaServicos),
    despesa("5.40", "Subcontratação de terceiros"),
    despesa("5.41", "Viagens e hospedagem"),
];

const ASSISTENCIA: &[ContaDeModelo] = &[
    receita("4.22", "Serviços de bancada", PapelConta::ReceitaServicos),
    receita(
        "4.23",
        "Venda de peças e acessórios",
        PapelConta::ReceitaVendas,
    ),
    despesa("5.40", "Peças e componentes para reparo"),
    despesa("5.41", "Ferramentas e consumíveis (solda, fluxo)"),
    despesa("5.42", "Garantia e retrabalho"),
    despesa("5.43", "Laboratório terceirizado"),
];

/// A árvore de quem controla o caixa por categoria (o mesmo recorte do controle financeiro que
/// serviu de base ao DFC gerencial). As agrupadoras vêm antes das filhas.
const GESTAO: &[ContaDeModelo] = &[
    receita("4.24", "Venda de produção", PapelConta::ReceitaVendas),
    grupo(
        "4.30",
        "Rendimentos e juros recebidos",
        Natureza::Receita,
        Some(PapelConta::ReceitaFinanceira),
    ),
    filha_receita("4.30.01", "Juros cobrados"),
    filha_receita("4.30.02", "Rendimento de aplicações financeiras"),
    grupo(
        "5.50",
        "Impostos sobre o faturamento",
        Natureza::Despesa,
        Some(PapelConta::ImpostosSobreVenda),
    ),
    despesa("5.50.01", "Simples Nacional"),
    despesa("5.50.02", "ICMS"),
    despesa("5.50.03", "ISS"),
    despesa("5.50.04", "PIS"),
    despesa("5.50.05", "COFINS"),
    despesa("5.50.06", "IRPJ"),
    despesa("5.50.07", "CSLL"),
    grupo(
        "5.51",
        "Custos dos produtos e serviços",
        Natureza::Despesa,
        Some(PapelConta::Cmv),
    ),
    despesa("5.51.01", "Compra de matéria-prima"),
    despesa("5.51.02", "Compra de mercadorias para revenda"),
    despesa("5.51.03", "Compra de embalagens"),
    despesa("5.51.04", "Fretes sobre compras"),
    despesa("5.51.05", "Fretes sobre vendas"),
    grupo(
        "5.52",
        "Despesas administrativas de estrutura",
        Natureza::Despesa,
        None,
    ),
    despesa("5.52.01", "Aluguel"),
    despesa("5.52.02", "Água"),
    despesa("5.52.03", "Energia elétrica"),
    despesa("5.52.04", "Telefone / Celular"),
    despesa("5.52.05", "Internet"),
    despesa("5.52.06", "Manutenção de imóvel"),
    despesa("5.52.07", "Taxas / Alvarás / Licenças"),
    despesa("5.52.08", "Seguro predial"),
    despesa("5.52.09", "IPTU"),
    grupo("5.53", "Serviços profissionais", Natureza::Despesa, None),
    despesa("5.53.01", "Honorários do contador"),
    despesa("5.53.02", "Honorários de consultoria"),
    despesa("5.53.03", "Honorários de advogado"),
    despesa("5.53.04", "Segurança do trabalho"),
    grupo(
        "5.54",
        "Outras despesas administrativas",
        Natureza::Despesa,
        None,
    ),
    despesa("5.54.01", "Sistemas"),
    despesa("5.54.02", "Materiais de expediente"),
    despesa("5.54.03", "Materiais de uso e consumo"),
    despesa("5.54.04", "Manutenção de equipamentos"),
    despesa("5.54.05", "Cartório"),
    despesa("5.54.06", "Eventos"),
    despesa("5.54.07", "Brindes / Doações"),
    despesa("5.54.08", "ARTs"),
    grupo("5.55", "Despesas com veículos", Natureza::Despesa, None),
    despesa("5.55.01", "Combustíveis"),
    despesa("5.55.02", "Manutenção de veículos"),
    despesa("5.55.03", "Pneus"),
    despesa("5.55.04", "IPVA / Despachante"),
    despesa("5.55.05", "Seguro de veículos"),
    despesa("5.55.06", "Multas"),
    despesa("5.55.07", "Lavação"),
    grupo(
        "5.56",
        "Marketing",
        Natureza::Despesa,
        Some(PapelConta::DespesaComercial),
    ),
    despesa("5.56.01", "Publicidade e propaganda"),
    despesa("5.56.02", "Assinatura de revistas e jornais"),
    grupo(
        "5.57",
        "Despesas com pessoal (detalhe)",
        Natureza::Despesa,
        Some(PapelConta::DespesaPessoal),
    ),
    despesa("5.57.01", "Salários"),
    despesa("5.57.02", "Pró-labore"),
    despesa("5.57.03", "Comissões"),
    despesa("5.57.04", "Décimo terceiro salário"),
    despesa("5.57.05", "Férias"),
    despesa("5.57.06", "Rescisões"),
    despesa("5.57.07", "FGTS"),
    despesa("5.57.08", "INSS"),
    despesa("5.57.09", "IRRF e outros impostos sobre a folha"),
    despesa("5.57.10", "Adiantamento de salário"),
    despesa("5.57.11", "Plano de saúde"),
    despesa("5.57.12", "Seguro de vida"),
    despesa("5.57.13", "Medicina ocupacional"),
    despesa("5.57.14", "EPIs"),
    despesa("5.57.15", "Uniformes"),
    despesa("5.57.16", "Treinamentos"),
    despesa("5.57.17", "Viagens / Hospedagem / Alimentação"),
    despesa("5.57.18", "Ações trabalhistas"),
    grupo(
        "5.58",
        "Despesas financeiras e bancárias",
        Natureza::Despesa,
        Some(PapelConta::DespesaFinanceira),
    ),
    despesa("5.58.01", "Despesas bancárias"),
    despesa("5.58.02", "Tarifa de cartão"),
    despesa("5.58.03", "Juros e multas sobre tributos"),
    despesa("5.58.04", "IOF"),
    despesa("5.58.05", "Juros sobre parcelas de empréstimos"),
    despesa("5.58.06", "Juros de limite de conta"),
    despesa("5.58.07", "Juros de fornecedores"),
    despesa("5.58.08", "Juros de custódias"),
    despesa("5.58.09", "Inadimplência"),
];

/// As contas que o modelo acrescenta ao plano base (as comuns primeiro; na gestão financeira, a
/// árvore própria, que já tem as do dia a dia).
#[must_use]
pub fn contas_do_modelo(modelo: ModeloDePlano) -> Vec<ContaDeModelo> {
    let proprias = match modelo {
        ModeloDePlano::Comercio => COMERCIO,
        ModeloDePlano::Servicos => SERVICOS,
        ModeloDePlano::AssistenciaTecnica => ASSISTENCIA,
        ModeloDePlano::GestaoFinanceira => return GESTAO.to_vec(),
    };
    COMUNS.iter().chain(proprias).copied().collect()
}

#[cfg(test)]
mod testes {
    use std::collections::HashSet;

    use super::*;
    use crate::plano::plano_padrao;

    #[test]
    fn nenhum_modelo_colide_com_o_plano_base_nem_consigo_mesmo() {
        let base: HashSet<String> = plano_padrao()
            .iter()
            .map(|c| c.codigo.como_str().to_owned())
            .collect();
        for m in ModeloDePlano::TODOS {
            let contas = contas_do_modelo(m);
            let mut vistos = HashSet::new();
            for c in &contas {
                assert!(
                    !base.contains(c.codigo),
                    "{m:?}: {} já existe no plano base",
                    c.codigo
                );
                assert!(
                    vistos.insert(c.codigo),
                    "{m:?}: código repetido {}",
                    c.codigo
                );
                // O pai (sem o último segmento) é uma sintética do plano base ou uma agrupadora
                // do próprio modelo, que vem antes.
                let pai = &c.codigo[..c.codigo.rfind('.').unwrap()];
                let pai_do_modelo = contas
                    .iter()
                    .take_while(|x| x.codigo != c.codigo)
                    .any(|x| x.agrupadora && x.codigo == pai);
                assert!(
                    base.contains(pai) || pai_do_modelo,
                    "{m:?}: pai {pai} de {} não existe",
                    c.codigo
                );
                let esperado = if c.codigo.starts_with('4') {
                    Natureza::Receita
                } else {
                    Natureza::Despesa
                };
                assert_eq!(c.natureza, esperado, "{}", c.codigo);
            }
        }
    }

    #[test]
    fn receitas_do_modelo_levam_papel_e_despesas_nao() {
        for m in ModeloDePlano::TODOS {
            let contas = contas_do_modelo(m);
            for c in contas.iter().filter(|c| !c.agrupadora) {
                // O papel é da própria conta ou da agrupadora em que ela está.
                let pai = &c.codigo[..c.codigo.rfind('.').unwrap()];
                let do_pai = contas
                    .iter()
                    .find(|x| x.agrupadora && x.codigo == pai)
                    .and_then(|x| x.papel);
                if c.natureza == Natureza::Receita {
                    assert!(c.papel.or(do_pai).is_some(), "{}", c.codigo);
                } else {
                    assert!(c.papel.is_none(), "{}", c.codigo);
                }
            }
        }
        // A gestão financeira tem a árvore: agrupadoras com filhas, até o 3º nível.
        let g = contas_do_modelo(ModeloDePlano::GestaoFinanceira);
        assert!(g.iter().filter(|c| c.agrupadora).count() >= 9);
        assert!(g
            .iter()
            .any(|c| c.nome == "Aluguel" && c.codigo == "5.52.01"));
        assert!(g.iter().all(|c| c.codigo.matches('.').count() <= 2));
        assert!(contas_do_modelo(ModeloDePlano::Comercio)
            .iter()
            .any(|c| c.nome == "Aluguel"));
    }
}
